//! The annotation tools on the selection: the toolbar, drawing through the
//! [`Editor`], the colour wheel, tool sizes and text entry. 15.0.0's
//! `CaptureWidget` tool handling, as events in and effects out.
//!
//! The document's image is the whole monitor in physical pixels, so a
//! pointer position is mapped through the monitor's scale, and tool sizes
//! (logical, as the settings hold them) are scaled the same way: a line of
//! size 3 is as thick on a 200% monitor as 15.0.0 drew it there.

use std::sync::Arc;

use phramer_annotate::{
    Annotation, Color, Command, Editor, FontLibrary, ObjectId, Point, Rect, Shape, ShapeStyle,
    TextAlign, TextFont, ToolKind, constrain, text_top_left_for_anchor,
};
use phramer_capture::{LogicalPoint, LogicalRect};
use phramer_core::config::ButtonType;

use super::toolbar::{BUTTON_SIZE, SEPARATOR, button_at, layout};
use super::tools::{
    MAX_TOOL_SIZE, SettingChange, ShapeKind, ToolSettings, button_info, draws, is_tool, offered,
};
use super::{DeferredAction, Drag, Effect, Effects, ExportAction, Millis, OverlayState, manhattan};
use crate::input::{Key, Modifiers};

/// How long the size notice stays up after the last change; the size is
/// saved when it goes (`NotifierBox`, 600 ms).
const SIZE_NOTICE_MS: Millis = 600;
/// A wheel turn of at least this much is one notch; less is a touchpad,
/// which steps at most once per [`TOUCHPAD_STEP_MS`] (`ToolSizeWheel`).
const WHEEL_NOTCH: i32 = 60;
const TOUCHPAD_STEP_MS: Millis = 200;
/// `--dur-fast`: the hover fade of a toolbar button.
pub(crate) const HOVER_FADE_MS: Millis = 100;
/// One animation frame.
const FRAME_MS: Millis = 16;
/// A colour swatch's radius on the wheel (`--space-8` + `--space-2`).
pub(crate) const SWATCH_RADIUS: i32 = 10;
/// How far past a swatch the pointer still picks it (`--space-4`).
const SWATCH_REACH: i32 = 4;
/// The id previews are drawn with; never one of the document's.
pub(crate) const PREVIEW_ID: ObjectId = ObjectId(u64::MAX);

/// Fonts for text objects and counters; shared, as states are cloned.
#[derive(Clone)]
pub(crate) struct Fonts(pub(crate) Arc<FontLibrary>);

impl std::fmt::Debug for Fonts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Fonts")
    }
}

impl Fonts {
    pub(crate) fn bundled() -> Self {
        let mut library = FontLibrary::new();
        if let Err(error) = library.add("Inter", false, false, crate::BUNDLED_FONT.to_vec()) {
            log::error!("overlay font: {error}");
        }
        Self(Arc::new(library))
    }
}

/// Text being typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TextEdit {
    top_left: Point,
    text: String,
    /// In characters.
    caret: usize,
}

/// What the colour wheel points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Spot {
    Swatch(usize),
    /// A colour from the custom disc.
    Custom(Color),
}

/// The right-button colour wheel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Wheel {
    center: LogicalPoint,
    hovered: Option<Spot>,
    /// The colour before the wheel opened, for Esc.
    original: Color,
    /// The custom colour disc is showing.
    custom: bool,
}

/// The colour wheel as drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WheelView {
    pub center: LogicalPoint,
    pub radius: i32,
    pub swatch_radius: i32,
    /// Each entry's centre and colour; `None` is the custom colour.
    pub swatches: Vec<(LogicalPoint, Option<Color>)>,
    /// The swatch under the pointer, or holding the current colour.
    pub highlighted: Option<usize>,
    /// The radius of the custom colour disc, when it shows.
    pub custom_radius: Option<i32>,
    /// The colour being drawn with.
    pub color: Color,
}

/// One toolbar button as drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolbarButton {
    pub button: ButtonType,
    pub rect: LogicalRect,
    /// The selected tool.
    pub active: bool,
    pub hovered: bool,
    pub pressed: bool,
    /// Undo or redo with nothing to undo or redo.
    pub enabled: bool,
}

/// One entry of the Shape tool's menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShapeOption {
    pub kind: ShapeKind,
    pub style: ShapeStyle,
    pub rect: LogicalRect,
    pub active: bool,
    pub hovered: bool,
}

/// The six Shape choices (`ShapeTool`'s menu).
const SHAPE_OPTIONS: [(ShapeKind, ShapeStyle); 6] = [
    (ShapeKind::Square, ShapeStyle::Outline),
    (ShapeKind::Square, ShapeStyle::Filled),
    (ShapeKind::Square, ShapeStyle::Highlight),
    (ShapeKind::Circle, ShapeStyle::Outline),
    (ShapeKind::Circle, ShapeStyle::Filled),
    (ShapeKind::Circle, ShapeStyle::Highlight),
];

/// Everything the tools add to the overlay's state.
#[derive(Debug, Clone)]
pub(crate) struct Tools {
    pub(crate) editor: Editor,
    pub(crate) settings: ToolSettings,
    /// The buttons on the toolbar, in order.
    pub(crate) buttons: Vec<ButtonType>,
    pub(crate) active: Option<ButtonType>,
    pub(crate) color: Color,
    /// In logical pixels, as the settings and the size notice have it.
    pub(crate) size: u32,
    /// The object being drawn.
    draft: Option<Annotation>,
    pub(crate) text: Option<TextEdit>,
    pub(crate) selected: Option<ObjectId>,
    /// The selected object's offset while it is dragged, in image pixels.
    moving: Option<(i32, i32)>,
    pub(crate) wheel: Option<Wheel>,
    pub(crate) shape_menu: bool,
    /// The size notice shows until then.
    pub(crate) notice_until: Option<Millis>,
    /// The size typed so far with the digit keys.
    digits: Option<u32>,
    /// When the touchpad last stepped the size.
    touchpad_step: Option<Millis>,
    /// The toolbar button (or shape option, past the buttons) under the
    /// pointer, and since when.
    pub(crate) hover: Option<usize>,
    pub(crate) hover_since: Millis,
    pub(crate) fonts: Fonts,
}

