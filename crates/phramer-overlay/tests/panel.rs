//! Scripted event sequences through the side panel, the magnifier and
//! colour grab (15.0.0's `SidePanelWidget`, `MagnifierWidget` and
//! `ColorGrabWidget`): what they show, what they change, and the settings
//! written back.

use std::sync::Arc;

use phramer_annotate::{Color, Point, Shape};
use phramer_capture::{LogicalPoint, LogicalRect, Monitor, MonitorId, PhysicalRect};
use phramer_core::config::{ButtonType, Settings};
use phramer_overlay::{
    CursorShape, Effect, Event, Key, MagnifierShape, Modifiers, Outcome, OverlayConfig,
    OverlayState, PanelLayout, Pixels, PointerButton, SettingChange,
};

fn p(x: i32, y: i32) -> LogicalPoint {
    LogicalPoint::new(x, y)
}

fn monitor_at(x: i32, y: i32, width: i32, height: i32, dpi: u32) -> Monitor {
    Monitor::new(
        MonitorId::new(format!("{x},{y}")),
        PhysicalRect::new(x, y, width, height),
        dpi,
        x == 0 && y == 0,
    )
}

/// The colour the test screenshots have at image pixel (x, y): every pixel
/// tells where it is.
fn pattern_color(x: i32, y: i32) -> Color {
    Color::rgb(
        (x % 251) as u8,
        (y % 241) as u8,
        ((x / 251 + 7 * (y / 241)) % 256) as u8,
    )
}

fn pattern(width: i32, height: i32) -> Vec<u8> {
    let mut bgra = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let color = pattern_color(x, y);
            bgra.extend_from_slice(&[color.b, color.g, color.r, 255]);
        }
    }
    bgra
}

fn center(rect: LogicalRect) -> LogicalPoint {
    p(rect.x + rect.width / 2, rect.y + rect.height / 2)
}

const ALT: Modifiers = Modifiers {
    alt: true,
    ..Modifiers::NONE
};

struct Overlay {
    state: OverlayState,
    now: u64,
    effects: Vec<Effect>,
}

impl Overlay {
    /// An overlay on `monitors[active]`, each with its pattern screenshot.
    fn on(monitors: Vec<Monitor>, active: usize, config: OverlayConfig) -> Self {
        let mut state =
            OverlayState::new(monitors.clone(), active, Vec::new(), config).expect("state");
        for (index, monitor) in monitors.iter().enumerate() {
            let bounds = monitor.bounds();
            let pixels = Pixels::new(
                bounds.width as u32,
                bounds.height as u32,
                Arc::new(pattern(bounds.width, bounds.height)),
            )
            .expect("pixels");
            state.set_pixels(index, pixels);
        }
        let effects = state.start();
        Self {
            state,
            now: 1_000,
            effects,
        }
    }

    fn with(config: OverlayConfig) -> Self {
        Self::on(vec![monitor_at(0, 0, 1920, 1080, 96)], 0, config)
    }

    fn new() -> Self {
        Self::with(OverlayConfig::default())
    }

    /// An overlay with a selection of (200,200)-(1000,700).
    fn selected() -> Self {
        let mut overlay = Self::new();
        overlay.drag(p(200, 200), p(1000, 700));
        overlay
    }

    fn send(&mut self, event: Event) -> Vec<Effect> {
        self.effects = self.state.handle(event, self.now);
        self.effects.clone()
    }

    fn at_time(&mut self, now: u64) {
        self.now = now;
    }

    fn monitor(&self) -> usize {
        self.state.active_monitor()
    }

    fn press_on(&mut self, monitor: usize, at: LogicalPoint, button: PointerButton) -> Vec<Effect> {
        self.send(Event::PointerDown {
            monitor,
            at,
            button,
            modifiers: Modifiers::NONE,
        })
    }

    fn press(&mut self, at: LogicalPoint, button: PointerButton) -> Vec<Effect> {
        self.press_on(self.monitor(), at, button)
    }

    fn release(&mut self, at: LogicalPoint, button: PointerButton) -> Vec<Effect> {
        self.send(Event::PointerUp {
            monitor: self.monitor(),
            at,
            button,
            modifiers: Modifiers::NONE,
        })
    }

    fn hover_on(&mut self, monitor: usize, at: LogicalPoint) -> Vec<Effect> {
        self.send(Event::PointerMove {
            monitor,
            at,
            left_down: false,
            modifiers: Modifiers::NONE,
        })
    }

    fn hover(&mut self, at: LogicalPoint) -> Vec<Effect> {
        self.hover_on(self.monitor(), at)
    }

    fn drag_to(&mut self, at: LogicalPoint) -> Vec<Effect> {
        self.send(Event::PointerMove {
            monitor: self.monitor(),
            at,
            left_down: true,
            modifiers: Modifiers::NONE,
        })
    }

