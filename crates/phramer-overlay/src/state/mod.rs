//! The overlay's behaviour as a pure state machine: events in, effects out.
//!
//! Nothing here touches a window, a clock or the screen. The shell feeds it
//! pointer and key events in the logical coordinates of the monitor they
//! happened on, plus the time, and carries out the [`Effect`]s it returns.
//! The same sequence of events always yields the same effects, which is what
//! lets every interaction be tested by script.
//!
//! The rules are `CaptureWidget` and `SelectionWidget` of 15.0.0; the
//! annotation tools (`annotate`) edit [`OverlayState::document`] through an
//! [`Editor`](phramer_annotate::Editor) with its undo history.

mod annotate;
mod selection;
pub(crate) mod toolbar;
pub mod tools;

use phramer_annotate::{Document, Editor, Size};
use phramer_capture::{LocalRect, LogicalPoint, LogicalRect, Monitor, PhysicalRect, window_at};
use phramer_core::config::{ButtonType, Settings};

use crate::error::OverlayError;
use crate::input::{Bindings, Direction, Key, KeyAction, Modifiers, PointerButton};
pub(crate) use annotate::{HOVER_FADE_MS, disc_color};
pub use annotate::{ShapeOption, ToolbarButton, WheelView};
pub use selection::Side;
use selection::{CLICK_DISTANCE, nudged, resize, side_at, spanned, translated};
pub use tools::{SettingChange, ShapeKind, ToolSettings};

/// Milliseconds on any monotonic clock; only differences matter.
pub type Millis = u64;

/// How long after the last keyboard nudge the selection counts as settled
/// (`setGeometryByKeyboard`'s 400 ms timer).
const KEYBOARD_SETTLE_MS: Millis = 400;

/// Where the selection's `WxH+X+Y` label sits (`showSelectionGeometry`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeometryLabel {
    Hidden,
    TopLeft,
    BottomLeft,
    TopRight,
    BottomRight,
    Center,
}

impl GeometryLabel {
    /// From the stored setting, 0 to 5 (`GeneralConf::xywh_position`).
    pub fn from_setting(value: i32) -> Self {
        match value {
            0 => Self::Hidden,
            1 => Self::TopLeft,
            2 => Self::BottomLeft,
            3 => Self::TopRight,
            4 => Self::BottomRight,
            _ => Self::Center,
        }
    }
}

/// Everything about one capture that is fixed when it starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlayConfig {
    pub bindings: Bindings,
    /// `showHelp`: the key list until a selection exists.
    pub show_help: bool,
    pub geometry_label: GeometryLabel,
    /// `showSelectionGeometryHideTime`; 0 keeps the label up.
    pub geometry_label_timeout_ms: u32,
    /// Close and export as soon as a selection settles (`-s`).
    pub accept_on_select: bool,
    /// The request already says what to do with the capture (`-c`, `-p`,
    /// ...): Enter accepts it, and the copy/save keys are not offered. A
    /// plain capture is the other way round, as 15.0.0's tool buttons were.
    pub has_tasks: bool,
    /// `captureRegionMode` 2: the overlay follows the cursor across
    /// monitors until the first press commits to one.
    pub follow_cursor: bool,
    /// A region to start with, in physical pixels relative to the monitor
    /// (`--region`, `--last-region`). Ignored while following the cursor.
    pub initial_selection: Option<LocalRect>,
    /// The toolbar, colours and tool sizes.
    pub tools: ToolSettings,
    /// The system asks for no animation: hover changes are instant.
    pub reduced_motion: bool,
}

impl OverlayConfig {
    /// The parts that come from the settings file; the request decides the
    /// rest.
    pub fn from_settings(settings: &Settings) -> Self {
        Self {
            bindings: Bindings::from_settings(settings),
            show_help: settings.show_help(),
            geometry_label: GeometryLabel::from_setting(settings.show_selection_geometry()),
            geometry_label_timeout_ms: u32::try_from(settings.show_selection_geometry_hide_time())
                .unwrap_or(0),
            accept_on_select: false,
            has_tasks: false,
            follow_cursor: false,
            initial_selection: None,
            tools: ToolSettings::from_settings(settings),
            reduced_motion: false,
        }
    }
}