impl Tools {
    pub(crate) fn new(editor: Editor, settings: &ToolSettings, has_tasks: bool) -> Self {
        let buttons = settings
            .buttons
            .iter()
            .copied()
            .filter(|button| offered(*button, has_tasks))
            .collect();
        Self {
            editor,
            buttons,
            active: None,
            color: settings.color,
            size: settings.size_of(None),
            draft: None,
            text: None,
            selected: None,
            moving: None,
            wheel: None,
            shape_menu: false,
            notice_until: None,
            digits: None,
            touchpad_step: None,
            hover: None,
            hover_since: 0,
            fonts: Fonts::bundled(),
            settings: settings.clone(),
        }
    }
}

/// HSV with full value to RGB, for the custom disc: `hue` in degrees,
/// `saturation` 0 to 1.
pub(crate) fn disc_color(hue: f64, saturation: f64) -> Color {
    let h = hue.rem_euclid(360.0) / 60.0;
    let s = saturation.clamp(0.0, 1.0);
    let chroma = s;
    let x = chroma * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let m = 1.0 - chroma;
    let byte = |v: f64| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    Color::rgb(byte(r), byte(g), byte(b))
}

/// `shape` moved by whole pixels (what `Command::Move` does).
fn translated(shape: &Shape, dx: i32, dy: i32) -> Shape {
    let t = |p: Point| Point::new(p.x.saturating_add(dx), p.y.saturating_add(dy));
    let mut shape = shape.clone();
    match &mut shape {
        Shape::Pencil { points } => {
            for point in points {
                *point = t(*point);
            }
        }
        Shape::Line { start, end }
        | Shape::Arrow { start, end, .. }
        | Shape::Rectangle { start, end, .. }
        | Shape::Ellipse { start, end, .. }
        | Shape::Marker { start, end }
        | Shape::Pixelate { start, end, .. }
        | Shape::Invert { start, end } => {
            *start = t(*start);
            *end = t(*end);
        }
        Shape::Text { top_left, .. } => *top_left = t(*top_left),
        Shape::CircleCounter {
            center, pointer, ..
        } => {
            *center = t(*center);
            *pointer = t(*pointer);
        }
    }
    shape
}

impl OverlayState {
    // --- Geometry ------------------------------------------------------------

    pub(crate) fn scale(&self) -> f64 {
        self.monitors[self.active].scale()
    }

    /// A pointer position in image pixels.
    pub(crate) fn image_point(&self, at: LogicalPoint) -> Point {
        let monitor = &self.monitors[self.active];
        let physical = monitor.logical_point_to_physical(at);
        let origin = monitor.bounds();
        Point::new(physical.x - origin.x, physical.y - origin.y)
    }

    /// A tool size as an object's `size`, at the monitor's scale.
    fn object_size(&self, kind: ToolKind, size: u32) -> u32 {
        let scale = self.scale();
        let scaled = |base: u32| (f64::from(size + base) * scale).round() as u32;
        match kind {
            // Point size is size + 8; the bubble radius size + 15; the
            // marker is 14 wider than the tool size
            ToolKind::Text => scaled(8).saturating_sub(8),
            ToolKind::CircleCounter => scaled(15).saturating_sub(15),
            ToolKind::Marker => scaled(14),
            // Strength, not a distance
            ToolKind::Pixelate | ToolKind::Invert => size,
            _ => scaled(0).max(1),
        }
    }

    fn highlight_outline(&self) -> u32 {
        (f64::from(self.tools.settings.highlight_outline) * self.scale()).round() as u32
    }

    /// The object the active tool draws between two points.
    fn shape_between(&self, tool: ButtonType, start: Point, end: Point) -> Option<Shape> {
        let settings = &self.tools.settings;
        let outline_width = self.highlight_outline();
        let closed = |kind: ShapeKind, style: ShapeStyle| match kind {
            ShapeKind::Square => Shape::Rectangle {
                start,
                end,
                style,
                outline_width,
            },
            ShapeKind::Circle => Shape::Ellipse {
                start,
                end,
                style,
                outline_width,
            },
        };
        Some(match tool {
            ButtonType::Pencil => Shape::Pencil {
                points: vec![start, end],
            },
            ButtonType::Line => Shape::Line { start, end },
            ButtonType::Arrow => Shape::Arrow {
                start,
                end,
                head: settings.arrow_head,
                reversed: settings.reverse_arrow,
            },
            ButtonType::Shape => closed(settings.shape_kind, settings.shape_style),
            ButtonType::Selection => closed(ShapeKind::Square, ShapeStyle::Outline),
            ButtonType::Rectangle => closed(ShapeKind::Square, settings.rectangle_style),
            ButtonType::Circle => closed(ShapeKind::Circle, ShapeStyle::Outline),
            ButtonType::Marker => Shape::Marker { start, end },
            ButtonType::Pixelate => Shape::Pixelate {
                start,
                end,
                style: settings.redact,
            },
            ButtonType::Invert => Shape::Invert { start, end },
            ButtonType::CircleCount => Shape::CircleCounter {
                center: start,
                pointer: end,
                count: self.tools.editor.document().next_counter(),
            },
            _ => return None,
        })
    }

    // --- What the renderer reads ------------------------------------------------

    /// The selected tool.
    pub fn active_tool(&self) -> Option<ButtonType> {
        self.tools.active
    }

    /// The colour new objects get.
    pub fn draw_color(&self) -> Color {
        self.tools.color
    }

    /// The tool size, logical, 1 to 50.
    pub fn tool_size(&self) -> u32 {
        self.tools.size
    }

    /// The selected object.
    pub fn selected_object(&self) -> Option<ObjectId> {
        self.tools.selected
    }

    /// The text being typed, if any.
    pub fn text_being_edited(&self) -> Option<&str> {
        self.tools.text.as_ref().map(|edit| edit.text.as_str())
    }

