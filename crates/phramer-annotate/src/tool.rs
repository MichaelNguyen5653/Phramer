//! The annotation tools and the objects they place.

use serde::{Deserialize, Serialize};

use crate::arrow::arrow_geometry;
use crate::color::Color;
use crate::font::{FontLibrary, text_area};
use crate::geometry::{Constraint, Point, Rect, fill_inflation, stroke_bounds_offset};

/// Extra width a marker stroke gets on top of the tool size.
const MARKER_EXTRA_WIDTH: u32 = 14;

/// Radius of a counter bubble at tool size 0.
pub(crate) const COUNTER_BASE_RADIUS: u32 = 15;
/// Width of the contrasting ring around a counter bubble.
pub(crate) const COUNTER_RING: u32 = 2;
/// Smallest number a counter bubble can show.
pub const COUNTER_MIN: u32 = 1;
/// Largest number a counter bubble can show.
pub const COUNTER_MAX: u32 = 999;

/// Border drawn around a highlight unless the user changed it (the old
/// `highlightOutlineWidth` setting).
pub const DEFAULT_HIGHLIGHT_OUTLINE_WIDTH: u32 = 2;
/// Upper bound of the highlight border setting.
pub const MAX_HIGHLIGHT_OUTLINE_WIDTH: u32 = 20;

/// Every tool that places an object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    Pencil,
    Line,
    Arrow,
    Rectangle,
    Ellipse,
    Marker,
    Text,
    Pixelate,
    Invert,
    CircleCounter,
}

impl ToolKind {
    pub const ALL: [ToolKind; 10] = [
        ToolKind::Pencil,
        ToolKind::Line,
        ToolKind::Arrow,
        ToolKind::Rectangle,
        ToolKind::Ellipse,
        ToolKind::Marker,
        ToolKind::Text,
        ToolKind::Pixelate,
        ToolKind::Invert,
        ToolKind::CircleCounter,
    ];

    /// What the constraint key does while dragging with this tool.
    pub fn constraint(self) -> Constraint {
        match self {
            ToolKind::Line | ToolKind::Arrow | ToolKind::Marker => {
                Constraint::OrthogonalAndDiagonal
            }
            ToolKind::Rectangle | ToolKind::Ellipse => Constraint::Diagonal,
            ToolKind::Pencil
            | ToolKind::Text
            | ToolKind::Pixelate
            | ToolKind::Invert
            | ToolKind::CircleCounter => Constraint::None,
        }
    }

    /// Tools that transform the pixels under them instead of painting over
    /// them. They act on the image alone and never extend the rendering.
    pub fn reads_pixels(self) -> bool {
        match self {
            ToolKind::Pixelate | ToolKind::Invert => true,
            ToolKind::Pencil
            | ToolKind::Line
            | ToolKind::Arrow
            | ToolKind::Rectangle
            | ToolKind::Ellipse
            | ToolKind::Marker
            | ToolKind::Text
            | ToolKind::CircleCounter => false,
        }
    }

    /// The `size` a new object gets when drawn at the toolbar's tool size.
    /// A marker is a broad stroke, so it starts 14 pixels wider.
    pub fn size_for_tool_size(self, tool_size: u32) -> u32 {
        match self {
            ToolKind::Marker => tool_size.saturating_add(MARKER_EXTRA_WIDTH),
            ToolKind::Pencil
            | ToolKind::Line
            | ToolKind::Arrow
            | ToolKind::Rectangle
            | ToolKind::Ellipse
            | ToolKind::Text
            | ToolKind::Pixelate
            | ToolKind::Invert
            | ToolKind::CircleCounter => tool_size,
        }
    }
}

/// Shape of an arrow head.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArrowHead {
    /// A plain triangle.
    #[default]
    Default,
    /// A triangle with a concave back.
    Curved,
}

/// How a rectangle or ellipse is painted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShapeStyle {
    /// Stroke only, exactly where the user dragged.
    Outline,
    /// Opaque fill.
    Filled,
    /// Highlighter fill: darkens what it covers instead of hiding it.
    Highlight,
}

/// How the pixelate tool hides what it covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RedactStyle {
    /// A frosted fill built only from the pixels bordering the area. The
    /// covered pixels are never read, so nothing can be recovered from it.
    #[default]
    Frosted,
    /// The old `insecurePixelate` setting: a real blur at sizes 0 and 1, a
    /// real pixelation above. Both can be partly reversed.
    Insecure,
}

/// Horizontal alignment of text lines within their block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

/// Which face a text object is set in, and its decorations.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct TextFont {
    /// Family name as registered in the [`FontLibrary`]. Empty selects the
    /// library's default family.
    #[serde(default)]
    pub family: String,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    #[serde(default)]
    pub underline: bool,
    #[serde(default)]
    pub strikeout: bool,
}

