//! The side panel (15.0.0's `UtilityPanel` holding `SidePanelWidget`): the
//! active tool's size, the draw colour (a hue disc and its hex value), Grab
//! Color, the grid, and the list of placed objects. Space toggles it, and
//! the Tool Settings tab on the left edge does when `showSidePanelButton`
//! is on. Every control applies at once; there is no OK.

use phramer_annotate::{Annotation, Color, Command, Shape, ToolKind};
use phramer_capture::{LogicalPoint, LogicalRect};
use phramer_core::config::ButtonType;

use super::tools::{MAX_TOOL_SIZE, SettingChange, button_info};
use super::{Drag, Effect, Effects, Millis, OverlayState, disc_color};

/// `--dur-panel`: the panel fades and slides in and out this long.
pub(crate) const PANEL_FADE_MS: Millis = 250;
/// The grid's spacing: `m_gridSizeSpin` (5 to 50 in steps of 5, from 10).
const GRID_DEFAULT: u32 = 10;
const GRID_MIN: u32 = 5;
const GRID_MAX: u32 = 50;
const GRID_STEP: u32 = 5;
/// `MAX_INFO_LENGTH` of the text tool's list entry.
const INFO_LENGTH: usize = 24;

// The panel's layout, in logical pixels, from the overlay's spacing scale
const MARGIN: i32 = 8;
const WIDTH: i32 = 224;
const PAD: i32 = 12;
const GAP: i32 = 8;
const LABEL: i32 = 20;
const CONTROL: i32 = 24;
const BUTTON: i32 = 28;
const DISC: i32 = 136;
const HEX_WIDTH: i32 = 96;
const STEPPER_VALUE: i32 = 32;
/// The slider's thumb radius; the track stops this far from each end.
pub(crate) const THUMB: i32 = 8;
/// The Tool Settings tab: on the left edge, centred.
const TAB_WIDTH: i32 = 24;
const TAB_HEIGHT: i32 = 112;

/// A control of the side panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelControl {
    /// Active tool size.
    Slider,
    /// The colour's hex value (`m_colorHex`).
    Hex,
    /// The hue and saturation disc (`m_colorWheel`).
    Disc,
    /// Grab Color.
    Grab,
    /// Display grid.
    Grid,
    GridSmaller,
    GridLarger,
    /// Deletes the selected object.
    Delete,
    /// A row of the object list: 0 is `<Empty>`, then the objects.
    Row(usize),
    Close,
}

/// Where the side panel's parts are, in logical pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelLayout {
    pub panel: LogicalRect,
    /// "Active tool size" and the size.
    pub size_label: LogicalRect,
    pub slider: LogicalRect,
    /// "Active color".
    pub color_label: LogicalRect,
    pub hex: LogicalRect,
    /// The square the disc fills.
    pub disc: LogicalRect,
    pub grab: LogicalRect,
    /// The check box and its "Display grid" label.
    pub grid: LogicalRect,
    pub grid_smaller: LogicalRect,
    pub grid_value: LogicalRect,
    pub grid_larger: LogicalRect,
    /// The hairline above the object list.
    pub separator_y: i32,
    pub delete: LogicalRect,
    /// The list rows that fit, top first.
    pub rows: Vec<LogicalRect>,
    pub close: LogicalRect,
}