    /// The size notice's number while it shows.
    pub fn size_notice(&self) -> Option<u32> {
        self.tools.notice_until.map(|_| self.tools.size)
    }

    pub fn can_undo(&self) -> bool {
        self.tools.editor.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.tools.editor.can_redo()
    }

    /// Whether the toolbar shows: there is a selection and it is not being
    /// dragged.
    pub fn toolbar_visible(&self) -> bool {
        self.selection.is_some()
            && !self.following
            && !matches!(
                self.drag,
                Drag::Pending { .. } | Drag::Resizing { .. } | Drag::Moving { .. }
            )
    }

    fn button_rects(&self) -> (Vec<LogicalRect>, bool) {
        match self.selection {
            Some(selection) if self.toolbar_visible() => {
                layout(selection, self.monitor_rect(), self.tools.buttons.len())
            }
            _ => (Vec::new(), false),
        }
    }

    /// The toolbar's buttons where they are drawn; empty while hidden.
    pub fn toolbar(&self) -> Vec<ToolbarButton> {
        let (rects, inside) = self.button_rects();
        // Buttons laid inside the selection make way while drawing there
        if inside && matches!(self.drag, Drag::Drawing | Drag::MovingObject { .. }) {
            return Vec::new();
        }
        let pressed = match self.drag {
            Drag::Button(index) => Some(index),
            _ => None,
        };
        rects
            .into_iter()
            .zip(&self.tools.buttons)
            .enumerate()
            .map(|(index, (rect, button))| ToolbarButton {
                button: *button,
                rect,
                active: self.tools.active == Some(*button),
                hovered: self.tools.hover == Some(index),
                pressed: pressed == Some(index),
                enabled: match button {
                    ButtonType::Undo => self.tools.editor.can_undo(),
                    ButtonType::Redo => self.tools.editor.can_redo(),
                    _ => true,
                },
            })
            .collect()
    }

    /// How far the hovered button's highlight has faded in, 0 to 1.
    pub fn hover_progress(&self) -> f32 {
        if self.config.reduced_motion {
            return 1.0;
        }
        let elapsed = self.now.saturating_sub(self.tools.hover_since);
        (elapsed as f32 / HOVER_FADE_MS as f32).min(1.0)
    }

    /// The hovered button's tooltip: its description and shortcut
    /// (`CaptureToolButton::initButton`), and the button it belongs to.
    pub fn tooltip(&self) -> Option<(LogicalRect, String)> {
        let toolbar = self.toolbar();
        let hovered = toolbar.iter().find(|button| button.hovered)?;
        let info = button_info(hovered.button);
        let mut text = info.description.to_owned();
        if let Some(shortcut) = self
            .config
            .bindings
            .shortcut(crate::input::button_action(hovered.button))
        {
            text.push_str(&format!(" ({})", shortcut.label()));
        }
        Some((hovered.rect, text))
    }

    /// The Shape tool's menu, when open.
    pub fn shape_menu(&self) -> Vec<ShapeOption> {
        if !self.tools.shape_menu {
            return Vec::new();
        }
        let toolbar = self.toolbar();
        let Some(anchor) = toolbar
            .iter()
            .find(|button| button.button == ButtonType::Shape)
            .map(|button| button.rect)
        else {
            return Vec::new();
        };
        let step = BUTTON_SIZE + SEPARATOR;
        let width = step * SHAPE_OPTIONS.len() as i32 - SEPARATOR;
        let screen = self.monitor_rect();
        let x = (anchor.x + BUTTON_SIZE / 2 - width / 2).clamp(0, (screen.width - width).max(0));
        // Away from the selection: below a button under it, else above
        let below = self
            .selection
            .is_some_and(|selection| anchor.y >= selection.bottom());
        let y = if below && anchor.bottom() + step <= screen.height {
            anchor.y + step
        } else if anchor.y - step >= 0 {
            anchor.y - step
        } else {
            anchor.y + step
        };
        let first_option = self.tools.buttons.len();
        SHAPE_OPTIONS
            .iter()
            .enumerate()
            .map(|(index, (kind, style))| ShapeOption {
                kind: *kind,
                style: *style,
                rect: LogicalRect::new(x + step * index as i32, y, BUTTON_SIZE, BUTTON_SIZE),
                active: self.tools.settings.shape_kind == *kind
                    && self.tools.settings.shape_style == *style,
                hovered: self.tools.hover == Some(first_option + index),
            })
            .collect()
    }

    /// The colour wheel, when open.
    pub fn wheel(&self) -> Option<WheelView> {
        let wheel = self.tools.wheel?;
        let palette = &self.tools.settings.palette;
        let radius = wheel_radius(palette.len());
        let swatches = swatch_centers(wheel.center, radius, palette.len())
            .into_iter()
            .zip(palette.iter().copied())
            .collect();
        let highlighted = match wheel.hovered {
            Some(Spot::Swatch(index)) => Some(index),
            _ => palette
                .iter()
                .position(|entry| *entry == Some(self.tools.color)),
        };
        Some(WheelView {
            center: wheel.center,
            radius,
            swatch_radius: SWATCH_RADIUS,
            swatches,
            highlighted,
            custom_radius: wheel.custom.then(|| custom_radius(radius)),
            color: self.tools.color,
        })
    }

    /// The document's objects to draw from the cache, and the one left out
    /// because it is drawn as a preview (dragged or recoloured).
    pub fn excluded_object(&self) -> Option<ObjectId> {
        let selected = self.tools.selected?;
        let recolouring = self
            .tools
            .wheel
            .is_some_and(|wheel| wheel.hovered.is_some())
            || self.recolouring_on_panel();
        (self.tools.moving.is_some() || recolouring).then_some(selected)
    }

