//! The capture toolbar's buttons: what each is called, which icon and
//! shortcut it has, which are tools and which are actions, and the tool
//! settings they read and write (15.0.0's `CaptureTool` subclasses and
//! `ConfigHandler::toolSize`).

use phramer_annotate::{ArrowHead, Color, RedactStyle, ShapeStyle};
use phramer_core::config::{self, ButtonType, ConfigError, Settings};

/// `maxToolSize` of the side panel: the largest tool size.
pub const MAX_TOOL_SIZE: u32 = 50;

/// The order the toolbar shows its buttons in
/// (`CaptureToolButton::iterableButtonTypes` of the Windows build; the
/// Imgur uploader is not built). The `buttons` setting only says which of
/// them are shown.
pub const TOOLBAR_ORDER: [ButtonType; 27] = [
    ButtonType::Pencil,
    ButtonType::Line,
    ButtonType::Arrow,
    ButtonType::Shape,
    ButtonType::Selection,
    ButtonType::Rectangle,
    ButtonType::Circle,
    ButtonType::Marker,
    ButtonType::Text,
    ButtonType::CircleCount,
    ButtonType::Pixelate,
    ButtonType::Invert,
    ButtonType::MoveObject,
    ButtonType::MoveSelection,
    ButtonType::Undo,
    ButtonType::Redo,
    ButtonType::Copy,
    ButtonType::CopyFile,
    ButtonType::Save,
    ButtonType::Exit,
    ButtonType::OpenApp,
    ButtonType::Ocr,
    ButtonType::Pin,
    ButtonType::SizeIncrease,
    ButtonType::SizeDecrease,
    ButtonType::Accept,
    ButtonType::OpenInEditor,
];

/// How a button presents itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ButtonInfo {
    /// The tool's `name()`.
    pub name: &'static str,
    /// The tooltip (`description()`).
    pub description: &'static str,
    /// The Lucide icon, a name in [`crate::ICONS`].
    pub icon: &'static str,
    /// The `[Shortcuts]` entry that triggers it.
    pub shortcut: &'static str,
}

/// The name, tooltip, icon and shortcut entry of a button.
pub fn button_info(button: ButtonType) -> ButtonInfo {
    let (name, description, icon, shortcut) = match button {
        ButtonType::Pencil => (
            "Pencil",
            "Set the Pencil as the paint tool",
            "pencil",
            "TYPE_PENCIL",
        ),
        ButtonType::Line => (
            "Line",
            "Set the Line as the paint tool",
            "slash",
            "TYPE_DRAWER",
        ),
        ButtonType::Arrow => (
            "Arrow",
            "Set the Arrow as the paint tool",
            "arrow-up-right",
            "TYPE_ARROW",
        ),
        ButtonType::Shape => (
            "Shape",
            "Draw a square or circle, hollow, filled or as a highlight",
            "shapes",
            "TYPE_SHAPE",
        ),
        ButtonType::Selection => (
            "Rectangular Selection",
            "Set Selection as the paint tool",
            "square",
            "TYPE_SELECTION",
        ),
        ButtonType::Rectangle => (
            "Rectangle",
            "Set the Rectangle as the paint tool",
            "rectangle-horizontal",
            "TYPE_RECTANGLE",
        ),
        ButtonType::Circle => (
            "Circle",
            "Set the Circle as the paint tool",
            "circle",
            "TYPE_CIRCLE",
        ),
        ButtonType::Marker => (
            "Marker",
            "Set the Marker as the paint tool",
            "highlighter",
            "TYPE_MARKER",
        ),
        ButtonType::Text => ("Text", "Add text to your capture", "type", "TYPE_TEXT"),
        ButtonType::CircleCount => (
            "Circle Counter",
            "Add an autoincrementing counter bubble",
            "hash",
            "TYPE_CIRCLECOUNT",
        ),
        ButtonType::Pixelate => (
            "Pixelate",
            "Set Pixelate as the paint tool.",
            "grid-3x3",
            "TYPE_PIXELATE",
        ),
        ButtonType::Invert => (
            "Invert",
            "Set Inverter as the paint tool",
            "contrast",
            "TYPE_INVERT",
        ),
        ButtonType::MoveObject => (
            "Move Object",
            "Select and move objects you have already drawn",
            "hand",
            "TYPE_MOVE_OBJECT",
        ),
        ButtonType::MoveSelection => (
            "Move",
            "Move the selection area",
            "move",
            "TYPE_MOVESELECTION",
        ),
        ButtonType::Undo => ("Undo", "Undo the last modification", "undo-2", "TYPE_UNDO"),
        ButtonType::Redo => ("Redo", "Redo the next modification", "redo-2", "TYPE_REDO"),
        ButtonType::Copy => ("Copy", "Copy selection to clipboard", "copy", "TYPE_COPY"),
        ButtonType::CopyFile => (
            "Copy as File",
            "Save the selection and copy the file, to paste as an attachment",
            "file-image",
            "TYPE_COPY_FILE",
        ),
        ButtonType::Save => ("Save", "Save screenshot to a file", "save", "TYPE_SAVE"),
        ButtonType::Exit => ("Exit", "Leave the capture screen", "x", "TYPE_EXIT"),
        ButtonType::OpenApp => (
            "App Launcher",
            "Choose an app to open the capture",
            "external-link",
            "TYPE_OPEN_APP",
        ),
        ButtonType::Ocr => (
            "OCR",
            "Extract text from the selection",
            "scan-text",
            "TYPE_OCR",
        ),
        ButtonType::Pin => ("Pin Tool", "Pin image on the desktop", "pin", "TYPE_PIN"),
        ButtonType::SizeIncrease => (
            "Increase Tool Size",
            "Increase the size of the other tools",
            "plus",
            "TYPE_SIZEINCREASE",
        ),
        ButtonType::SizeDecrease => (
            "Decrease Tool Size",
            "Decrease the size of the other tools",
            "minus",
            "TYPE_SIZEDECREASE",
        ),
        ButtonType::Accept => ("Accept", "Accept the capture", "check", "TYPE_ACCEPT"),
        ButtonType::OpenInEditor => (
            "Open in Editor",
            "Open this capture in the editor",
            "square-pen",
            "TYPE_OPEN_IN_EDITOR",
        ),
    };
    ButtonInfo {
        name,
        description,
        icon,
        shortcut,
    }
}