/// The side panel's layout on a monitor of logical size `screen`, with
/// room for up to `rows` list rows. A monitor too short for the controls
/// shows no rows.
pub fn side_panel_layout(screen: LogicalRect, rows: usize) -> PanelLayout {
    let left = screen.x + MARGIN;
    let top = screen.y + MARGIN;
    let x = left + PAD;
    let inner = WIDTH - 2 * PAD;
    let right = x + inner;
    let mut y = top + PAD;
    let size_label = LogicalRect::new(x, y, inner, LABEL);
    y += LABEL + GAP / 2;
    let slider = LogicalRect::new(x, y, inner, LABEL);
    y += LABEL + GAP;
    let color_label = LogicalRect::new(x, y, inner - HEX_WIDTH - GAP, CONTROL);
    let hex = LogicalRect::new(right - HEX_WIDTH, y, HEX_WIDTH, CONTROL);
    y += CONTROL + GAP;
    let disc = LogicalRect::new(x + (inner - DISC) / 2, y, DISC, DISC);
    y += DISC + GAP;
    let grab = LogicalRect::new(x, y, inner, BUTTON);
    y += BUTTON + GAP;
    let grid_larger = LogicalRect::new(right - CONTROL, y, CONTROL, CONTROL);
    let grid_value = LogicalRect::new(grid_larger.x - STEPPER_VALUE, y, STEPPER_VALUE, CONTROL);
    let grid_smaller = LogicalRect::new(grid_value.x - CONTROL, y, CONTROL, CONTROL);
    let grid = LogicalRect::new(x, y, grid_smaller.x - GAP - x, CONTROL);
    y += CONTROL + GAP;
    let separator_y = y;
    y += 1 + GAP;
    let delete = LogicalRect::new(x, y, CONTROL, CONTROL);
    y += CONTROL + GAP / 2;
    let fixed = y - top + GAP + BUTTON + PAD;
    let room = (screen.height - 2 * MARGIN - fixed).max(0) / CONTROL;
    let count = rows.min(usize::try_from(room).unwrap_or(0));
    let rows: Vec<LogicalRect> = (0..count)
        .map(|index| LogicalRect::new(x, y + CONTROL * index as i32, inner, CONTROL))
        .collect();
    y += CONTROL * count as i32 + GAP;
    let close = LogicalRect::new(x, y, inner, BUTTON);
    let panel = LogicalRect::new(left, top, WIDTH, close.bottom() + PAD - top);
    PanelLayout {
        panel,
        size_label,
        slider,
        color_label,
        hex,
        disc,
        grab,
        grid,
        grid_smaller,
        grid_value,
        grid_larger,
        separator_y,
        delete,
        rows,
        close,
    }
}

/// The Tool Settings tab on a monitor of logical size `screen`.
pub fn side_panel_tab(screen: LogicalRect) -> LogicalRect {
    LogicalRect::new(
        screen.x,
        screen.y + (screen.height - TAB_HEIGHT) / 2,
        TAB_WIDTH,
        TAB_HEIGHT,
    )
}

/// The hex value being typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HexEdit {
    text: String,
}

/// The side panel's own state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Panel {
    pub(crate) open: bool,
    /// When it last opened or closed; `None` before the first time.
    pub(crate) since: Option<Millis>,
    pub(crate) hex: Option<HexEdit>,
    /// Display grid, and its spacing in logical pixels.
    pub(crate) grid: bool,
    pub(crate) grid_size: u32,
    /// The slider or disc being dragged, or the control pressed.
    pub(crate) held: Option<PanelControl>,
    pub(crate) hovered: Option<PanelControl>,
    /// The first object row shown.
    pub(crate) scroll: usize,
}

impl Panel {
    pub(crate) fn new() -> Self {
        Self {
            open: false,
            since: None,
            hex: None,
            grid: false,
            grid_size: GRID_DEFAULT,
            held: None,
            hovered: None,
            scroll: 0,
        }
    }
}

/// One row of the object list as drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelRow {
    pub label: String,
    /// A name in [`crate::ICONS`]; none for `<Empty>`.
    pub icon: Option<&'static str>,
    pub selected: bool,
}

/// The side panel as drawn.
#[derive(Debug, Clone, PartialEq)]
pub struct SidePanelView {
    pub layout: PanelLayout,
    /// 0 to 1 through the fade, linear; 1 when settled.
    pub progress: f32,
    /// Fading in (or open); false while it fades out.
    pub opening: bool,
    pub size: u32,
    pub color: Color,
    /// The hex value, as typed while the field has the keyboard.
    pub hex: String,
    pub hex_focused: bool,
    pub grid: bool,
    pub grid_size: u32,
    /// An object is selected, so Delete does something.
    pub can_delete: bool,
    /// The list rows that fit, from the scroll position.
    pub rows: Vec<PanelRow>,
    /// The list entry the first row shows (`PanelControl::Row` numbers
    /// entries from the top of the list).
    pub first_row: usize,
    pub hovered: Option<PanelControl>,
    pub held: Option<PanelControl>,
}

/// `#rrggbb`, as `QColor::name()` writes it.
pub(crate) fn hex_name(color: Color) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b)
}

/// A typed colour: `#rgb` or `#rrggbb`.
fn parse_hex(text: &str) -> Option<Color> {
    let digits = text.strip_prefix('#')?;
    let value = |range: &str| u8::from_str_radix(range, 16).ok();
    match digits.len() {
        3 => {
            let mut channels = digits.chars().map(|c| {
                let v = c.to_digit(16)? as u8;
                Some(v * 16 + v)
            });
            let r = channels.next()??;
            let g = channels.next()??;
            let b = channels.next()??;
            Some(Color::rgb(r, g, b))
        }
        6 => Some(Color::rgb(
            value(digits.get(0..2)?)?,
            value(digits.get(2..4)?)?,
            value(digits.get(4..6)?)?,
        )),
        _ => None,
    }
}