    /// What is drawn over the document: the object being drawn, the text
    /// being typed, the selected object while dragged or recoloured.
    pub fn previews(&self) -> Vec<Annotation> {
        let mut previews = Vec::new();
        if let Some(id) = self.excluded_object()
            && let Some(object) = self.tools.editor.document().get(id)
        {
            let (dx, dy) = self.tools.moving.unwrap_or((0, 0));
            previews.push(Annotation {
                id: PREVIEW_ID,
                color: if self.tools.wheel.is_some() || self.recolouring_on_panel() {
                    self.tools.color
                } else {
                    object.color
                },
                size: object.size,
                shape: translated(&object.shape, dx, dy),
            });
        }
        if let Some(draft) = &self.tools.draft {
            previews.push(draft.clone());
        }
        if let Some(edit) = &self.tools.text
            && !edit.text.is_empty()
        {
            previews.push(self.text_annotation(edit));
        }
        previews
    }

    fn text_annotation(&self, edit: &TextEdit) -> Annotation {
        Annotation {
            id: PREVIEW_ID,
            color: self.tools.color,
            size: self.object_size(ToolKind::Text, self.tools.size),
            shape: Shape::Text {
                top_left: edit.top_left,
                text: edit.text.clone(),
                font: TextFont::default(),
                align: TextAlign::Left,
            },
        }
    }

    /// The outline around the selected object, in image pixels.
    pub fn selected_bounds(&self) -> Option<Rect> {
        let id = self.tools.selected?;
        let object = self.tools.editor.document().get(id)?;
        let (dx, dy) = self.tools.moving.unwrap_or((0, 0));
        let moved = Annotation {
            shape: translated(&object.shape, dx, dy),
            ..object.clone()
        };
        moved.bounding_rect(&self.tools.fonts.0)
    }

    /// The text caret: its top in image pixels and its height.
    pub fn caret(&self) -> Option<(Point, u32)> {
        let edit = self.tools.text.as_ref()?;
        let before: String = edit.text.chars().take(edit.caret).collect();
        let line_index = before.matches('\n').count();
        let line = before.rsplit('\n').next().unwrap_or_default();
        let fonts = &self.tools.fonts.0;
        let area = |text: &str| {
            let annotation = Annotation {
                shape: Shape::Text {
                    top_left: edit.top_left,
                    text: text.to_owned(),
                    font: TextFont::default(),
                    align: TextAlign::Left,
                },
                ..self.text_annotation(edit)
            };
            annotation.bounding_rect(fonts)
        };
        // A one-line box: padding on both sides of one line's height
        let single = area("X")?;
        let line_height = single.height.saturating_sub(10);
        let width = if line.is_empty() {
            0
        } else {
            area(line).map_or(0, |rect| rect.width.saturating_sub(10))
        };
        let advance = if line_index == 0 {
            0
        } else {
            let lines = "X\n".repeat(line_index) + "X";
            area(&lines).map_or(0, |rect| rect.height.saturating_sub(single.height))
                / u32::try_from(line_index).unwrap_or(1)
        };
        let top = edit.top_left.y
            + 5
            + i32::try_from(advance * u32::try_from(line_index).unwrap_or(0)).unwrap_or(0);
        let x = edit.top_left.x + 5 + i32::try_from(width).unwrap_or(0);
        Some((Point::new(x, top), line_height))
    }
}

/// The ring's radius: three per colour plus a button (`ColorPicker`).
fn wheel_radius(count: usize) -> i32 {
    3 * i32::try_from(count).unwrap_or(0) + BUTTON_SIZE
}

fn custom_radius(radius: i32) -> i32 {
    (radius - SWATCH_RADIUS - SEPARATOR).max(SWATCH_RADIUS)
}

/// The first colour on top, the rest anticlockwise (`QLineF::setAngle`).
fn swatch_centers(center: LogicalPoint, radius: i32, count: usize) -> Vec<LogicalPoint> {
    let step = 360.0 / count.max(1) as f64;
    (0..count)
        .map(|index| {
            let angle = (90.0 + step * index as f64).to_radians();
            LogicalPoint::new(
                center.x + (f64::from(radius) * angle.cos()).round() as i32,
                center.y - (f64::from(radius) * angle.sin()).round() as i32,
            )
        })
        .collect()
}

impl OverlayState {
    // --- Pointer ----------------------------------------------------------------

    /// A left press the tools take. Returns false to leave it to the
    /// selection.
    pub(crate) fn tools_press(
        &mut self,
        at: LogicalPoint,
        modifiers: Modifiers,
        effects: &mut Effects,
    ) -> bool {
        if self.tools.wheel.is_some() {
            // A click picks what the wheel points at, or closes it
            self.wheel_hover(at, effects);
            self.wheel_commit(effects);
            return true;
        }
        if let Some(index) = self.hit_toolbar(at) {
            self.drag = Drag::Button(index);
            effects.redraw(self.active);
            return true;
        }
        if self.tools.shape_menu {
            self.tools.shape_menu = false;
            effects.redraw(self.active);
        }
        let Some(selection) = self.selection else {
            return false;
        };
        if let Some(edit) = &self.tools.text {
            // A click in the box keeps typing; elsewhere it commits, and
            // starts nothing new
            let inside = self
                .text_annotation(edit)
                .bounding_rect(&self.tools.fonts.0)
                .is_some_and(|area| area.contains(self.image_point(at)));
            if !inside {
                self.commit_text(effects);
            }
            return true;
        }
        let side = super::side_at(selection, at);
        let on_handle = side.is_some_and(|side| side != super::Side::Center);
        match self.tools.active {
            Some(tool) if draws(tool) && !on_handle => {
                self.start_drawing(tool, at, modifiers, effects);
                true
            }
            Some(ButtonType::MoveSelection) if !on_handle => {
                self.drag = Drag::Moving { last: at };
                true
            }
            Some(ButtonType::MoveObject) | None if !on_handle => {
                let point = self.image_point(at);
                match self
                    .tools
                    .editor
                    .document()
                    .hit_test(point, &self.tools.fonts.0)
                {
                    Some(id) => {
                        self.tools.selected = Some(id);
                        self.drag = Drag::MovingObject {
                            start: at,
                            moved: false,
                        };
                        effects.redraw(self.active);
                        true
                    }
                    None => {
                        if self.tools.selected.take().is_some() {
                            effects.redraw(self.active);
                        }
                        false
                    }
                }
            }
            _ => false,
        }
    }