/// Whether the button is a tool that stays selected (`isSelectable()`),
/// as opposed to an action that runs once.
pub fn is_tool(button: ButtonType) -> bool {
    matches!(
        button,
        ButtonType::Pencil
            | ButtonType::Line
            | ButtonType::Arrow
            | ButtonType::Shape
            | ButtonType::Selection
            | ButtonType::Rectangle
            | ButtonType::Circle
            | ButtonType::Marker
            | ButtonType::Text
            | ButtonType::CircleCount
            | ButtonType::Pixelate
            | ButtonType::Invert
            | ButtonType::MoveObject
            | ButtonType::MoveSelection
    )
}

/// Whether the button places objects (every tool but the two movers).
pub fn draws(button: ButtonType) -> bool {
    is_tool(button) && !matches!(button, ButtonType::MoveObject | ButtonType::MoveSelection)
}

/// Buttons a capture that already says what to do with the result does not
/// offer (`CaptureWidget::initButtons`), and the one a plain capture does
/// not.
pub fn offered(button: ButtonType, has_tasks: bool) -> bool {
    if has_tasks {
        !matches!(
            button,
            ButtonType::Save
                | ButtonType::Copy
                | ButtonType::CopyFile
                | ButtonType::OpenApp
                | ButtonType::Pin
                | ButtonType::OpenInEditor
        )
    } else {
        button != ButtonType::Accept
    }
}

/// `shapeKind`: what the Shape tool draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShapeKind {
    Square,
    Circle,
}

/// The tool settings of one capture, read from the settings file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolSettings {
    /// The buttons the `buttons` setting shows, in toolbar order.
    pub buttons: Vec<ButtonType>,
    /// `drawColor`.
    pub color: Color,
    /// `userColors`: the palette; `None` is the custom colour.
    pub palette: Vec<Option<Color>>,
    /// The size each tool starts with (`ConfigHandler::toolSize`).
    pub sizes: Vec<(ButtonType, u32)>,
    /// `drawThickness`: the size with no tool selected.
    pub thickness: u32,
    /// `arrowStyle`, `reverseArrow`.
    pub arrow_head: ArrowHead,
    pub reverse_arrow: bool,
    /// `shapeKind`, `shapeStyle`.
    pub shape_kind: ShapeKind,
    pub shape_style: ShapeStyle,
    /// `rectangleFillMode`: how the Rectangle tool fills.
    pub rectangle_style: ShapeStyle,
    /// `highlightOutlineWidth`.
    pub highlight_outline: u32,
    /// `insecurePixelate`.
    pub redact: RedactStyle,
    /// `undoLimit`; 0 is unlimited.
    pub undo_limit: usize,
    /// `fontFamily`: the family new text is set in; empty for the bundled
    /// Inter.
    pub font_family: String,
}

fn unsigned(value: i32) -> u32 {
    u32::try_from(value).unwrap_or(0)
}

fn annotate_color(color: config::Color) -> Color {
    Color::rgba(color.r, color.g, color.b, color.a)
}