/// Where a colour sits on the hue disc: hue in degrees and saturation.
pub fn disc_position(color: Color) -> (f64, f64) {
    let (r, g, b) = (
        f64::from(color.r) / 255.0,
        f64::from(color.g) / 255.0,
        f64::from(color.b) / 255.0,
    );
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    if max <= 0.0 || delta <= 0.0 {
        return (0.0, 0.0);
    }
    let hue = if max == r {
        60.0 * ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    (hue, delta / max)
}

/// The list entry of an object (`CaptureTool::info()`).
fn object_row(object: &Annotation) -> (String, &'static str) {
    let named = |button: ButtonType| {
        let info = button_info(button);
        (info.name.to_owned(), info.icon)
    };
    match &object.shape {
        Shape::Text { text, .. } => {
            let info = button_info(ButtonType::Text);
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return (info.name.to_owned(), info.icon);
            }
            let full = format!("{} - {trimmed}", info.name);
            let line = full.split('\n').next().unwrap_or_default();
            let label = if line.chars().count() > INFO_LENGTH {
                line.chars().take(INFO_LENGTH).chain(Some('…')).collect()
            } else {
                line.to_owned()
            };
            (label, info.icon)
        }
        Shape::CircleCounter { count, .. } => {
            let info = button_info(ButtonType::CircleCount);
            (format!("{} - {count}", info.name), info.icon)
        }
        shape => named(match shape.kind() {
            ToolKind::Pencil => ButtonType::Pencil,
            ToolKind::Line => ButtonType::Line,
            ToolKind::Arrow => ButtonType::Arrow,
            ToolKind::Ellipse => ButtonType::Circle,
            ToolKind::Marker => ButtonType::Marker,
            ToolKind::Pixelate => ButtonType::Pixelate,
            ToolKind::Invert => ButtonType::Invert,
            _ => ButtonType::Rectangle,
        }),
    }
}

impl OverlayState {
    // --- What the renderer reads --------------------------------------------

    /// Whether the side panel is open (it may still be fading in).
    pub fn side_panel_open(&self) -> bool {
        self.panel.open
    }

    /// Display grid's spacing in logical pixels, while it shows.
    pub fn grid(&self) -> Option<u32> {
        self.panel.grid.then_some(self.panel.grid_size)
    }

    fn panel_progress(&self) -> f32 {
        match self.panel.since {
            Some(since) if !self.config.reduced_motion => {
                (self.now.saturating_sub(since) as f32 / PANEL_FADE_MS as f32).min(1.0)
            }
            _ => 1.0,
        }
    }

    /// Whether the panel's fade still runs, for the next frame.
    pub(crate) fn panel_animating(&self) -> bool {
        !self.config.reduced_motion
            && self
                .panel
                .since
                .is_some_and(|since| self.now < since + PANEL_FADE_MS)
    }

    fn object_count(&self) -> usize {
        self.tools.editor.document().objects().len()
    }

    /// The list's first row shown: as scrolled, but never past the point
    /// where the last entry is at the bottom (objects go with undo and
    /// Delete).
    fn list_scroll(&self) -> usize {
        let total = self.object_count() + 1;
        let shown = side_panel_layout(self.monitor_rect(), total).rows.len();
        self.panel.scroll.min(total.saturating_sub(shown))
    }

    fn layout(&self) -> PanelLayout {
        let total = self.object_count() + 1;
        side_panel_layout(
            self.monitor_rect(),
            total.saturating_sub(self.list_scroll()),
        )
    }