impl Default for OverlayConfig {
    fn default() -> Self {
        Self::from_settings(&Settings::default())
    }
}

/// Input, in the logical coordinates of the monitor whose window got it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    PointerMove {
        monitor: usize,
        at: LogicalPoint,
        /// Whether the left button is held.
        left_down: bool,
        modifiers: Modifiers,
    },
    PointerDown {
        monitor: usize,
        at: LogicalPoint,
        button: PointerButton,
        modifiers: Modifiers,
    },
    PointerUp {
        monitor: usize,
        at: LogicalPoint,
        button: PointerButton,
        modifiers: Modifiers,
    },
    Key {
        key: Key,
        modifiers: Modifiers,
    },
    /// A character typed (`WM_CHAR`), for text being entered.
    Text(char),
    /// The mouse wheel turned; `delta` is in `WHEEL_DELTA` units (120 a
    /// notch), positive away from the user.
    Wheel {
        monitor: usize,
        at: LogicalPoint,
        delta: i32,
        modifiers: Modifiers,
    },
    /// A wake-up asked for with [`Effect::WakeAt`] (or any later time).
    Tick,
    /// The overlay gained or lost the keyboard.
    Focus(bool),
}

/// The pointer shape to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CursorShape {
    Cross,
    Arrow,
    /// Top-left to bottom-right diagonal.
    ResizeNwSe,
    /// Top-right to bottom-left diagonal.
    ResizeNeSw,
    ResizeHorizontal,
    ResizeVertical,
    Grabbing,
}

/// What a successful close does with the capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExportAction {
    /// Enter: whatever the request asked for.
    Accept,
    Copy,
    Save,
    /// Save to the save folder, then put the file on the clipboard.
    CopyFile,
}

/// The region to export and how.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Export {
    /// Index into the monitors the overlay was given.
    pub monitor: usize,
    /// Physical pixels relative to that monitor: an index range into its
    /// frame.
    pub region: LocalRect,
    pub action: ExportAction,
}

/// What a toolbar action hands to a part of the application that is not
/// built yet. The overlay closes with the region, as for an export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeferredAction {
    /// Pin the capture on the desktop.
    Pin,
    /// Read the text in the selection (15.0.0 kept the overlay open).
    Ocr,
    /// Open the capture, with its annotations editable, in the editor.
    OpenInEditor,
    /// Open the capture with another application (App Launcher).
    OpenWith,
}

/// The region a deferred action works on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Deferred {
    pub monitor: usize,
    /// Physical pixels relative to that monitor.
    pub region: LocalRect,
    pub action: DeferredAction,
}

/// How the overlay ended. There is exactly one per capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Cancelled,
    Export(Export),
    /// An action whose backend does not exist yet.
    Deferred(Deferred),
}

/// What the shell must do after an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Render and present this monitor's window again.
    Redraw {
        monitor: usize,
    },
    SetCursor(CursorShape),
    /// The cursor took the overlay to another monitor (follow mode).
    SwitchMonitor {
        to: usize,
    },
    /// Follow mode ended: remove every window but the active monitor's.
    ReleaseMonitors,
    /// Send [`Event::Tick`] at this time.
    WakeAt(Millis),
    /// Write a choice back to the settings file now, as 15.0.0 did.
    Persist(SettingChange),
    /// Close every window; export when the outcome says so. Always the last
    /// effect, and never emitted twice.
    Close(Outcome),
}

/// What a pressed left button is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Drag {
    Idle,
    /// Not yet a drag. `window_click`: released here, it picks the
    /// highlighted window.
    Pending {
        start: LogicalPoint,
        window_click: bool,
    },
    Resizing {
        side: Side,
    },
    Moving {
        last: LogicalPoint,
    },
    /// A tool is drawing the object in progress.
    Drawing,
    /// The selected object follows the pointer once it moved past the click
    /// distance.
    MovingObject {
        start: LogicalPoint,
        moved: bool,
    },
    /// A toolbar button (or Shape menu entry) is held.
    Button(usize),
}