    /// Press, move halfway, move, release; the effects of all of it.
    fn drag(&mut self, from: LogicalPoint, to: LogicalPoint) -> Vec<Effect> {
        let mut all = self.press(from, PointerButton::Left);
        all.extend(self.drag_to(p((from.x + to.x) / 2, (from.y + to.y) / 2)));
        all.extend(self.drag_to(to));
        all.extend(self.release(to, PointerButton::Left));
        self.effects = all.clone();
        all
    }

    fn click(&mut self, at: LogicalPoint) -> Vec<Effect> {
        let mut all = self.press(at, PointerButton::Left);
        all.extend(self.release(at, PointerButton::Left));
        self.effects = all.clone();
        all
    }

    fn key(&mut self, key: Key, modifiers: Modifiers) -> Vec<Effect> {
        self.send(Event::Key { key, modifiers })
    }

    fn char(&mut self, c: char) -> Vec<Effect> {
        self.key(Key::Char(c), Modifiers::NONE)
    }

    fn space(&mut self) -> Vec<Effect> {
        self.key(Key::Space, Modifiers::NONE)
    }

    fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            self.send(Event::Text(c));
        }
    }

    fn layout(&self) -> PanelLayout {
        self.state.side_panel().expect("the panel is open").layout
    }

    fn row_labels(&self) -> Vec<String> {
        self.state
            .side_panel()
            .expect("the panel is open")
            .rows
            .into_iter()
            .map(|row| row.label)
            .collect()
    }

    /// Draws a rectangle with the Rectangle tool, then puts the tool down.
    fn rectangle(&mut self, from: LogicalPoint, to: LogicalPoint) {
        self.char('r');
        assert_eq!(self.state.active_tool(), Some(ButtonType::Rectangle));
        self.drag(from, to);
        self.key(Key::Escape, Modifiers::NONE);
    }
}

fn persisted(effects: &[Effect]) -> Vec<SettingChange> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Persist(change) => Some(*change),
            _ => None,
        })
        .collect()
}

fn redraws(effects: &[Effect], monitor: usize) -> bool {
    effects.contains(&Effect::Redraw { monitor })
}

const RED: Color = Color::rgb(255, 0, 0);

// --- Side panel ----------------------------------------------------------------

#[test]
fn space_toggles_the_side_panel_and_esc_closes_it_after_the_tool() {
    let mut overlay = Overlay::selected();
    assert!(!overlay.state.side_panel_open());
    assert!(redraws(&overlay.space(), 0));
    assert!(overlay.state.side_panel_open());
    overlay.space();
    assert!(!overlay.state.side_panel_open());
    // Still on screen while it fades out, gone after
    assert!(
        overlay
            .state
            .side_panel()
            .is_some_and(|panel| !panel.opening)
    );
    overlay.at_time(2_000);
    overlay.send(Event::Tick);
    assert!(overlay.state.side_panel().is_none());

    // `deleteToolWidgetOrClose`: the tool goes first, then the panel
    overlay.space();
    overlay.char('p');
    overlay.key(Key::Escape, Modifiers::NONE);
    assert_eq!(overlay.state.active_tool(), None);
    assert!(overlay.state.side_panel_open());
    overlay.key(Key::Escape, Modifiers::NONE);
    assert!(!overlay.state.side_panel_open());
    assert!(overlay.state.selection().is_some());
    let effects = overlay.key(Key::Escape, Modifiers::NONE);
    assert!(effects.contains(&Effect::Close(Outcome::Cancelled)));
}

#[test]
fn the_panel_fades_in_over_250_ms_unless_motion_is_reduced() {
    let mut overlay = Overlay::selected();
    let effects = overlay.space();
    assert!(effects.contains(&Effect::WakeAt(1_016)));
    let progress = |overlay: &Overlay| overlay.state.side_panel().expect("panel").progress;
    assert_eq!(progress(&overlay), 0.0);
    overlay.at_time(1_125);
    assert!(redraws(&overlay.send(Event::Tick), 0));
    assert!((progress(&overlay) - 0.5).abs() < 1e-6);
    overlay.at_time(1_250);
    overlay.send(Event::Tick);
    assert_eq!(progress(&overlay), 1.0);
    // Input is taken while it fades: the panel works at once
    let mut overlay = Overlay::selected();
    overlay.space();
    let grid = overlay.layout().grid;
    overlay.click(center(grid));
    assert_eq!(overlay.state.grid(), Some(10));

    let mut reduced = Overlay::with(OverlayConfig {
        reduced_motion: true,
        ..OverlayConfig::default()
    });
    let effects = reduced.space();
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::WakeAt(_)))
    );
    assert_eq!(progress(&reduced), 1.0);
}