impl ToolSettings {
    pub fn from_settings(settings: &Settings) -> Self {
        let shown = settings.buttons();
        let buttons = TOOLBAR_ORDER
            .into_iter()
            .filter(|button| shown.contains(button))
            .collect();
        let sizes = TOOLBAR_ORDER
            .into_iter()
            .filter(|button| draws(*button) || *button == ButtonType::MoveObject)
            .map(|button| (button, unsigned(stored_size(settings, button))))
            .collect();
        Self {
            buttons,
            color: annotate_color(settings.draw_color()),
            palette: settings
                .user_colors()
                .into_iter()
                .map(|entry| entry.map(annotate_color))
                .collect(),
            sizes,
            thickness: unsigned(settings.draw_thickness()),
            arrow_head: if settings.arrow_style() == 1 {
                ArrowHead::Curved
            } else {
                ArrowHead::Default
            },
            reverse_arrow: settings.reverse_arrow(),
            shape_kind: if settings.shape_kind() == 1 {
                ShapeKind::Circle
            } else {
                ShapeKind::Square
            },
            shape_style: shape_style(settings.shape_style()),
            rectangle_style: if settings.rectangle_fill_mode() == 1 {
                ShapeStyle::Highlight
            } else {
                ShapeStyle::Filled
            },
            highlight_outline: unsigned(settings.highlight_outline_width()),
            redact: if settings.insecure_pixelate() {
                RedactStyle::Insecure
            } else {
                RedactStyle::Frosted
            },
            undo_limit: usize::try_from(settings.undo_limit()).unwrap_or(0),
            font_family: settings.font_family().trim().to_owned(),
        }
    }

    /// The size `button` starts with.
    pub fn size_of(&self, button: Option<ButtonType>) -> u32 {
        button
            .and_then(|button| {
                self.sizes
                    .iter()
                    .find(|(tool, _)| *tool == button)
                    .map(|(_, size)| *size)
            })
            .unwrap_or(self.thickness)
            .clamp(1, MAX_TOOL_SIZE)
    }
}

impl Default for ToolSettings {
    fn default() -> Self {
        Self::from_settings(&Settings::default())
    }
}

fn shape_style(value: i32) -> ShapeStyle {
    match value {
        0 => ShapeStyle::Outline,
        2 => ShapeStyle::Highlight,
        _ => ShapeStyle::Filled,
    }
}

fn shape_style_value(style: ShapeStyle) -> i32 {
    match style {
        ShapeStyle::Outline => 0,
        ShapeStyle::Filled => 1,
        ShapeStyle::Highlight => 2,
    }
}

/// `ConfigHandler::toolSize`: each tool's own key.
fn stored_size(settings: &Settings, button: ButtonType) -> i32 {
    match button {
        ButtonType::Text => settings.draw_font_size(),
        ButtonType::Rectangle | ButtonType::Shape => settings.draw_rectangle_size(),
        ButtonType::Marker => settings.draw_marker_size(),
        ButtonType::Pixelate => settings.draw_pixelate_size(),
        ButtonType::CircleCount => settings.draw_circle_counter_size(),
        ButtonType::Pencil => settings.draw_pencil_size(),
        ButtonType::Line => settings.draw_line_size(),
        ButtonType::Arrow => settings.draw_arrow_size(),
        ButtonType::Circle => settings.draw_circle_size(),
        ButtonType::Selection => settings.draw_selection_size(),
        _ => settings.draw_thickness(),
    }
}

/// A choice made on the overlay that 15.0.0 wrote back to the settings
/// file: the colour as soon as it is picked, a tool's size once the size
/// notice goes, the Shape tool's kind and style when picked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingChange {
    /// `drawColor`.
    DrawColor(Color),
    /// `ConfigHandler::setToolSize`: the key of `button`'s tool.
    ToolSize { button: ButtonType, size: u32 },
    /// `shapeKind`, `shapeStyle`.
    Shape { kind: ShapeKind, style: ShapeStyle },
}

impl SettingChange {
    /// Writes the change into `settings` (not to disk).
    pub fn apply(&self, settings: &mut Settings) -> Result<(), ConfigError> {
        match *self {
            SettingChange::DrawColor(color) => {
                settings.set_draw_color(config::Color::rgb(color.r, color.g, color.b));
                Ok(())
            }
            SettingChange::ToolSize { button, size } => {
                let size = i32::try_from(size).unwrap_or(i32::MAX);
                match button {
                    ButtonType::Text => settings.set_draw_font_size(size),
                    ButtonType::Rectangle | ButtonType::Shape => {
                        settings.set_draw_rectangle_size(size)
                    }
                    ButtonType::Marker => settings.set_draw_marker_size(size),
                    ButtonType::Pixelate => settings.set_draw_pixelate_size(size),
                    ButtonType::CircleCount => settings.set_draw_circle_counter_size(size),
                    ButtonType::Pencil => settings.set_draw_pencil_size(size),
                    ButtonType::Line => settings.set_draw_line_size(size),
                    ButtonType::Arrow => settings.set_draw_arrow_size(size),
                    ButtonType::Circle => settings.set_draw_circle_size(size),
                    ButtonType::Selection => settings.set_draw_selection_size(size),
                    _ => settings.set_draw_thickness(size),
                }
            }
            SettingChange::Shape { kind, style } => {
                settings.set_shape_kind(match kind {
                    ShapeKind::Square => 0,
                    ShapeKind::Circle => 1,
                })?;
                settings.set_shape_style(shape_style_value(style))
            }
        }
    }
}