    fn start_drawing(
        &mut self,
        tool: ButtonType,
        at: LogicalPoint,
        modifiers: Modifiers,
        effects: &mut Effects,
    ) {
        let point = self.image_point(self.snapped(at));
        self.tools.selected = None;
        if tool == ButtonType::Text {
            let size = self.object_size(ToolKind::Text, self.tools.size);
            let top_left =
                text_top_left_for_anchor(point, &TextFont::default(), size, &self.tools.fonts.0)
                    .unwrap_or(point);
            self.tools.text = Some(TextEdit {
                top_left,
                text: String::new(),
                caret: 0,
            });
            effects.redraw(self.active);
            return;
        }
        let Some(shape) = self.shape_between(tool, point, point) else {
            return;
        };
        let kind = shape.kind();
        self.tools.draft = Some(Annotation {
            id: PREVIEW_ID,
            color: self.tools.color,
            size: self.object_size(kind, self.tools.size),
            shape,
        });
        self.drag = Drag::Drawing;
        // A counter shows where it lands at once
        self.draw_to(at, modifiers, effects);
    }

    /// The drawing follows the pointer; Shift keeps lines to 45 degrees and
    /// boxes square (`AbstractTwoPointTool`).
    fn draw_to(&mut self, at: LogicalPoint, modifiers: Modifiers, effects: &mut Effects) {
        let point = self.image_point(self.snapped(at));
        let Some(tool) = self.tools.active else {
            return;
        };
        let Some(draft) = self.tools.draft.as_mut() else {
            return;
        };
        match &mut draft.shape {
            Shape::Pencil { points } => {
                if points.last() != Some(&point) {
                    points.push(point);
                }
            }
            Shape::CircleCounter { pointer, .. } => *pointer = point,
            Shape::Line { start, end }
            | Shape::Arrow { start, end, .. }
            | Shape::Rectangle { start, end, .. }
            | Shape::Ellipse { start, end, .. }
            | Shape::Marker { start, end }
            | Shape::Pixelate { start, end, .. }
            | Shape::Invert { start, end } => {
                *end = if modifiers.shift {
                    constrain(*start, point, kind_of(tool).constraint())
                } else {
                    point
                };
            }
            Shape::Text { .. } => {}
        }
        effects.redraw(self.active);
    }

    /// Pointer movement with the left button held, for the tools. Returns
    /// false when the drag is not theirs.
    pub(crate) fn tools_drag(
        &mut self,
        at: LogicalPoint,
        modifiers: Modifiers,
        effects: &mut Effects,
    ) -> bool {
        match self.drag {
            Drag::Drawing => self.draw_to(at, modifiers, effects),
            Drag::MovingObject { start, moved } => {
                let moved = moved || manhattan(at, start) > super::CLICK_DISTANCE;
                self.drag = Drag::MovingObject { start, moved };
                if moved {
                    let from = self.image_point(start);
                    let to = self.image_point(at);
                    self.tools.moving = Some((to.x - from.x, to.y - from.y));
                    effects.redraw(self.active);
                }
            }
            Drag::Button(_) => self.update_tools_hover(at, effects),
            _ => return false,
        }
        true
    }

    /// The left button released during a tools drag.
    pub(crate) fn tools_release(&mut self, drag: Drag, at: LogicalPoint, effects: &mut Effects) {
        match drag {
            Drag::Drawing => {
                if let Some(draft) = self.tools.draft.take() {
                    let command = Command::Add {
                        color: draft.color,
                        size: draft.size,
                        shape: draft.shape,
                    };
                    // A click that drew nothing is no object
                    if let Ok(Some(id)) = self.tools.editor.apply(command) {
                        log::debug!("drew object {id}");
                    }
                }
                effects.redraw(self.active);
            }
            Drag::MovingObject { .. } => {
                if let (Some(id), Some((dx, dy))) = (self.tools.selected, self.tools.moving.take())
                    && (dx, dy) != (0, 0)
                    && let Err(error) = self.tools.editor.apply(Command::Move { id, dx, dy })
                {
                    log::warn!("cannot move object {id}: {error}");
                }
                effects.redraw(self.active);
            }
            Drag::Button(index) => {
                effects.redraw(self.active);
                if self.hit_toolbar(at) == Some(index) {
                    match self.tools.buttons.get(index).copied() {
                        Some(button) => self.press_button(button, effects),
                        None => self.pick_shape(index - self.tools.buttons.len(), effects),
                    }
                }
            }
            _ => {}
        }
    }

    /// The toolbar entry (or shape option, numbered after the buttons)
    /// under `at`.
    fn hit_toolbar(&self, at: LogicalPoint) -> Option<usize> {
        let options: Vec<LogicalRect> = self.shape_menu().iter().map(|o| o.rect).collect();
        if let Some(index) = button_at(&options, at) {
            return Some(self.tools.buttons.len() + index);
        }
        let rects: Vec<LogicalRect> = self.toolbar().iter().map(|b| b.rect).collect();
        button_at(&rects, at)
    }

    /// Whether the pointer is over the toolbar.
    pub(crate) fn over_toolbar(&self, at: LogicalPoint) -> bool {
        self.hit_toolbar(at).is_some()
    }

    /// Hover over the toolbar and the wheel, with no button held.
    pub(crate) fn update_tools_hover(&mut self, at: LogicalPoint, effects: &mut Effects) {
        if self.tools.wheel.is_some() {
            self.wheel_hover(at, effects);
        }
        let hover = self.hit_toolbar(at);
        if hover != self.tools.hover {
            self.tools.hover = hover;
            self.tools.hover_since = self.now;
            effects.redraw(self.active);
        }
    }

    // --- Buttons --------------------------------------------------------------------