/// Geometry and tool-specific settings of one annotation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tool", rename_all = "snake_case")]
pub enum Shape {
    /// Freehand stroke through every point.
    Pencil {
        points: Vec<Point>,
    },
    Line {
        start: Point,
        end: Point,
    },
    /// The head sits at `end`, or at `start` when `reversed`.
    Arrow {
        start: Point,
        end: Point,
        #[serde(default)]
        head: ArrowHead,
        #[serde(default)]
        reversed: bool,
    },
    /// `outline_width` is the border of the highlight style, fixed when the
    /// object is drawn; the other styles ignore it.
    Rectangle {
        start: Point,
        end: Point,
        style: ShapeStyle,
        #[serde(default = "default_outline_width")]
        outline_width: u32,
    },
    Ellipse {
        start: Point,
        end: Point,
        style: ShapeStyle,
        #[serde(default = "default_outline_width")]
        outline_width: u32,
    },
    /// Straight highlighter stroke.
    Marker {
        start: Point,
        end: Point,
    },
    /// `top_left` is the corner of the text area: the glyph block plus five
    /// pixels of padding on every side.
    Text {
        top_left: Point,
        text: String,
        #[serde(default)]
        font: TextFont,
        #[serde(default)]
        align: TextAlign,
    },
    Pixelate {
        start: Point,
        end: Point,
        #[serde(default)]
        style: RedactStyle,
    },
    Invert {
        start: Point,
        end: Point,
    },
    /// A numbered bubble at `center`. When `pointer` lies outside the bubble
    /// a wedge points at it.
    CircleCounter {
        center: Point,
        pointer: Point,
        count: u32,
    },
}

fn default_outline_width() -> u32 {
    DEFAULT_HIGHLIGHT_OUTLINE_WIDTH
}

impl Shape {
    pub fn kind(&self) -> ToolKind {
        match self {
            Shape::Pencil { .. } => ToolKind::Pencil,
            Shape::Line { .. } => ToolKind::Line,
            Shape::Arrow { .. } => ToolKind::Arrow,
            Shape::Rectangle { .. } => ToolKind::Rectangle,
            Shape::Ellipse { .. } => ToolKind::Ellipse,
            Shape::Marker { .. } => ToolKind::Marker,
            Shape::Text { .. } => ToolKind::Text,
            Shape::Pixelate { .. } => ToolKind::Pixelate,
            Shape::Invert { .. } => ToolKind::Invert,
            Shape::CircleCounter { .. } => ToolKind::CircleCounter,
        }
    }

    /// Whether the shape draws anything. A drag that never left its starting
    /// point, a single-point stroke and empty text do not, and are not kept.
    pub fn is_drawable(&self) -> bool {
        match self {
            Shape::Pencil { points } => points.len() > 1,
            Shape::Line { start, end }
            | Shape::Arrow { start, end, .. }
            | Shape::Rectangle { start, end, .. }
            | Shape::Ellipse { start, end, .. }
            | Shape::Marker { start, end }
            | Shape::Pixelate { start, end, .. }
            | Shape::Invert { start, end } => start != end,
            Shape::Text { text, .. } => !text.is_empty(),
            Shape::CircleCounter { .. } => true,
        }
    }

    /// Shifts the whole shape.
    pub(crate) fn translate(&mut self, dx: i32, dy: i32) {
        match self {
            Shape::Pencil { points } => {
                for point in points {
                    *point = point.translated(dx, dy);
                }
            }
            Shape::Line { start, end }
            | Shape::Arrow { start, end, .. }
            | Shape::Rectangle { start, end, .. }
            | Shape::Ellipse { start, end, .. }
            | Shape::Marker { start, end }
            | Shape::Pixelate { start, end, .. }
            | Shape::Invert { start, end } => {
                *start = start.translated(dx, dy);
                *end = end.translated(dx, dy);
            }
            Shape::Text { top_left, .. } => *top_left = top_left.translated(dx, dy),
            Shape::CircleCounter {
                center, pointer, ..
            } => {
                *center = center.translated(dx, dy);
                *pointer = pointer.translated(dx, dy);
            }
        }
    }
}

/// Identifies an annotation within its document. Never reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ObjectId(pub u64);

impl core::fmt::Display for ObjectId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// One placed annotation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Annotation {
    pub id: ObjectId,
    pub color: Color,
    /// Stroke thickness in pixels for strokes and outlines; corner radius
    /// and inflation for filled shapes; font size step for text; softness
    /// for pixelate; bubble size step for counters.
    pub size: u32,
    pub shape: Shape,
}