#[test]
fn the_tool_settings_tab_follows_show_side_panel_button() {
    let mut overlay = Overlay::selected();
    let tab = overlay.state.side_panel_tab().expect("tab");
    assert_eq!(tab.x, 0);
    assert_eq!(tab.y + tab.height / 2, 540);
    overlay.hover(center(tab));
    assert_eq!(overlay.state.cursor(), CursorShape::Arrow);
    overlay.click(center(tab));
    assert!(overlay.state.side_panel_open());
    // The click kept the selection, and the tab steps aside for the panel
    assert!(overlay.state.selection().is_some());
    assert!(overlay.state.side_panel_tab().is_none());
    let close = overlay.layout().close;
    overlay.click(center(close));
    assert!(!overlay.state.side_panel_open());
    assert!(overlay.state.side_panel_tab().is_some());

    let mut hidden = Overlay::with(OverlayConfig {
        side_panel_button: false,
        ..OverlayConfig::default()
    });
    assert!(hidden.state.side_panel_tab().is_none());
    hidden.drag(p(5, 500), p(300, 700));
    assert!(!hidden.state.side_panel_open());
    assert_eq!(
        hidden.state.selection(),
        Some(LogicalRect::new(5, 500, 295, 200))
    );
    // Space still opens it
    hidden.space();
    assert!(hidden.state.side_panel_open());
}

#[test]
fn the_size_slider_applies_live_and_saves_on_release() {
    let mut overlay = Overlay::selected();
    overlay.char('p');
    overlay.space();
    let slider = overlay.layout().slider;
    let left = slider.x + 8;
    let span = slider.width - 16;
    let effects = overlay.press(p(left, slider.y + 10), PointerButton::Left);
    assert_eq!(overlay.state.tool_size(), 1);
    assert!(persisted(&effects).is_empty());
    let effects = overlay.drag_to(p(left + span / 2, slider.y + 40));
    // 24.5 steps from 1 rounds to 26; off the track vertically still slides
    assert_eq!(overlay.state.tool_size(), 26);
    assert!(redraws(&effects, 0));
    overlay.drag_to(p(slider.right() + 200, slider.y));
    assert_eq!(overlay.state.tool_size(), 50);
    assert!(overlay.state.size_notice().is_none());
    let effects = overlay.release(p(slider.right() + 200, slider.y), PointerButton::Left);
    assert_eq!(
        persisted(&effects),
        [SettingChange::ToolSize {
            button: ButtonType::Pencil,
            size: 50
        }]
    );
    // Without a tool, the size is the selected object's and nothing is saved
    overlay.key(Key::Escape, Modifiers::NONE);
    overlay.press(p(left, slider.y + 10), PointerButton::Left);
    let effects = overlay.release(p(left, slider.y + 10), PointerButton::Left);
    assert_eq!(overlay.state.tool_size(), 1);
    assert!(persisted(&effects).is_empty());
}

#[test]
fn the_disc_recolours_live_and_saves_once_on_release() {
    let mut overlay = Overlay::selected();
    overlay.rectangle(p(400, 300), p(600, 450));
    overlay.click(p(500, 380));
    let id = overlay.state.selected_object().expect("selected");
    overlay.space();
    let disc = overlay.layout().disc;
    let middle = center(disc);
    let effects = overlay.press(middle, PointerButton::Left);
    // The centre of the disc is white, shown on the object at once
    assert_eq!(overlay.state.draw_color(), Color::rgb(255, 255, 255));
    assert_eq!(overlay.state.excluded_object(), Some(id));
    assert_eq!(overlay.state.previews()[0].color, Color::rgb(255, 255, 255));
    assert!(persisted(&effects).is_empty());
    // The left edge is cyan; past the edge it stays on the rim
    overlay.drag_to(p(disc.x - 50, middle.y));
    assert_eq!(overlay.state.draw_color(), Color::rgb(0, 255, 255));
    let effects = overlay.release(p(disc.x - 50, middle.y), PointerButton::Left);
    let cyan = Color::rgb(0, 255, 255);
    assert_eq!(persisted(&effects), [SettingChange::DrawColor(cyan)]);
    assert_eq!(
        overlay.state.document().get(id).map(|object| object.color),
        Some(cyan)
    );
    assert_eq!(overlay.state.excluded_object(), None);
    // One step to undo
    overlay.key(Key::Char('z'), Modifiers::CTRL);
    assert_eq!(
        overlay.state.document().get(id).map(|object| object.color),
        Some(RED)
    );
}