    /// A toolbar button or its shortcut (`CaptureWidget::handleButtonLeftClick`).
    pub(crate) fn press_button(&mut self, button: ButtonType, effects: &mut Effects) {
        let has_tasks = self.config.has_tasks;
        if !offered(button, has_tasks) {
            return;
        }
        self.commit_text(effects);
        effects.redraw(self.active);
        if is_tool(button) {
            if self.tools.active == Some(button) {
                if button == ButtonType::Shape {
                    // The Shape button opens its menu instead
                    self.tools.shape_menu = !self.tools.shape_menu;
                } else {
                    self.tools.active = None;
                    self.tools.shape_menu = false;
                }
            } else {
                self.persist_size(effects);
                self.tools.active = Some(button);
                self.tools.shape_menu = false;
                self.tools.size = self.tools.settings.size_of(Some(button));
                if draws(button) {
                    self.tools.selected = None;
                }
            }
            return;
        }
        match button {
            ButtonType::Undo => {
                self.tools.editor.undo();
                self.forget_missing_selection();
            }
            ButtonType::Redo => {
                self.tools.editor.redo();
                self.forget_missing_selection();
            }
            ButtonType::SizeIncrease => self.set_size(self.tools.size.saturating_add(1), effects),
            ButtonType::SizeDecrease => self.set_size(self.tools.size.saturating_sub(1), effects),
            ButtonType::Copy => self.export(ExportAction::Copy, effects),
            ButtonType::CopyFile => self.export(ExportAction::CopyFile, effects),
            ButtonType::Save => self.export(ExportAction::Save, effects),
            ButtonType::Accept => self.export(ExportAction::Accept, effects),
            ButtonType::Exit => self.close(super::Outcome::Cancelled, effects),
            ButtonType::Pin => self.defer(DeferredAction::Pin, effects),
            ButtonType::Ocr => self.defer(DeferredAction::Ocr, effects),
            ButtonType::OpenInEditor => self.defer(DeferredAction::OpenInEditor, effects),
            ButtonType::OpenApp => self.defer(DeferredAction::OpenWith, effects),
            _ => {}
        }
    }

    fn pick_shape(&mut self, option: usize, effects: &mut Effects) {
        let Some((kind, style)) = SHAPE_OPTIONS.get(option).copied() else {
            return;
        };
        self.tools.settings.shape_kind = kind;
        self.tools.settings.shape_style = style;
        self.tools.shape_menu = false;
        effects.push(Effect::Persist(SettingChange::Shape { kind, style }));
        effects.redraw(self.active);
    }

    fn forget_missing_selection(&mut self) {
        if let Some(id) = self.tools.selected
            && self.tools.editor.document().get(id).is_none()
        {
            self.tools.selected = None;
        }
    }

    /// Delete: removes the selected object.
    pub(crate) fn delete_selected(&mut self, effects: &mut Effects) {
        if let Some(id) = self.tools.selected.take() {
            if let Err(error) = self.tools.editor.apply(Command::Delete { id }) {
                log::warn!("cannot delete object {id}: {error}");
            }
            effects.redraw(self.active);
        }
    }

    // --- Size ------------------------------------------------------------------------

    /// `CaptureWidget::setToolSize`: clamps, applies to the selected object
    /// and what is being drawn, and shows the notice.
    pub(crate) fn set_size(&mut self, size: u32, effects: &mut Effects) {
        self.apply_size(size, effects);
        self.tools.notice_until = Some(self.now + SIZE_NOTICE_MS);
    }

    /// A new tool size without the notice: the side panel's slider.
    pub(crate) fn apply_size(&mut self, size: u32, effects: &mut Effects) {
        let size = size.clamp(1, MAX_TOOL_SIZE);
        self.tools.size = size;
        if let Some(id) = self.tools.selected
            && let Some(object) = self.tools.editor.document().get(id)
        {
            let size = self.object_size(object.shape.kind(), size);
            if let Err(error) = self.tools.editor.apply(Command::SetSize { id, size }) {
                log::warn!("cannot resize object {id}: {error}");
            }
        }
        if let Some(kind) = self.tools.draft.as_ref().map(|draft| draft.shape.kind()) {
            let object_size = self.object_size(kind, size);
            if let Some(draft) = self.tools.draft.as_mut() {
                draft.size = object_size;
            }
        }
        if let Some(button) = self.tools.active
            && let Some(entry) = self
                .tools
                .settings
                .sizes
                .iter_mut()
                .find(|(tool, _)| *tool == button)
        {
            entry.1 = size;
        }
        effects.redraw(self.active);
    }

    /// The mouse wheel: one step per notch, touchpads at most every 200 ms.
    /// Ctrl with the counter tool changes the next number instead.
    pub(crate) fn wheel_turned(&mut self, delta: i32, modifiers: Modifiers, effects: &mut Effects) {
        let step = if delta.abs() >= WHEEL_NOTCH {
            delta.signum()
        } else if delta != 0
            && self
                .tools
                .touchpad_step
                .is_none_or(|last| self.now >= last + TOUCHPAD_STEP_MS)
        {
            self.tools.touchpad_step = Some(self.now);
            delta.signum()
        } else {
            0
        };
        if step == 0 {
            return;
        }
        if modifiers.ctrl && self.tools.active == Some(ButtonType::CircleCount) {
            let next = self.tools.editor.document().next_counter();
            let next = if step > 0 {
                next.saturating_add(1)
            } else {
                next.saturating_sub(1)
            };
            self.tools.editor.set_next_counter(next);
            effects.redraw(self.active);
            return;
        }
        let size = if step > 0 {
            self.tools.size.saturating_add(1)
        } else {
            self.tools.size.saturating_sub(1)
        };
        self.set_size(size, effects);
    }

    /// A digit key: sizes are typed digit by digit; a value out of range is
    /// clamped and typing starts over.
    pub(crate) fn digit(&mut self, digit: u32, effects: &mut Effects) {
        let typed = self.tools.digits.unwrap_or(0).saturating_mul(10) + digit;
        let clamped = typed.clamp(1, MAX_TOOL_SIZE);
        self.set_size(clamped, effects);
        self.tools.digits = (clamped == typed).then_some(typed);
    }