    /// The side panel, while it is open or fading out.
    pub fn side_panel(&self) -> Option<SidePanelView> {
        let progress = self.panel_progress();
        if !self.panel.open && progress >= 1.0 {
            return None;
        }
        let layout = self.layout();
        let objects = self.tools.editor.document().objects();
        let scroll = self.list_scroll();
        let rows = (scroll..scroll + layout.rows.len())
            .filter_map(|index| match index.checked_sub(1) {
                None => Some(PanelRow {
                    label: "<Empty>".to_owned(),
                    icon: None,
                    selected: self.tools.selected.is_none(),
                }),
                Some(object) => {
                    let object = objects.get(object)?;
                    let (label, icon) = object_row(object);
                    Some(PanelRow {
                        label,
                        icon: Some(icon),
                        selected: self.tools.selected == Some(object.id),
                    })
                }
            })
            .collect();
        Some(SidePanelView {
            layout,
            progress,
            opening: self.panel.open,
            size: self.tools.size,
            color: self.tools.color,
            hex: match &self.panel.hex {
                Some(edit) => edit.text.clone(),
                None => hex_name(self.tools.color),
            },
            hex_focused: self.panel.hex.is_some(),
            grid: self.panel.grid,
            grid_size: self.panel.grid_size,
            can_delete: self.tools.selected.is_some(),
            rows,
            first_row: scroll,
            hovered: self.panel.hovered,
            held: self.panel.held,
        })
    }

    /// The Tool Settings tab, when it shows.
    pub fn side_panel_tab(&self) -> Option<LogicalRect> {
        (self.config.side_panel_button && !self.panel.open && self.grab.is_none())
            .then(|| side_panel_tab(self.monitor_rect()))
    }

    /// Whether `at` is on the open panel or the tab.
    pub(crate) fn over_panel(&self, at: LogicalPoint) -> bool {
        (self.panel.open && self.layout().panel.contains(at))
            || self.side_panel_tab().is_some_and(|tab| tab.contains(at))
    }

    fn panel_control_at(&self, at: LogicalPoint) -> Option<PanelControl> {
        if !self.panel.open {
            return None;
        }
        let layout = self.layout();
        let fixed = [
            (layout.slider, PanelControl::Slider),
            (layout.hex, PanelControl::Hex),
            (layout.grab, PanelControl::Grab),
            (layout.grid, PanelControl::Grid),
            (layout.grid_smaller, PanelControl::GridSmaller),
            (layout.grid_larger, PanelControl::GridLarger),
            (layout.delete, PanelControl::Delete),
            (layout.close, PanelControl::Close),
        ];
        if let Some((_, control)) = fixed.iter().find(|(rect, _)| rect.contains(at)) {
            return Some(*control);
        }
        if disc_offset(&layout, at).is_some_and(|(dx, dy, radius)| dx.hypot(dy) <= radius) {
            return Some(PanelControl::Disc);
        }
        layout
            .rows
            .iter()
            .position(|row| row.contains(at))
            .map(|index| PanelControl::Row(self.list_scroll() + index))
    }

    // --- Opening and closing -------------------------------------------------

    /// Space, the tab or Close (`UtilityPanel::toggle`).
    pub(crate) fn toggle_panel(&mut self, effects: &mut Effects) {
        if self.panel.open {
            self.close_panel(effects);
        } else {
            self.open_panel(effects);
        }
    }

    pub(crate) fn open_panel(&mut self, effects: &mut Effects) {
        if self.panel.open {
            return;
        }
        self.panel.open = true;
        self.panel.since = Some(self.now);
        self.panel.hovered = None;
        effects.redraw(self.active);
    }

    pub(crate) fn close_panel(&mut self, effects: &mut Effects) {
        if !self.panel.open {
            return;
        }
        self.finish_hex(effects);
        self.panel.open = false;
        self.panel.since = Some(self.now);
        self.panel.held = None;
        self.panel.hovered = None;
        effects.redraw(self.active);
    }

    // --- Pointer ---------------------------------------------------------------