#[test]
fn the_hex_field_takes_a_typed_colour_when_editing_finishes() {
    let mut overlay = Overlay::selected();
    overlay.space();
    let hex = overlay.layout().hex;
    overlay.click(center(hex));
    assert!(
        overlay
            .state
            .side_panel()
            .is_some_and(|panel| panel.hex_focused)
    );
    for _ in 0..7 {
        overlay.key(Key::Backspace, Modifiers::NONE);
    }
    // Digit keys type, they do not set the tool size
    let size = overlay.state.tool_size();
    for c in "#00ff00".chars() {
        overlay.key(Key::Char(c), Modifiers::NONE);
        overlay.send(Event::Text(c));
    }
    assert_eq!(overlay.state.tool_size(), size);
    assert_eq!(overlay.state.side_panel().expect("panel").hex, "#00ff00");
    let effects = overlay.key(Key::Return, Modifiers::NONE);
    let green = Color::rgb(0, 255, 0);
    assert_eq!(persisted(&effects), [SettingChange::DrawColor(green)]);
    assert_eq!(overlay.state.draw_color(), green);

    // Not a colour: dropped, and the field shows the colour again
    overlay.click(center(hex));
    overlay.key(Key::Backspace, Modifiers::NONE);
    overlay.key(Key::Backspace, Modifiers::NONE);
    let effects = overlay.key(Key::Return, Modifiers::NONE);
    assert!(persisted(&effects).is_empty());
    assert_eq!(overlay.state.side_panel().expect("panel").hex, "#00ff00");

    // Esc only takes the keyboard away, which finishes editing (15.0.0)
    overlay.click(center(hex));
    overlay.key(Key::Backspace, Modifiers::NONE);
    overlay.key(Key::Backspace, Modifiers::NONE);
    overlay.type_text("f0");
    let effects = overlay.key(Key::Escape, Modifiers::NONE);
    assert_eq!(
        persisted(&effects),
        [SettingChange::DrawColor(Color::rgb(0, 255, 0xf0))]
    );
    assert!(overlay.state.side_panel_open());
    assert!(!overlay.state.is_closed());

    // A click elsewhere finishes it too
    overlay.click(center(hex));
    overlay.key(Key::Backspace, Modifiers::NONE);
    overlay.key(Key::Backspace, Modifiers::NONE);
    overlay.type_text("00");
    let effects = overlay.click(p(1500, 900));
    assert_eq!(
        persisted(&effects),
        [SettingChange::DrawColor(Color::rgb(0, 255, 0))]
    );
}

#[test]
fn display_grid_shows_and_snaps_drawing() {
    let mut overlay = Overlay::selected();
    overlay.space();
    let layout = overlay.layout();
    assert_eq!(overlay.state.grid(), None);
    // The spacing is fixed while the grid is off
    overlay.click(center(layout.grid_larger));
    overlay.click(center(layout.grid));
    assert_eq!(overlay.state.grid(), Some(10));
    overlay.click(center(layout.grid_larger));
    assert_eq!(overlay.state.grid(), Some(15));
    for _ in 0..3 {
        overlay.click(center(layout.grid_smaller));
    }
    assert_eq!(overlay.state.grid(), Some(5));
    overlay.click(center(layout.grid_larger));
    assert_eq!(overlay.state.grid(), Some(10));
    overlay.space();
    overlay.char('r');
    overlay.drag(p(303, 304), p(497, 512));
    let object = overlay.state.document().objects().last().expect("drawn");
    let Shape::Rectangle { start, end, .. } = &object.shape else {
        panic!("{:?}", object.shape);
    };
    assert_eq!((*start, *end), (Point::new(300, 300), Point::new(500, 510)));
}

#[test]
fn the_object_list_selects_and_deletes() {
    let mut overlay = Overlay::selected();
    overlay.rectangle(p(300, 300), p(400, 400));
    overlay.rectangle(p(500, 300), p(600, 400));
    overlay.char('t');
    overlay.click(p(300, 550));
    overlay.type_text("A long line of text here");
    overlay.key(Key::Return, Modifiers::NONE);
    overlay.type_text("second");
    overlay.key(Key::Escape, Modifiers::NONE);
    overlay.key(Key::Escape, Modifiers::NONE);
    overlay.space();
    assert_eq!(
        overlay.row_labels(),
        [
            "<Empty>",
            "Rectangle",
            "Rectangle",
            "Text - A long line of te…"
        ]
    );
    let rows = overlay.layout().rows;
    let first = overlay.state.document().objects()[0].id;
    overlay.click(center(rows[1]));
    assert_eq!(overlay.state.selected_object(), Some(first));
    assert!(overlay.state.side_panel().expect("panel").rows[1].selected);
    let delete = overlay.layout().delete;
    overlay.click(center(delete));
    assert_eq!(overlay.state.document().objects().len(), 2);
    assert_eq!(overlay.state.selected_object(), None);
    overlay.click(center(overlay.layout().rows[1]));
    assert!(overlay.state.selected_object().is_some());
    overlay.click(center(overlay.layout().rows[0]));
    assert_eq!(overlay.state.selected_object(), None);
    // Delete with nothing selected does nothing
    overlay.click(center(delete));
    assert_eq!(overlay.state.document().objects().len(), 2);
}