/// One capture's overlay.
#[derive(Debug, Clone)]
pub struct OverlayState {
    monitors: Vec<Monitor>,
    /// Visible top-level windows, physical, topmost first.
    windows: Vec<PhysicalRect>,
    config: OverlayConfig,
    active: usize,
    following: bool,
    /// Logical, relative to the active monitor; may reach off it after a
    /// move, and is clipped when exported.
    selection: Option<LogicalRect>,
    /// The window a click would capture, on the active monitor.
    hover: Option<LogicalRect>,
    drag: Drag,
    help_visible: bool,
    label_visible: bool,
    label_hide_at: Option<Millis>,
    settle_at: Option<Millis>,
    /// The wake-up last asked for.
    scheduled: Option<Millis>,
    focused: bool,
    cursor: CursorShape,
    closed: bool,
    /// The time of the event being handled, for fades.
    now: Millis,
    tools: annotate::Tools,
}

/// Collects effects, one redraw per monitor.
#[derive(Default)]
pub(crate) struct Effects(Vec<Effect>);

impl Effects {
    fn push(&mut self, effect: Effect) {
        if !self.0.contains(&effect) {
            self.0.push(effect);
        }
    }

    fn redraw(&mut self, monitor: usize) {
        self.push(Effect::Redraw { monitor });
    }
}

impl OverlayState {
    /// An overlay on `monitors[active]`. `windows` is the snap list (empty
    /// for none).
    pub fn new(
        monitors: Vec<Monitor>,
        active: usize,
        windows: Vec<PhysicalRect>,
        config: OverlayConfig,
    ) -> Result<Self, OverlayError> {
        let monitor = monitors
            .get(active)
            .ok_or(OverlayError::NoSuchMonitor(active))?;
        let editor = Editor::with_undo_limit(document_for(monitor), config.tools.undo_limit);
        let tools = annotate::Tools::new(editor, &config.tools, config.has_tasks);
        let following = config.follow_cursor && monitors.len() > 1;
        let selection = match config.initial_selection {
            Some(local) if !following => monitor.rect_to_logical(monitor.rect_to_physical(local)),
            _ => None,
        };
        Ok(Self {
            help_visible: config.show_help,
            monitors,
            windows,
            active,
            following,
            selection,
            hover: None,
            drag: Drag::Idle,
            label_visible: false,
            label_hide_at: None,
            settle_at: None,
            scheduled: None,
            focused: false,
            cursor: CursorShape::Cross,
            closed: false,
            now: 0,
            tools,
            config,
        })
    }

    /// The effects of opening: draw everything, and with a region given up
    /// front, settle it (which closes at once under accept-on-select).
    pub fn start(&mut self) -> Vec<Effect> {
        let mut effects = Effects::default();
        for monitor in self.window_monitors() {
            effects.redraw(monitor);
        }
        effects.push(Effect::SetCursor(self.cursor));
        if self.selection.is_some() {
            self.settle(&mut effects);
        }
        effects.0
    }

    /// Feeds one event in; `now` is the current time.
    pub fn handle(&mut self, event: Event, now: Millis) -> Vec<Effect> {
        let mut effects = Effects::default();
        if self.closed {
            return effects.0;
        }
        self.now = now;
        match event {
            Event::PointerMove {
                monitor,
                at,
                left_down,
                modifiers,
            } => self.pointer_move(monitor, at, left_down, modifiers, now, &mut effects),
            Event::PointerDown {
                monitor,
                at,
                button,
                modifiers,
            } => self.pointer_down(monitor, at, button, modifiers, &mut effects),
            Event::PointerUp {
                monitor,
                at,
                button,
                ..
            } => self.pointer_up(monitor, at, button, now, &mut effects),
            Event::Key { key, modifiers } => self.key(key, modifiers, now, &mut effects),
            Event::Text(c) => self.typed(c, &mut effects),
            Event::Wheel {
                monitor,
                delta,
                modifiers,
                ..
            } => {
                if monitor == self.active && self.selection.is_some() {
                    self.wheel_turned(delta, modifiers, &mut effects);
                }
            }
            Event::Tick => self.tick(now, &mut effects),
            Event::Focus(focused) => {
                if focused != self.focused {
                    self.focused = focused;
                    effects.redraw(self.active);
                }
            }
        }
        // Only a new wake-up time is news; a stale one just ticks for
        // nothing
        let wake = self.next_wake();
        if !self.closed && wake != self.scheduled {
            self.scheduled = wake;
            if let Some(at) = wake {
                effects.push(Effect::WakeAt(at));
            }
        }
        effects.0
    }