    /// A left press on the panel or the tab. Returns false when it is not
    /// theirs. A press anywhere ends typing in the hex field first.
    pub(crate) fn panel_press(&mut self, at: LogicalPoint, effects: &mut Effects) -> bool {
        let control = self.panel_control_at(at);
        if control != Some(PanelControl::Hex) {
            self.finish_hex(effects);
        }
        if self.side_panel_tab().is_some_and(|tab| tab.contains(at)) {
            self.commit_text(effects);
            self.drag = Drag::Panel;
            self.open_panel(effects);
            return true;
        }
        if !self.panel.open || !self.layout().panel.contains(at) {
            return false;
        }
        self.commit_text(effects);
        self.drag = Drag::Panel;
        self.panel.held = control;
        effects.redraw(self.active);
        match control {
            Some(PanelControl::Slider) => self.slide_to(at, effects),
            Some(PanelControl::Disc) => self.disc_to(at, effects),
            Some(PanelControl::Hex) => {
                if self.panel.hex.is_none() {
                    self.panel.hex = Some(HexEdit {
                        text: hex_name(self.tools.color),
                    });
                }
            }
            Some(PanelControl::Grab) => {
                self.drag = Drag::Idle;
                self.panel.held = None;
                self.start_grab(effects);
            }
            Some(PanelControl::Grid) => self.panel.grid = !self.panel.grid,
            Some(PanelControl::GridSmaller) if self.panel.grid => {
                self.panel.grid_size = self.panel.grid_size.saturating_sub(GRID_STEP).max(GRID_MIN);
            }
            Some(PanelControl::GridLarger) if self.panel.grid => {
                self.panel.grid_size = (self.panel.grid_size + GRID_STEP).min(GRID_MAX);
            }
            Some(PanelControl::Delete) => self.delete_selected(effects),
            Some(PanelControl::Row(index)) => {
                let objects = self.tools.editor.document().objects();
                self.tools.selected = index
                    .checked_sub(1)
                    .and_then(|object| objects.get(object))
                    .map(|object| object.id);
            }
            Some(PanelControl::Close) => self.close_panel(effects),
            _ => {}
        }
        true
    }

    /// The pointer moved with a panel press held: the slider and the disc
    /// follow it.
    pub(crate) fn panel_drag(&mut self, at: LogicalPoint, effects: &mut Effects) {
        match self.panel.held {
            Some(PanelControl::Slider) => self.slide_to(at, effects),
            Some(PanelControl::Disc) => self.disc_to(at, effects),
            _ => {}
        }
    }

    /// The press on the panel ended: a size or colour dragged to is saved.
    pub(crate) fn panel_release(&mut self, effects: &mut Effects) {
        match self.panel.held.take() {
            Some(PanelControl::Slider) => {
                if let Some(button) = self.tools.active {
                    effects.push(Effect::Persist(SettingChange::ToolSize {
                        button,
                        size: self.tools.size,
                    }));
                }
            }
            Some(PanelControl::Disc) => {
                let color = self.tools.color;
                self.apply_color(color, effects);
            }
            _ => {}
        }
        effects.redraw(self.active);
    }

    /// Hover over the panel, with no button held.
    pub(crate) fn panel_hover(&mut self, at: LogicalPoint, effects: &mut Effects) {
        let hovered = self.panel_control_at(at);
        if hovered != self.panel.hovered {
            self.panel.hovered = hovered;
            effects.redraw(self.active);
        }
    }

    /// The wheel over the object list scrolls it. Returns false when the
    /// wheel is not over the panel.
    pub(crate) fn panel_wheel(
        &mut self,
        at: LogicalPoint,
        delta: i32,
        effects: &mut Effects,
    ) -> bool {
        if !self.panel.open || !self.layout().panel.contains(at) {
            return false;
        }
        let total = self.object_count() + 1;
        let shown = side_panel_layout(self.monitor_rect(), total).rows.len();
        let last = total.saturating_sub(shown);
        let current = self.list_scroll();
        let scroll = if delta > 0 {
            current.saturating_sub(1)
        } else if delta < 0 {
            (current + 1).min(last)
        } else {
            current
        };
        if scroll != self.panel.scroll {
            self.panel.scroll = scroll;
            effects.redraw(self.active);
        }
        true
    }

    fn slide_to(&mut self, at: LogicalPoint, effects: &mut Effects) {
        let slider = self.layout().slider;
        let left = slider.x + THUMB;
        let span = (slider.width - 2 * THUMB).max(1);
        let fraction = f64::from((at.x - left).clamp(0, span)) / f64::from(span);
        let size = 1 + (fraction * f64::from(MAX_TOOL_SIZE - 1)).round() as u32;
        if size != self.tools.size {
            self.apply_size(size, effects);
        }
    }

    fn disc_to(&mut self, at: LogicalPoint, effects: &mut Effects) {
        let layout = self.layout();
        let Some((dx, dy, radius)) = disc_offset(&layout, at) else {
            return;
        };
        let hue = (-dy).atan2(dx).to_degrees();
        let color = disc_color(hue, (dx.hypot(dy) / radius).min(1.0));
        if color != self.tools.color {
            self.tools.color = color;
            effects.redraw(self.active);
        }
    }