    pub(crate) fn reset_digits(&mut self) {
        self.tools.digits = None;
    }

    /// The size notice went: the size is saved (`onToolSizeSettled`).
    pub(crate) fn persist_size(&mut self, effects: &mut Effects) {
        if self.tools.notice_until.take().is_none() {
            return;
        }
        self.tools.digits = None;
        effects.redraw(self.active);
        if let Some(button) = self.tools.active {
            effects.push(Effect::Persist(SettingChange::ToolSize {
                button,
                size: self.tools.size,
            }));
        }
    }

    // --- Colour wheel -----------------------------------------------------------------

    /// A right press opens the wheel centred on it, kept on the monitor.
    pub(crate) fn open_wheel(&mut self, at: LogicalPoint, effects: &mut Effects) {
        if self.tools.text.is_some() {
            return;
        }
        let radius = wheel_radius(self.tools.settings.palette.len());
        let reach = radius + SWATCH_RADIUS + SWATCH_REACH;
        let screen = self.monitor_rect();
        let clamp = |value: i32, size: i32| {
            if size > 2 * reach {
                value.clamp(reach, size - reach)
            } else {
                size / 2
            }
        };
        self.tools.wheel = Some(Wheel {
            center: LogicalPoint::new(clamp(at.x, screen.width), clamp(at.y, screen.height)),
            hovered: None,
            original: self.tools.color,
            custom: false,
        });
        self.tools.shape_menu = false;
        effects.redraw(self.active);
    }

    fn wheel_spot(&self, at: LogicalPoint) -> Option<Spot> {
        let wheel = self.tools.wheel?;
        let palette = &self.tools.settings.palette;
        let radius = wheel_radius(palette.len());
        let reach = SWATCH_RADIUS + SWATCH_REACH;
        if let Some(index) = swatch_centers(wheel.center, radius, palette.len())
            .iter()
            .position(|center| {
                let (dx, dy) = (at.x - center.x, at.y - center.y);
                dx * dx + dy * dy <= reach * reach
            })
        {
            return Some(Spot::Swatch(index));
        }
        if wheel.custom {
            let disc = custom_radius(radius);
            let (dx, dy) = (
                f64::from(at.x - wheel.center.x),
                f64::from(at.y - wheel.center.y),
            );
            let distance = dx.hypot(dy);
            if distance <= f64::from(disc) {
                let hue = (-dy).atan2(dx).to_degrees();
                return Some(Spot::Custom(disc_color(hue, distance / f64::from(disc))));
            }
        }
        None
    }

    /// Hovering a colour applies it at once; leaving every colour keeps
    /// the last one.
    fn wheel_hover(&mut self, at: LogicalPoint, effects: &mut Effects) {
        let Some(spot) = self.wheel_spot(at) else {
            return;
        };
        let Some(wheel) = self.tools.wheel.as_mut() else {
            return;
        };
        if wheel.hovered == Some(spot) {
            return;
        }
        wheel.hovered = Some(spot);
        match spot {
            Spot::Swatch(index) => {
                if let Some(Some(color)) = self.tools.settings.palette.get(index) {
                    self.tools.color = *color;
                }
            }
            Spot::Custom(color) => self.tools.color = color,
        }
        effects.redraw(self.active);
    }

    /// The right button released over the wheel. Returns false when nothing
    /// was picked, so the release counts as a plain click.
    pub(crate) fn wheel_release(&mut self, at: LogicalPoint, effects: &mut Effects) -> bool {
        let Some(wheel) = self.tools.wheel else {
            return false;
        };
        self.wheel_hover(at, effects);
        let picker = self.wheel_spot(at).is_some_and(|spot| {
            matches!(spot, Spot::Swatch(index)
                if self.tools.settings.palette.get(index) == Some(&None))
        });
        if picker {
            // The custom colour opens its disc; the wheel stays
            if let Some(wheel) = self.tools.wheel.as_mut() {
                wheel.custom = true;
            }
            effects.redraw(self.active);
            return true;
        }
        if wheel.custom && self.wheel_spot(at).is_none() {
            // The disc is open: the release that opened it is not a pick
            return true;
        }
        let picked = self
            .tools
            .wheel
            .is_some_and(|wheel| wheel.hovered.is_some());
        self.wheel_commit(effects);
        picked
    }

    /// Keeps the colour the wheel shows, saves it and recolours the selected
    /// object (`CaptureWidget::setDrawColor`).
    fn wheel_commit(&mut self, effects: &mut Effects) {
        let Some(wheel) = self.tools.wheel.take() else {
            return;
        };
        effects.redraw(self.active);
        let color = self.tools.color;
        if color == wheel.original && wheel.hovered.is_none() {
            return;
        }
        effects.push(Effect::Persist(SettingChange::DrawColor(color)));
        if let Some(id) = self.tools.selected
            && let Err(error) = self.tools.editor.apply(Command::SetColor { id, color })
        {
            log::warn!("cannot recolour object {id}: {error}");
        }
    }

    /// Esc on the wheel: the colour from before it opened.
    pub(crate) fn wheel_cancel(&mut self, effects: &mut Effects) {
        if let Some(wheel) = self.tools.wheel.take() {
            self.tools.color = wheel.original;
            effects.redraw(self.active);
        }
    }

    // --- Keys -------------------------------------------------------------------------

    /// Esc: closes what is open, one thing per press, before it closes the
    /// overlay (`deleteToolWidgetOrClose`). Returns false when nothing was
    /// open.
    pub(crate) fn escape_tools(&mut self, effects: &mut Effects) -> bool {
        if self.tools.text.is_some() {
            self.commit_text(effects);
        } else if self.tools.wheel.is_some() {
            self.wheel_cancel(effects);
        } else if self.tools.shape_menu {
            self.tools.shape_menu = false;
        } else if self.tools.active.is_some() {
            self.tools.active = None;
        } else if self.tools.selected.is_some() {
            self.tools.selected = None;
        } else if self.side_panel_open() {
            self.close_panel(effects);
        } else {
            return false;
        }
        effects.redraw(self.active);
        true
    }