    pub fn monitors(&self) -> &[Monitor] {
        &self.monitors
    }

    /// The monitor the overlay is on.
    pub fn active_monitor(&self) -> usize {
        self.active
    }

    /// Whether the overlay still follows the cursor (mode 2, before the
    /// first press).
    pub fn is_following(&self) -> bool {
        self.following
    }

    /// The monitors that have a window: all of them while following, the
    /// active one otherwise.
    pub fn window_monitors(&self) -> Vec<usize> {
        if self.following {
            (0..self.monitors.len()).collect()
        } else {
            vec![self.active]
        }
    }

    /// The selection in the active monitor's logical coordinates.
    pub fn selection(&self) -> Option<LogicalRect> {
        self.selection
    }

    /// The highlighted window, logical, clipped to the active monitor.
    pub fn hover_window(&self) -> Option<LogicalRect> {
        self.hover
    }

    pub fn help_visible(&self) -> bool {
        self.help_visible
    }

    /// The help message, one (keys, description) pair per row, built from
    /// the bindings (`initHelpMessage`).
    pub fn help_rows(&self) -> Vec<(String, String)> {
        let mut rows = vec![
            ("Mouse".to_owned(), "Select screenshot area".to_owned()),
            (
                "Click".to_owned(),
                "Capture the window under the mouse".to_owned(),
            ),
        ];
        let actions: &[(KeyAction, &str)] = if self.config.has_tasks {
            &[(KeyAction::Accept, "Accept the capture")]
        } else {
            &[
                (KeyAction::Save, "Save screenshot to a file"),
                (KeyAction::Copy, "Copy selection to clipboard"),
            ]
        };
        for (action, description) in actions {
            if let Some(shortcut) = self.config.bindings.shortcut(*action) {
                rows.push((shortcut.label(), (*description).to_owned()));
            }
        }
        rows.push(("Esc".to_owned(), "Exit".to_owned()));
        rows
    }

    /// The `WxH+X+Y` label (physical pixels, relative to the monitor), when
    /// it is showing.
    pub fn geometry_label(&self) -> Option<(GeometryLabel, String)> {
        let selection = self.selection?;
        if !self.label_visible || self.config.geometry_label == GeometryLabel::Hidden {
            return None;
        }
        let monitor = &self.monitors[self.active];
        let physical = monitor.logical_rect_to_physical(selection);
        let origin = monitor.bounds();
        let text = format!(
            "{}x{}+{}+{}",
            physical.width,
            physical.height,
            physical.x - origin.x,
            physical.y - origin.y
        );
        Some((self.config.geometry_label, text))
    }

    /// Whether one of the overlay's windows has the keyboard.
    pub fn is_focused(&self) -> bool {
        self.focused
    }

    pub fn cursor(&self) -> CursorShape {
        self.cursor
    }

    pub fn is_closed(&self) -> bool {
        self.closed
    }

    /// The annotation document of the active monitor's capture: its image
    /// is the whole monitor, in physical pixels, so placed objects stay
    /// where they are drawn when the selection moves, as in 15.0.0.
    pub fn document(&self) -> &Document {
        self.tools.editor.document()
    }

    /// The time of the last event, for fades.
    pub fn now(&self) -> Millis {
        self.now
    }

    fn monitor_rect(&self) -> LogicalRect {
        let (width, height) = self.monitors[self.active].logical_size();
        LogicalRect::new(0, 0, width, height)
    }