    /// Takes `color` as the draw colour: saved, and given to the selected
    /// object (`CaptureWidget::setDrawColor`).
    pub(crate) fn apply_color(&mut self, color: Color, effects: &mut Effects) {
        self.tools.color = color;
        effects.push(Effect::Persist(SettingChange::DrawColor(color)));
        if let Some(id) = self.tools.selected
            && let Err(error) = self.tools.editor.apply(Command::SetColor { id, color })
        {
            log::warn!("cannot recolour object {id}: {error}");
        }
        effects.redraw(self.active);
    }

    // --- The hex field ---------------------------------------------------------

    /// Whether the hex field has the keyboard.
    pub(crate) fn typing_hex(&self) -> bool {
        self.panel.hex.is_some()
    }

    /// `editingFinished`: a valid colour is taken, anything else is
    /// dropped and the field shows the colour again.
    pub(crate) fn finish_hex(&mut self, effects: &mut Effects) {
        let Some(edit) = self.panel.hex.take() else {
            return;
        };
        effects.redraw(self.active);
        if let Some(color) = parse_hex(&edit.text) {
            self.apply_color(color, effects);
        }
    }

    /// Keys while the hex field has the keyboard: Enter and Esc finish (Esc
    /// only takes the focus away, as 15.0.0 did, which finishes too).
    pub(crate) fn hex_key(&mut self, key: crate::input::Key, effects: &mut Effects) {
        use crate::input::Key;
        match key {
            Key::Return | Key::Enter | Key::Escape | Key::Tab => self.finish_hex(effects),
            Key::Backspace => {
                if let Some(edit) = self.panel.hex.as_mut() {
                    edit.text.pop();
                    effects.redraw(self.active);
                }
            }
            _ => {}
        }
    }

    /// A character typed into the hex field.
    pub(crate) fn hex_typed(&mut self, c: char, effects: &mut Effects) {
        let Some(edit) = self.panel.hex.as_mut() else {
            return;
        };
        let accepted = c.is_ascii_hexdigit() || (c == '#' && edit.text.is_empty());
        if accepted && edit.text.chars().count() < 7 {
            edit.text.push(c.to_ascii_lowercase());
            effects.redraw(self.active);
        }
    }
}

/// From the disc's centre to `at`, and its radius, in logical pixels.
fn disc_offset(layout: &PanelLayout, at: LogicalPoint) -> Option<(f64, f64, f64)> {
    let disc = layout.disc;
    let radius = f64::from(disc.width) / 2.0;
    if radius <= 0.0 {
        return None;
    }
    let cx = f64::from(disc.x) + radius;
    let cy = f64::from(disc.y) + radius;
    Some((f64::from(at.x) - cx, f64::from(at.y) - cy, radius))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_values_parse_like_qcolor_names() {
        assert_eq!(parse_hex("#ff8000"), Some(Color::rgb(255, 128, 0)));
        assert_eq!(parse_hex("#f80"), Some(Color::rgb(255, 136, 0)));
        assert_eq!(parse_hex("ff8000"), None);
        assert_eq!(parse_hex("#ff80"), None);
        assert_eq!(parse_hex("#gg0000"), None);
        assert_eq!(hex_name(Color::rgb(1, 171, 255)), "#01abff");
    }

    #[test]
    fn the_disc_position_inverts_the_disc_colour() {
        for (hue, saturation) in [(0.0, 1.0), (120.0, 0.5), (240.0, 0.25), (300.0, 1.0)] {
            let (h, s) = disc_position(disc_color(hue, saturation));
            assert!((h - hue).abs() < 1.5, "{hue} -> {h}");
            assert!((s - saturation).abs() < 0.01, "{saturation} -> {s}");
        }
        assert_eq!(disc_position(Color::rgb(255, 255, 255)), (0.0, 0.0));
    }

    #[test]
    fn the_layout_fits_rows_to_the_monitor() {
        let tall = side_panel_layout(LogicalRect::new(0, 0, 1920, 1080), 3);
        assert_eq!(tall.rows.len(), 3);
        assert!(tall.panel.bottom() <= 1080 - MARGIN);
        let many = side_panel_layout(LogicalRect::new(0, 0, 1920, 1080), 500);
        assert!(many.panel.bottom() <= 1080 - MARGIN);
        assert!(many.rows.len() > 10);
        let short = side_panel_layout(LogicalRect::new(0, 0, 640, 400), 3);
        assert!(short.rows.is_empty());
        for rect in [tall.slider, tall.hex, tall.disc, tall.grab, tall.close] {
            assert!(tall.panel.contains(LogicalPoint::new(rect.x, rect.y)));
        }
    }
}