#[test]
fn presses_on_the_panel_stay_on_the_panel() {
    let mut overlay = Overlay::selected();
    overlay.space();
    let panel = overlay.layout().panel;
    let inside = p(panel.x + 4, panel.bottom() - 4);
    overlay.click(inside);
    assert!(overlay.state.selection().is_some());
    overlay.hover(inside);
    assert_eq!(overlay.state.cursor(), CursorShape::Arrow);
    // The right button does not open the colour wheel over it
    overlay.press(inside, PointerButton::Right);
    overlay.release(inside, PointerButton::Right);
    assert!(overlay.state.wheel().is_none());
    overlay.press(p(1500, 900), PointerButton::Right);
    assert!(overlay.state.wheel().is_some());
}

#[test]
fn the_wheel_scrolls_the_object_list_and_sizes_tools_elsewhere() {
    let mut overlay = Overlay::selected();
    overlay.char('r');
    for index in 0..40 {
        let x = 240 + (index % 10) * 70;
        let y = 240 + (index / 10) * 70;
        overlay.drag(p(x, y), p(x + 40, y + 40));
    }
    overlay.key(Key::Escape, Modifiers::NONE);
    overlay.space();
    let rows = overlay.layout().rows;
    assert!(rows.len() < 41);
    let over_list = center(rows[0]);
    let wheel = |overlay: &mut Overlay, at: LogicalPoint, delta: i32| {
        overlay.send(Event::Wheel {
            monitor: 0,
            at,
            delta,
            modifiers: Modifiers::NONE,
        })
    };
    let size = overlay.state.tool_size();
    wheel(&mut overlay, over_list, -120);
    assert_eq!(overlay.state.side_panel().expect("panel").first_row, 1);
    assert_eq!(overlay.row_labels()[0], "Rectangle");
    assert_eq!(overlay.state.tool_size(), size);
    for _ in 0..100 {
        wheel(&mut overlay, over_list, -120);
    }
    let view = overlay.state.side_panel().expect("panel");
    assert_eq!(view.first_row + view.rows.len(), 41);
    wheel(&mut overlay, over_list, 120);
    assert_eq!(
        overlay.state.side_panel().expect("panel").first_row,
        view.first_row - 1
    );
    wheel(&mut overlay, p(1500, 900), 120);
    assert_eq!(overlay.state.tool_size(), size + 1);
}

#[test]
fn space_types_into_text_and_the_keys_follow_the_settings() {
    let mut overlay = Overlay::selected();
    overlay.char('t');
    overlay.click(p(300, 300));
    overlay.space();
    overlay.send(Event::Text(' '));
    assert!(!overlay.state.side_panel_open());
    assert_eq!(overlay.state.text_being_edited(), Some(" "));

    let mut settings = Settings::default();
    settings
        .set_shortcut("TYPE_TOGGLE_PANEL", "F2")
        .unwrap_or_else(|error| panic!("{error}"));
    settings
        .set_shortcut("TYPE_GRAB_COLOR", "")
        .unwrap_or_else(|error| panic!("{error}"));
    let mut overlay = Overlay::with(OverlayConfig::from_settings(&settings));
    overlay.space();
    assert!(!overlay.state.side_panel_open());
    overlay.key(Key::Function(2), Modifiers::NONE);
    assert!(overlay.state.side_panel_open());
    overlay.key(Key::Char('g'), ALT);
    assert!(!overlay.state.is_grabbing_color());
}

// --- Magnifier ------------------------------------------------------------------

fn magnifying() -> OverlayConfig {
    OverlayConfig {
        show_magnifier: true,
        ..OverlayConfig::default()
    }
}