    /// Takes an event from another monitor's window. While following with
    /// nothing selected the overlay moves there; otherwise it is ignored.
    fn claim(&mut self, monitor: usize, effects: &mut Effects) -> bool {
        if monitor == self.active {
            return true;
        }
        if !self.following
            || self.selection.is_some()
            || self.drag != Drag::Idle
            || monitor >= self.monitors.len()
        {
            return false;
        }
        let from = self.active;
        self.active = monitor;
        self.hover = None;
        self.tools.editor = Editor::with_undo_limit(
            document_for(&self.monitors[monitor]),
            self.config.tools.undo_limit,
        );
        effects.push(Effect::SwitchMonitor { to: monitor });
        effects.redraw(from);
        effects.redraw(monitor);
        true
    }

    fn pointer_move(
        &mut self,
        monitor: usize,
        at: LogicalPoint,
        left_down: bool,
        modifiers: Modifiers,
        now: Millis,
        effects: &mut Effects,
    ) {
        if !self.claim(monitor, effects) {
            return;
        }
        if self.drag != Drag::Idle && !left_down {
            // The release went somewhere else; finish the drag here
            self.pointer_up(monitor, at, PointerButton::Left, now, effects);
            return;
        }
        if self.tools_drag(at, modifiers, effects) {
            self.update_cursor(Some(at), effects);
            return;
        }
        match self.drag {
            Drag::Idle => {
                self.update_hover(at, effects);
                self.update_tools_hover(at, effects);
            }
            Drag::Pending { start, .. } => {
                if manhattan(at, start) > CLICK_DISTANCE {
                    // A new selection, whatever was selected before. The
                    // pending window click is dropped with it.
                    self.selection = Some(spanned(start, start));
                    self.drag = Drag::Resizing {
                        side: Side::BottomRight,
                    };
                    self.resize_to(at, modifiers, now, effects);
                }
            }
            Drag::Resizing { .. } => self.resize_to(at, modifiers, now, effects),
            // Handled by the tools above
            Drag::Drawing | Drag::MovingObject { .. } | Drag::Button(_) => {}
            Drag::Moving { last } => {
                if let Some(selection) = self.selection {
                    self.selection = Some(translated(selection, at.x - last.x, at.y - last.y));
                    self.drag = Drag::Moving { last: at };
                    self.geometry_changed(now, effects);
                }
            }
        }
        self.update_cursor(Some(at), effects);
    }

    fn resize_to(
        &mut self,
        at: LogicalPoint,
        modifiers: Modifiers,
        now: Millis,
        effects: &mut Effects,
    ) {
        let (Drag::Resizing { side }, Some(selection)) = (self.drag, self.selection) else {
            return;
        };
        let (resized, side) = resize(selection, side, at, modifiers.shift, modifiers.ctrl);
        self.drag = Drag::Resizing { side };
        if resized != selection {
            self.selection = Some(resized);
        }
        // Even an unchanged rectangle counts: it is now a real selection
        self.geometry_changed(now, effects);
    }

    fn pointer_down(
        &mut self,
        monitor: usize,
        at: LogicalPoint,
        button: PointerButton,
        modifiers: Modifiers,
        effects: &mut Effects,
    ) {
        if !self.claim(monitor, effects) {
            return;
        }
        if self.following {
            // The press commits the capture to this monitor
            self.following = false;
            effects.push(Effect::ReleaseMonitors);
            effects.redraw(self.active);
        }
        if button == PointerButton::Right {
            // The colour wheel, centred on the press
            self.open_wheel(at, effects);
            return;
        }
        if button != PointerButton::Left {
            return;
        }
        if self.tools_press(at, modifiers, effects) {
            self.update_cursor(Some(at), effects);
            return;
        }
        // What a click picks is what is under the pointer now, even if it
        // has not moved since the overlay opened
        if self.selection.is_none() {
            self.update_hover(at, effects);
        }
        self.drag = match self.selection.and_then(|selection| side_at(selection, at)) {
            Some(Side::Center) => Drag::Moving { last: at },
            Some(side) => Drag::Resizing { side },
            None => Drag::Pending {
                start: at,
                window_click: self.selection.is_none() && self.hover.is_some(),
            },
        };
        self.update_cursor(Some(at), effects);
    }