impl Annotation {
    /// The rectangle a selection outline is drawn around. `None` when the
    /// object draws nothing, or is text whose font is not registered.
    pub fn bounding_rect(&self, fonts: &FontLibrary) -> Option<Rect> {
        if !self.shape.is_drawable() {
            return None;
        }
        let rect = match &self.shape {
            Shape::Pencil { points } => {
                let offset = stroke_bounds_offset(self.size);
                let (first, rest) = points.split_first()?;
                let mut span = Span::at(*first);
                for point in rest {
                    span.include(i64::from(point.x), i64::from(point.y));
                }
                span.inflated(offset)
            }
            Shape::Line { start, end }
            | Shape::Marker { start, end }
            | Shape::Rectangle { start, end, .. }
            | Shape::Ellipse { start, end, .. } => {
                let mut span = Span::at(*start);
                span.include(i64::from(end.x), i64::from(end.y));
                span.inflated(stroke_bounds_offset(self.size))
            }
            Shape::Arrow {
                start,
                end,
                head,
                reversed,
            } => {
                let mut span = Span::at(*start);
                span.include(i64::from(end.x), i64::from(end.y));
                if let Some(geometry) = arrow_geometry(*start, *end, self.size, *head, *reversed) {
                    for point in geometry.head_points() {
                        // Truncated, as the C++ cast them.
                        span.include(point.x as i64, point.y as i64);
                    }
                }
                span.inflated(fill_inflation(self.size))
            }
            Shape::Pixelate { start, end, .. } | Shape::Invert { start, end } => {
                Rect::from_drag(*start, *end)
            }
            Shape::Text {
                top_left,
                text,
                font,
                ..
            } => text_area(*top_left, text, font, self.size, fonts)?,
            Shape::CircleCounter {
                center, pointer, ..
            } => {
                let reach = i64::from(counter_radius(self.size)) + i64::from(COUNTER_RING);
                let (cx, cy) = (i64::from(center.x), i64::from(center.y));
                let (px, py) = (i64::from(pointer.x), i64::from(pointer.y));
                Rect::from_edges(
                    (cx - reach).min(px),
                    (cy - reach).min(py),
                    (cx + reach).max(px),
                    (cy + reach).max(py),
                )
            }
        };
        Some(rect)
    }

    /// How far this object pushes the rendering past the image. `None` for
    /// tools that only transform image pixels, and for objects that draw
    /// nothing.
    pub(crate) fn export_reach(&self, fonts: &FontLibrary) -> Option<Rect> {
        if self.shape.kind().reads_pixels() {
            return None;
        }
        // A stroke is wider than its bounding rectangle allows for.
        let pad = (self.size / 2).saturating_add(2).max(2);
        self.bounding_rect(fonts)
            .map(|rect| rect.inflated(pad))
            .filter(|rect| !rect.is_empty())
    }
}

/// Radius of a counter bubble, without its ring.
pub(crate) fn counter_radius(size: u32) -> u32 {
    size.saturating_add(COUNTER_BASE_RADIUS)
}

/// Running minimum and maximum of a set of points.
struct Span {
    min_x: i64,
    min_y: i64,
    max_x: i64,
    max_y: i64,
}

impl Span {
    fn at(point: Point) -> Self {
        let (x, y) = (i64::from(point.x), i64::from(point.y));
        Self {
            min_x: x,
            min_y: y,
            max_x: x,
            max_y: y,
        }
    }

    fn include(&mut self, x: i64, y: i64) {
        self.min_x = self.min_x.min(x);
        self.min_y = self.min_y.min(y);
        self.max_x = self.max_x.max(x);
        self.max_y = self.max_y.max(y);
    }