#[test]
fn the_magnifier_follows_show_magnifier_and_the_active_tool() {
    let mut off = Overlay::new();
    off.hover(p(500, 500));
    assert!(off.state.magnifier().is_none());

    let mut overlay = Overlay::with(magnifying());
    let effects = overlay.hover(p(500, 500));
    assert!(redraws(&effects, 0));
    let view = overlay.state.magnifier().expect("magnifier");
    assert_eq!(view.shape, MagnifierShape::Round);
    assert_eq!((view.zoom.cell, view.zoom.cells), (10, 17));
    assert_eq!(view.zoom.source, Point::new(492, 492));
    // While a selection is dragged out and resized
    overlay.press(p(500, 500), PointerButton::Left);
    overlay.drag_to(p(700, 600));
    assert!(overlay.state.magnifier().is_some());
    overlay.drag_to(p(900, 700));
    overlay.release(p(900, 700), PointerButton::Left);
    overlay.press(p(900, 700), PointerButton::Left);
    overlay.drag_to(p(950, 750));
    assert!(overlay.state.magnifier().is_some());
    overlay.release(p(950, 750), PointerButton::Left);
    // Not with a tool (15.0.0: `!m_activeButton`)
    overlay.char('p');
    overlay.hover(p(700, 600));
    assert!(overlay.state.magnifier().is_none());
    overlay.key(Key::Escape, Modifiers::NONE);
    overlay.hover(p(701, 600));
    assert!(overlay.state.magnifier().is_some());
    // Not over the toolbar, the panel or the colour wheel
    let button = overlay.state.toolbar()[0].rect;
    overlay.hover(center(button));
    assert!(overlay.state.magnifier().is_none());
    overlay.space();
    overlay.hover(center(overlay.layout().disc));
    assert!(overlay.state.magnifier().is_none());
    overlay.space();
    overlay.press(p(700, 600), PointerButton::Right);
    assert!(overlay.state.magnifier().is_none());

    let square = Overlay::with(OverlayConfig {
        square_magnifier: true,
        ..magnifying()
    });
    let mut square = square;
    square.hover(p(500, 500));
    assert_eq!(
        square.state.magnifier().map(|view| view.shape),
        Some(MagnifierShape::Square)
    );
}

#[test]
fn the_magnifier_stays_on_the_monitor_and_its_pixels_are_whole() {
    let mut overlay = Overlay::with(magnifying());
    let zoom = |overlay: &mut Overlay, at: LogicalPoint| {
        overlay.hover(at);
        overlay.state.magnifier().expect("magnifier").zoom
    };
    // Below right of the pointer, 16 away
    let near = zoom(&mut overlay, p(100, 100));
    assert_eq!((near.x, near.y), (116, 116));
    // By the bottom right corner it goes above and to the left
    let corner = zoom(&mut overlay, p(1910, 1070));
    assert_eq!((corner.x, corner.y), (1910 - 16 - 170, 1070 - 16 - 170));
    let top_right = zoom(&mut overlay, p(1910, 5));
    assert_eq!((top_right.x, top_right.y), (1724, 21));

    // A whole number of canvas pixels per screenshot pixel at 100, 150 and
    // 200%, and the same 170 logical pixels across
    for (dpi, cell) in [(96, 10), (144, 15), (192, 20)] {
        let scale = f64::from(dpi) / 96.0;
        let size = |v: i32| (f64::from(v) * scale).round() as i32;
        let mut overlay = Overlay::on(
            vec![monitor_at(0, 0, size(1280), size(720), dpi)],
            0,
            magnifying(),
        );
        let view = zoom(&mut overlay, p(640, 360));
        assert_eq!(view.cell, cell);
        assert_eq!(view.side(), size(170));
        let pixel = (size(640), size(360));
        assert_eq!(
            (view.source.x + view.center.0, view.source.y + view.center.1),
            pixel
        );
        assert_eq!(view.x, pixel.0 + size(16));
        // Kept whole on the monitor at the far corner
        let far = zoom(&mut overlay, p(1279, 719));
        assert!(far.x >= 0 && far.x + far.side() <= size(1280));
        assert!(far.y >= 0 && far.y + far.side() <= size(720));
    }

    // At the corner, the square one keeps its pixels on the screenshot and
    // moves the crosshair; the round one shows black past the edge
    let mut square = Overlay::with(OverlayConfig {
        square_magnifier: true,
        ..magnifying()
    });
    let view = zoom(&mut square, p(0, 0));
    assert_eq!((view.source, view.center), (Point::new(0, 0), (0, 0)));
    let view = zoom(&mut overlay, p(0, 0));
    assert_eq!((view.source, view.center), (Point::new(-8, -8), (8, 8)));
}

#[test]
fn the_magnifier_follows_the_cursor_across_monitors() {
    let monitors = vec![
        monitor_at(-1920, 0, 1920, 1080, 96),
        monitor_at(0, 0, 2880, 1620, 144),
    ];
    let mut overlay = Overlay::on(
        monitors,
        0,
        OverlayConfig {
            follow_cursor: true,
            ..magnifying()
        },
    );
    overlay.hover_on(0, p(500, 500));
    assert_eq!(overlay.state.magnifier().expect("on 0").zoom.cell, 10);
    let effects = overlay.hover_on(1, p(500, 500));
    assert_eq!(overlay.state.active_monitor(), 1);
    assert!(redraws(&effects, 1));
    let view = overlay.state.magnifier().expect("on 1");
    assert_eq!(view.zoom.cell, 15);
    assert_eq!(view.zoom.source, Point::new(750 - 8, 750 - 8));
}