    fn pointer_up(
        &mut self,
        monitor: usize,
        at: LogicalPoint,
        button: PointerButton,
        now: Millis,
        effects: &mut Effects,
    ) {
        if monitor != self.active {
            return;
        }
        if button == PointerButton::Right && self.wheel_release(at, effects) {
            self.update_cursor(Some(at), effects);
            return;
        }
        let drag = std::mem::replace(&mut self.drag, Drag::Idle);
        if matches!(
            drag,
            Drag::Drawing | Drag::MovingObject { .. } | Drag::Button(_)
        ) {
            if button == PointerButton::Left {
                self.tools_release(drag, at, effects);
            } else {
                // Another button let go mid-drag: the drag goes on
                self.drag = drag;
            }
            if !self.closed {
                self.update_cursor(Some(at), effects);
            }
            return;
        }
        if button == PointerButton::Left
            && let Drag::Pending {
                start,
                window_click: true,
            } = drag
            && manhattan(at, start) <= CLICK_DISTANCE
            && let Some(window) = self.hover.take()
        {
            self.selection = Some(window);
            self.geometry_changed(now, effects);
            self.settle(effects);
            self.update_cursor(Some(at), effects);
            return;
        }
        // Any button released outside the selection clears it
        // (`SelectionWidget::parentMouseReleaseEvent`)
        if let Some(selection) = self.selection
            && side_at(selection, at).is_none()
        {
            self.clear_selection(effects);
        }
        self.settle(effects);
        if !self.closed {
            self.update_cursor(Some(at), effects);
        }
    }

    fn key(&mut self, key: Key, modifiers: Modifiers, now: Millis, effects: &mut Effects) {
        if self.tools.text.is_some() {
            self.text_key(key, modifiers, effects);
            return;
        }
        if key == Key::Escape {
            if !self.escape_tools(effects) {
                self.close(Outcome::Cancelled, effects);
            }
            return;
        }
        let plain = !modifiers.ctrl && !modifiers.alt && !modifiers.meta && !modifiers.shift;
        if let Key::Char(c) = key
            && plain
            && let Some(digit) = c.to_digit(10)
        {
            // Typed tool sizes (`CaptureWidget::keyPressEvent`)
            self.digit(digit, effects);
            return;
        }
        self.reset_digits();
        let Some(action) = self.config.bindings.action(key, modifiers) else {
            // Ctrl+Y redoes too, unless something else has it
            if key == Key::Char('y') && modifiers == Modifiers::CTRL {
                self.press_button(ButtonType::Redo, effects);
            }
            return;
        };
        let has_tasks = self.config.has_tasks;
        match action {
            KeyAction::Exit => self.close(Outcome::Cancelled, effects),
            KeyAction::Accept if has_tasks => self.export(ExportAction::Accept, effects),
            KeyAction::Copy if !has_tasks => self.export(ExportAction::Copy, effects),
            KeyAction::Save if !has_tasks => self.export(ExportAction::Save, effects),
            KeyAction::CopyFile if !has_tasks => self.export(ExportAction::CopyFile, effects),
            KeyAction::Accept | KeyAction::Copy | KeyAction::Save | KeyAction::CopyFile => {}
            KeyAction::Cancel => {
                if self.selection.is_some() {
                    self.clear_selection(effects);
                }
                self.settle(effects);
            }
            KeyAction::SelectAll => {
                self.selection = Some(self.monitor_rect());
                self.hover = None;
                self.geometry_changed(now, effects);
                self.settle(effects);
            }
            KeyAction::Move(direction) => self.nudge(move_edges(direction), now, effects),
            KeyAction::Resize(direction) => self.nudge(resize_edges(direction), now, effects),
            KeyAction::SymmetricResize(direction) => {
                self.nudge(symmetric_edges(direction), now, effects);
            }
            KeyAction::Button(button) => self.press_button(button, effects),
            KeyAction::CommitTool => self.commit_current(effects),
            KeyAction::DeleteTool => self.delete_selected(effects),
        }
    }