    fn inflated(&self, by: i64) -> Rect {
        Rect::from_edges(
            self.min_x - by,
            self.min_y - by,
            self.max_x + by,
            self.max_y + by,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn annotation(size: u32, shape: Shape) -> Annotation {
        Annotation {
            id: ObjectId(1),
            color: Color::BLACK,
            size,
            shape,
        }
    }

    fn p(x: i32, y: i32) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn only_straight_strokes_and_closed_shapes_are_constrained() {
        assert_eq!(
            ToolKind::Line.constraint(),
            Constraint::OrthogonalAndDiagonal
        );
        assert_eq!(
            ToolKind::Arrow.constraint(),
            Constraint::OrthogonalAndDiagonal
        );
        assert_eq!(
            ToolKind::Marker.constraint(),
            Constraint::OrthogonalAndDiagonal
        );
        assert_eq!(ToolKind::Rectangle.constraint(), Constraint::Diagonal);
        assert_eq!(ToolKind::Ellipse.constraint(), Constraint::Diagonal);
        for kind in [
            ToolKind::Pencil,
            ToolKind::Text,
            ToolKind::Pixelate,
            ToolKind::Invert,
            ToolKind::CircleCounter,
        ] {
            assert_eq!(kind.constraint(), Constraint::None, "{kind:?}");
        }
    }

    #[test]
    fn marker_is_fourteen_pixels_wider_than_the_tool_size() {
        assert_eq!(ToolKind::Marker.size_for_tool_size(5), 19);
        assert_eq!(ToolKind::Line.size_for_tool_size(5), 5);
    }

    #[test]
    fn undrawable_shapes_have_no_bounds() {
        let fonts = FontLibrary::new();
        let dot = annotation(
            3,
            Shape::Line {
                start: p(5, 5),
                end: p(5, 5),
            },
        );
        assert_eq!(dot.bounding_rect(&fonts), None);
        let single = annotation(
            3,
            Shape::Pencil {
                points: vec![p(1, 1)],
            },
        );
        assert_eq!(single.bounding_rect(&fonts), None);
        let empty = annotation(
            3,
            Shape::Text {
                top_left: p(0, 0),
                text: String::new(),
                font: TextFont::default(),
                align: TextAlign::Left,
            },
        );
        assert_eq!(empty.bounding_rect(&fonts), None);
    }

    #[test]
    fn stroke_bounds_grow_with_thickness() {
        let fonts = FontLibrary::new();
        // Thickness 10: offset round(7.5) = 8.
        let line = annotation(
            10,
            Shape::Line {
                start: p(100, 50),
                end: p(20, 80),
            },
        );
        assert_eq!(line.bounding_rect(&fonts), Some(Rect::new(12, 42, 96, 46)));
        let pencil = annotation(
            1,
            Shape::Pencil {
                points: vec![p(5, 9), p(-3, 4), p(7, 20)],
            },
        );
        assert_eq!(pencil.bounding_rect(&fonts), Some(Rect::new(-4, 3, 12, 18)));
    }

    #[test]
    fn region_tools_are_bounded_by_the_drag_rectangle() {
        let fonts = FontLibrary::new();
        let invert = annotation(
            4,
            Shape::Invert {
                start: p(10, 10),
                end: p(19, 14),
            },
        );
        assert_eq!(invert.bounding_rect(&fonts), Some(Rect::new(10, 10, 10, 5)));
        assert_eq!(invert.export_reach(&fonts), None);
    }

    #[test]
    fn arrow_bounds_include_the_head() {
        let fonts = FontLibrary::new();
        // Thickness 3: head base at x = 76, half width 8; inflation 2.
        let arrow = annotation(
            3,
            Shape::Arrow {
                start: p(0, 0),
                end: p(100, 0),
                head: ArrowHead::Default,
                reversed: false,
            },
        );
        assert_eq!(
            arrow.bounding_rect(&fonts),
            Some(Rect::new(-2, -10, 104, 20))
        );
    }

    #[test]
    fn counter_bounds_cover_the_ring_and_the_pointer() {
        let fonts = FontLibrary::new();
        // Size 5: radius 20, ring 2.
        let lone = annotation(
            5,
            Shape::CircleCounter {
                center: p(50, 50),
                pointer: p(50, 50),
                count: 1,
            },
        );
        assert_eq!(lone.bounding_rect(&fonts), Some(Rect::new(28, 28, 44, 44)));
        let pointing = annotation(
            5,
            Shape::CircleCounter {
                center: p(50, 50),
                pointer: p(120, 40),
                count: 1,
            },
        );
        assert_eq!(
            pointing.bounding_rect(&fonts),
            Some(Rect::new(28, 28, 92, 44))
        );
    }

    #[test]
    fn export_reach_pads_by_half_the_size_plus_two() {
        let fonts = FontLibrary::new();
        let thin = annotation(
            1,
            Shape::Line {
                start: p(0, 0),
                end: p(10, 0),
            },
        );
        // Bounds (-1,-1) 12x2, pad max(2, 0 + 2) = 2.
        assert_eq!(thin.export_reach(&fonts), Some(Rect::new(-3, -3, 16, 6)));
        let thick = annotation(
            10,
            Shape::Line {
                start: p(0, 0),
                end: p(10, 0),
            },
        );
        // Bounds (-8,-8) 26x16, pad 7.
        assert_eq!(
            thick.export_reach(&fonts),
            Some(Rect::new(-15, -15, 40, 30))
        );
    }

    #[test]
    fn translate_moves_every_point() {
        let mut shape = Shape::CircleCounter {
            center: p(1, 2),
            pointer: p(3, 4),
            count: 7,
        };
        shape.translate(10, -10);
        assert_eq!(
            shape,
            Shape::CircleCounter {
                center: p(11, -8),
                pointer: p(13, -6),
                count: 7
            }
        );
        let mut path = Shape::Pencil {
            points: vec![p(0, 0), p(5, 5)],
        };
        path.translate(-2, 3);
        assert_eq!(
            path,
            Shape::Pencil {
                points: vec![p(-2, 3), p(3, 8)]
            }
        );
    }
}