// --- Colour grab ----------------------------------------------------------------

/// One monitor at 150% whose corner is at negative desktop coordinates.
fn negative_150() -> Overlay {
    Overlay::on(
        vec![monitor_at(-2880, -1620, 2880, 1620, 144)],
        0,
        OverlayConfig::default(),
    )
}

#[test]
fn alt_g_grabs_the_colour_under_the_pointer_and_enter_keeps_it() {
    let mut overlay = negative_150();
    overlay.hover(p(101, 67));
    let effects = overlay.key(Key::Char('g'), ALT);
    assert!(overlay.state.is_grabbing_color());
    assert_eq!(overlay.state.cursor(), CursorShape::Cross);
    // 101.5 and 100.5 round away from zero
    let picked = pattern_color(152, 101);
    assert_eq!(overlay.state.draw_color(), picked);
    assert!(persisted(&effects).is_empty());
    let view = overlay.state.color_grab().expect("grab");
    assert_eq!(view.pointer, Point::new(152, 101));
    let zoom = view.zoom.expect("preview");
    // Seven pixels at 11x: 17 canvas pixels each at 150%, the pointer's in
    // the middle
    assert_eq!((zoom.cells, zoom.cell, zoom.center), (7, 17, (3, 3)));
    assert_eq!(zoom.source, Point::new(149, 98));
    // Other keys are swallowed
    overlay.char('p');
    assert_eq!(overlay.state.active_tool(), None);
    let effects = overlay.key(Key::Char('c'), Modifiers::CTRL);
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::Close(_)))
    );
    let effects = overlay.key(Key::Return, Modifiers::NONE);
    assert!(!overlay.state.is_grabbing_color());
    assert_eq!(persisted(&effects), [SettingChange::DrawColor(picked)]);
    // The panel shows when grabbing ends (`finalizeGrab`), here under the
    // pointer
    assert!(overlay.state.side_panel_open());
    assert_eq!(overlay.state.cursor(), CursorShape::Arrow);
}

#[test]
fn a_click_picks_the_exact_pixel_at_mixed_scales() {
    // 125% at negative coordinates: 416.25 and 96.25 round down
    let mut overlay = Overlay::on(
        vec![monitor_at(-2400, -1350, 2400, 1350, 120)],
        0,
        OverlayConfig::default(),
    );
    overlay.key(Key::Char('g'), ALT);
    overlay.hover(p(333, 77));
    let effects = overlay.click(p(333, 77));
    assert!(!overlay.state.is_grabbing_color());
    assert_eq!(
        persisted(&effects),
        [SettingChange::DrawColor(pattern_color(416, 96))]
    );

    // Started on one monitor (100%, left of the primary), picked on the
    // other (150%) while the overlay follows the cursor
    let monitors = vec![
        monitor_at(-1920, 0, 1920, 1080, 96),
        monitor_at(0, 0, 2880, 1620, 144),
    ];
    let mut overlay = Overlay::on(
        monitors,
        0,
        OverlayConfig {
            follow_cursor: true,
            ..OverlayConfig::default()
        },
    );
    overlay.hover_on(0, p(10, 20));
    overlay.key(Key::Char('g'), ALT);
    assert_eq!(overlay.state.draw_color(), pattern_color(10, 20));
    overlay.hover_on(1, p(1279, 3));
    assert_eq!(overlay.state.active_monitor(), 1);
    assert_eq!(overlay.state.draw_color(), pattern_color(1919, 5));
    overlay.press_on(1, p(1279, 3), PointerButton::Left);
    let effects = overlay.release(p(1279, 3), PointerButton::Left);
    assert_eq!(
        persisted(&effects),
        [SettingChange::DrawColor(pattern_color(1919, 5))]
    );
}

#[test]
fn esc_cancels_the_grab_and_brings_the_colour_back() {
    let mut overlay = Overlay::selected();
    overlay.key(Key::Char('g'), ALT);
    overlay.hover(p(700, 500));
    assert_eq!(overlay.state.draw_color(), pattern_color(700, 500));
    let effects = overlay.key(Key::Escape, Modifiers::NONE);
    assert!(!overlay.state.is_grabbing_color());
    assert!(!overlay.state.is_closed());
    assert_eq!(overlay.state.draw_color(), RED);
    assert!(persisted(&effects).is_empty());
    assert!(overlay.state.side_panel_open());
    assert!(overlay.state.selection().is_some());
}