    fn nudge(&mut self, edges: (i32, i32, i32, i32), now: Millis, effects: &mut Effects) {
        let Some(selection) = self.selection else {
            return;
        };
        self.selection = Some(nudged(selection, self.monitor_rect(), edges));
        self.geometry_changed(now, effects);
        self.settle_at = Some(now + KEYBOARD_SETTLE_MS);
    }

    fn tick(&mut self, now: Millis, effects: &mut Effects) {
        // A timer that fired a little early must be asked for again
        self.scheduled = None;
        if self.label_hide_at.is_some_and(|at| at <= now) {
            self.label_hide_at = None;
            self.label_visible = false;
            effects.redraw(self.active);
        }
        if self.settle_at.is_some_and(|at| at <= now) {
            self.settle_at = None;
            self.settle(effects);
        }
        if self.tools.notice_until.is_some_and(|at| at <= now) {
            self.persist_size(effects);
        }
        if self.tools.hover.is_some() && now <= self.tools.hover_since + HOVER_FADE_MS {
            effects.redraw(self.active);
        }
    }

    fn next_wake(&self) -> Option<Millis> {
        [
            self.label_hide_at,
            self.settle_at,
            self.tools.notice_until,
            self.next_frame(),
        ]
        .into_iter()
        .flatten()
        .min()
    }

    fn update_hover(&mut self, at: LogicalPoint, effects: &mut Effects) {
        let monitor = &self.monitors[self.active];
        let hover = if self.selection.is_none() && self.monitor_rect().contains(at) {
            window_at(&self.windows, monitor.logical_point_to_physical(at))
                .and_then(|window| monitor.rect_to_logical(window))
        } else {
            None
        };
        if hover != self.hover {
            self.hover = hover;
            effects.redraw(self.active);
        }
    }

    /// `geometryChanged`: the help goes, the size label shows.
    fn geometry_changed(&mut self, now: Millis, effects: &mut Effects) {
        self.help_visible = false;
        self.hover = None;
        if self.config.geometry_label != GeometryLabel::Hidden {
            self.label_visible = true;
            self.label_hide_at = match self.config.geometry_label_timeout_ms {
                0 => None,
                timeout => Some(now + Millis::from(timeout)),
            };
        }
        effects.redraw(self.active);
    }

    fn clear_selection(&mut self, effects: &mut Effects) {
        self.selection = None;
        self.label_visible = false;
        self.label_hide_at = None;
        self.settle_at = None;
        // The help comes back when the selection goes
        self.help_visible = self.config.show_help;
        effects.redraw(self.active);
    }

    /// `geometrySettled`: under accept-on-select a selection ends the
    /// capture.
    fn settle(&mut self, effects: &mut Effects) {
        if self.config.accept_on_select && self.selection.is_some() {
            self.export(ExportAction::Accept, effects);
        }
    }

    /// Ends the capture with the selection, or the whole monitor when there
    /// is none (`CaptureContext::selectedScreenshotArea`).
    fn export(&mut self, action: ExportAction, effects: &mut Effects) {
        let region = self.export_region(effects);
        self.close(
            Outcome::Export(Export {
                monitor: self.active,
                region,
                action,
            }),
            effects,
        );
    }

    /// Ends the capture with an action the application cannot do yet.
    fn defer(&mut self, action: DeferredAction, effects: &mut Effects) {
        let region = self.export_region(effects);
        self.close(
            Outcome::Deferred(Deferred {
                monitor: self.active,
                region,
                action,
            }),
            effects,
        );
    }

    /// The region to export; typed text is placed first.
    fn export_region(&mut self, effects: &mut Effects) -> LocalRect {
        self.commit_text(effects);
        let monitor = &self.monitors[self.active];
        let whole = LocalRect::new(0, 0, monitor.bounds().width, monitor.bounds().height);
        self.selection
            .and_then(|selection| selection.intersection(&self.monitor_rect()))
            .and_then(|visible| monitor.rect_to_local(monitor.logical_rect_to_physical(visible)))
            .unwrap_or(whole)
    }