    /// Keys while text is typed (`QTextEdit`): Enter is a new line, Esc and
    /// Ctrl+Enter finish.
    pub(crate) fn text_key(&mut self, key: Key, modifiers: Modifiers, effects: &mut Effects) {
        let Some(edit) = self.tools.text.as_mut() else {
            return;
        };
        let length = edit.text.chars().count();
        let byte = |text: &str, chars: usize| {
            text.char_indices()
                .nth(chars)
                .map_or(text.len(), |(index, _)| index)
        };
        match key {
            Key::Escape => {
                self.commit_text(effects);
                return;
            }
            Key::Return | Key::Enter if modifiers.ctrl => {
                self.commit_text(effects);
                return;
            }
            Key::Return | Key::Enter => {
                let at = byte(&edit.text, edit.caret);
                edit.text.insert(at, '\n');
                edit.caret += 1;
            }
            Key::Backspace if edit.caret > 0 => {
                let at = byte(&edit.text, edit.caret - 1);
                edit.text.remove(at);
                edit.caret -= 1;
            }
            Key::Delete if edit.caret < length => {
                let at = byte(&edit.text, edit.caret);
                edit.text.remove(at);
            }
            Key::Left => edit.caret = edit.caret.saturating_sub(1),
            Key::Right => edit.caret = (edit.caret + 1).min(length),
            Key::Home => {
                let before: String = edit.text.chars().take(edit.caret).collect();
                edit.caret = before
                    .rfind('\n')
                    .map_or(0, |at| before[..at].chars().count() + 1);
            }
            Key::End => {
                let after: String = edit.text.chars().skip(edit.caret).collect();
                edit.caret += after
                    .find('\n')
                    .map_or(after.chars().count(), |at| after[..at].chars().count());
            }
            _ => return,
        }
        effects.redraw(self.active);
    }

    /// A typed character (`WM_CHAR`).
    pub(crate) fn typed(&mut self, c: char, effects: &mut Effects) {
        let Some(edit) = self.tools.text.as_mut() else {
            return;
        };
        if c.is_control() {
            return;
        }
        let at = edit
            .text
            .char_indices()
            .nth(edit.caret)
            .map_or(edit.text.len(), |(index, _)| index);
        edit.text.insert(at, c);
        edit.caret += 1;
        effects.redraw(self.active);
    }

    /// Places the typed text; empty text places nothing.
    pub(crate) fn commit_text(&mut self, effects: &mut Effects) {
        let Some(edit) = self.tools.text.take() else {
            return;
        };
        effects.redraw(self.active);
        if edit.text.is_empty() {
            return;
        }
        let annotation = self.text_annotation(&edit);
        if let Err(error) = self.tools.editor.apply(Command::Add {
            color: annotation.color,
            size: annotation.size,
            shape: annotation.shape,
        }) {
            log::warn!("cannot place text: {error}");
        }
    }

    /// Ctrl+Enter (`TYPE_COMMIT_CURRENT_TOOL`).
    pub(crate) fn commit_current(&mut self, effects: &mut Effects) {
        self.commit_text(effects);
    }

    /// Whether a fade is still running, for the next frame.
    pub(crate) fn animating(&self) -> bool {
        !self.config.reduced_motion
            && self.tools.hover.is_some()
            && self.now < self.tools.hover_since + HOVER_FADE_MS
    }

    pub(crate) fn next_frame(&self) -> Option<Millis> {
        let hover = self
            .animating()
            .then(|| (self.now + FRAME_MS).min(self.tools.hover_since + HOVER_FADE_MS));
        let panel = self.panel_animating().then_some(self.now + FRAME_MS);
        hover.into_iter().chain(panel).min()
    }

    /// The selected object takes the colour being dragged on the side
    /// panel's disc, as a preview.
    fn recolouring_on_panel(&self) -> bool {
        self.panel.held == Some(super::panel::PanelControl::Disc)
    }

    /// `snapToGrid`: with Display grid on, where a tool draws goes to the
    /// nearest grid point.
    fn snapped(&self, at: LogicalPoint) -> LogicalPoint {
        match self.grid() {
            Some(size) => {
                let size = f64::from(size);
                let snap = |v: i32| ((f64::from(v) / size).round() * size) as i32;
                LogicalPoint::new(snap(at.x), snap(at.y))
            }
            None => at,
        }
    }
}

/// The tool kind a drawing button places, for its Shift constraint.
fn kind_of(button: ButtonType) -> ToolKind {
    match button {
        ButtonType::Pencil => ToolKind::Pencil,
        ButtonType::Line => ToolKind::Line,
        ButtonType::Arrow => ToolKind::Arrow,
        ButtonType::Circle => ToolKind::Ellipse,
        ButtonType::Marker => ToolKind::Marker,
        ButtonType::Text => ToolKind::Text,
        ButtonType::CircleCount => ToolKind::CircleCounter,
        ButtonType::Pixelate => ToolKind::Pixelate,
        ButtonType::Invert => ToolKind::Invert,
        // Shape, Selection, Rectangle: a box, constrained as one
        _ => ToolKind::Rectangle,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_custom_disc_runs_from_white_to_full_hues() {
        assert_eq!(disc_color(0.0, 0.0), Color::rgb(255, 255, 255));
        assert_eq!(disc_color(0.0, 1.0), Color::rgb(255, 0, 0));
        assert_eq!(disc_color(120.0, 1.0), Color::rgb(0, 255, 0));
        assert_eq!(disc_color(240.0, 1.0), Color::rgb(0, 0, 255));
        assert_eq!(disc_color(-120.0, 1.0), Color::rgb(0, 0, 255));
    }

    #[test]
    fn swatches_start_on_top_and_go_anticlockwise() {
        let centers = swatch_centers(LogicalPoint::new(100, 100), 50, 4);
        assert_eq!(
            centers,
            [
                LogicalPoint::new(100, 50),
                LogicalPoint::new(50, 100),
                LogicalPoint::new(100, 150),
                LogicalPoint::new(150, 100),
            ]
        );
    }
}