#[test]
fn holding_the_click_zooms_in_and_picks_inside_the_zoom() {
    let mut overlay = Overlay::new();
    overlay.key(Key::Char('g'), ALT);
    overlay.hover(p(500, 400));
    let effects = overlay.press(p(500, 400), PointerButton::Left);
    assert!(effects.contains(&Effect::WakeAt(1_250)));
    assert!(!overlay.state.color_grab().expect("grab").precise);
    overlay.at_time(1_250);
    assert!(redraws(&overlay.send(Event::Tick), 0));
    let view = overlay.state.color_grab().expect("grab");
    assert!(view.precise);
    let zoom = view.zoom.expect("zoom");
    // Eleven pixels at 15x round the press, which stays where it was
    assert_eq!((zoom.cells, zoom.cell), (11, 15));
    assert_eq!((zoom.x, zoom.y), (500 - 75 - 7, 400 - 75 - 7));
    // The pixel drawn under the pointer is the one picked
    overlay.send(Event::PointerMove {
        monitor: 0,
        at: p(515, 370),
        left_down: true,
        modifiers: Modifiers::NONE,
    });
    assert_eq!(overlay.state.draw_color(), pattern_color(501, 398));
    let effects = overlay.release(p(515, 370), PointerButton::Left);
    assert_eq!(
        persisted(&effects),
        [SettingChange::DrawColor(pattern_color(501, 398))]
    );

    // Leaving the zoom lets the click go
    overlay.key(Key::Char('g'), ALT);
    overlay.press(p(500, 400), PointerButton::Left);
    overlay.at_time(1_600);
    overlay.send(Event::Tick);
    overlay.send(Event::PointerMove {
        monitor: 0,
        at: p(900, 900),
        left_down: true,
        modifiers: Modifiers::NONE,
    });
    assert!(!overlay.state.color_grab().expect("grab").precise);
    let effects = overlay.release(p(900, 900), PointerButton::Left);
    assert!(overlay.state.is_grabbing_color());
    assert!(persisted(&effects).is_empty());
    assert_eq!(overlay.state.draw_color(), pattern_color(900, 900));
}

#[test]
fn space_and_the_right_button_toggle_the_preview() {
    let mut overlay = Overlay::new();
    overlay.hover(p(300, 300));
    overlay.key(Key::Char('g'), ALT);
    let zoomed = |overlay: &Overlay| overlay.state.color_grab().expect("grab").zoom.is_some();
    assert!(zoomed(&overlay));
    overlay.space();
    assert!(!zoomed(&overlay));
    assert!(!overlay.state.side_panel_open());
    overlay.press(p(300, 300), PointerButton::Right);
    overlay.release(p(300, 300), PointerButton::Right);
    assert!(zoomed(&overlay));
    assert!(overlay.state.is_grabbing_color());
    assert!(overlay.state.wheel().is_none());
}

#[test]
fn grab_color_on_the_panel_recolours_the_selected_object() {
    let mut overlay = Overlay::selected();
    overlay.rectangle(p(400, 300), p(600, 450));
    overlay.click(p(500, 380));
    let id = overlay.state.selected_object().expect("selected");
    overlay.space();
    let grab = overlay.layout().grab;
    // Grabbing starts on the press (`QPushButton::pressed`) and the panel
    // steps aside; the release that follows is not a pick
    overlay.press(center(grab), PointerButton::Left);
    assert!(overlay.state.is_grabbing_color());
    assert!(overlay.state.side_panel().is_none());
    assert!(overlay.state.side_panel_tab().is_none());
    let effects = overlay.release(center(grab), PointerButton::Left);
    assert!(overlay.state.is_grabbing_color());
    assert!(persisted(&effects).is_empty());
    overlay.hover(p(1200, 800));
    let effects = overlay.click(p(1200, 800));
    let picked = pattern_color(1200, 800);
    assert_eq!(persisted(&effects), [SettingChange::DrawColor(picked)]);
    assert_eq!(
        overlay.state.document().get(id).map(|object| object.color),
        Some(picked)
    );
    assert!(overlay.state.side_panel_open());
    assert_eq!(
        overlay.state.selection(),
        Some(LogicalRect::new(200, 200, 800, 500))
    );
}

#[test]
fn without_a_screenshot_grabbing_keeps_the_colour() {
    let mut state = OverlayState::new(
        vec![monitor_at(0, 0, 800, 600, 96)],
        0,
        Vec::new(),
        OverlayConfig::default(),
    )
    .expect("state");
    state.handle(
        Event::Key {
            key: Key::Char('g'),
            modifiers: ALT,
        },
        0,
    );
    state.handle(
        Event::PointerMove {
            monitor: 0,
            at: p(10, 10),
            left_down: false,
            modifiers: Modifiers::NONE,
        },
        0,
    );
    assert_eq!(state.draw_color(), RED);
}