    fn close(&mut self, outcome: Outcome, effects: &mut Effects) {
        if self.closed {
            return;
        }
        // A size still on its notice is saved first: Close comes last
        self.persist_size(effects);
        self.closed = true;
        self.drag = Drag::Idle;
        effects
            .0
            .retain(|effect| !matches!(effect, Effect::WakeAt(_)));
        effects.push(Effect::Close(outcome));
    }

    fn update_cursor(&mut self, at: Option<LogicalPoint>, effects: &mut Effects) {
        let shape = match self.drag {
            Drag::Moving { .. } | Drag::MovingObject { .. } => CursorShape::Grabbing,
            Drag::Resizing { side } => side_cursor(side),
            Drag::Drawing => CursorShape::Cross,
            Drag::Button(_) => CursorShape::Arrow,
            Drag::Idle | Drag::Pending { .. }
                if at.is_some_and(|at| self.over_toolbar(at) || self.tools.wheel.is_some()) =>
            {
                CursorShape::Arrow
            }
            Drag::Idle | Drag::Pending { .. } => {
                let drawing = self
                    .tools
                    .active
                    .is_some_and(|tool| tools::draws(tool) || tool == ButtonType::MoveSelection);
                match (self.selection, at) {
                    (Some(selection), Some(at)) => match side_at(selection, at) {
                        // A drawing tool draws inside
                        Some(Side::Center) if drawing => CursorShape::Cross,
                        // `setIdleCentralCursor(Qt::ArrowCursor)`
                        Some(Side::Center) => CursorShape::Arrow,
                        Some(side) => side_cursor(side),
                        None => CursorShape::Cross,
                    },
                    _ => CursorShape::Cross,
                }
            }
        };
        if shape != self.cursor {
            self.cursor = shape;
            effects.push(Effect::SetCursor(shape));
        }
    }
}

fn document_for(monitor: &Monitor) -> Document {
    let bounds = monitor.bounds();
    Document::new(Size::new(
        u32::try_from(bounds.width).unwrap_or(0),
        u32::try_from(bounds.height).unwrap_or(0),
    ))
}

fn manhattan(a: LogicalPoint, b: LogicalPoint) -> i32 {
    (a.x - b.x).abs() + (a.y - b.y).abs()
}

fn side_cursor(side: Side) -> CursorShape {
    match side {
        Side::TopLeft | Side::BottomRight => CursorShape::ResizeNwSe,
        Side::TopRight | Side::BottomLeft => CursorShape::ResizeNeSw,
        Side::Left | Side::Right => CursorShape::ResizeHorizontal,
        Side::Top | Side::Bottom => CursorShape::ResizeVertical,
        Side::Center => CursorShape::Grabbing,
    }
}

/// `moveLeft` and friends: (left, top, right, bottom) edge offsets.
fn move_edges(direction: Direction) -> (i32, i32, i32, i32) {
    match direction {
        Direction::Left => (-1, 0, -1, 0),
        Direction::Right => (1, 0, 1, 0),
        Direction::Up => (0, -1, 0, -1),
        Direction::Down => (0, 1, 0, 1),
    }
}

/// `resizeLeft` and friends: the right or bottom edge moves.
fn resize_edges(direction: Direction) -> (i32, i32, i32, i32) {
    match direction {
        Direction::Left => (0, 0, -1, 0),
        Direction::Right => (0, 0, 1, 0),
        Direction::Up => (0, 0, 0, -1),
        Direction::Down => (0, 0, 0, 1),
    }
}

/// `symResizeLeft` and friends: both edges of an axis, opposite ways.
fn symmetric_edges(direction: Direction) -> (i32, i32, i32, i32) {
    match direction {
        Direction::Left => (1, 0, -1, 0),
        Direction::Right => (-1, 0, 1, 0),
        Direction::Up => (0, -1, 0, 1),
        Direction::Down => (0, 1, 0, -1),
    }
}
