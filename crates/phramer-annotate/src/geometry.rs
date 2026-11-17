//! Integer image-space geometry and the shift-constraint rules.
//!
//! Coordinates are image pixels with (0,0) at the top-left corner of the
//! image. They may be negative or exceed the image: annotations can sit
//! beside the image on the floating canvas.

use core::f64::consts::{FRAC_PI_2, FRAC_PI_4};

use serde::{Deserialize, Serialize};

/// A point in image coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// Shifted copy. Saturates instead of overflowing: coordinates can come
    /// from a deserialised document and must never panic.
    pub(crate) fn translated(self, dx: i32, dy: i32) -> Self {
        Self {
            x: self.x.saturating_add(dx),
            y: self.y.saturating_add(dy),
        }
    }
}

/// Pixel dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

impl Size {
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }
}

/// A half-open pixel rectangle: it covers `x..x + width` and `y..y + height`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub const fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Exclusive right edge.
    pub fn right(&self) -> i64 {
        i64::from(self.x) + i64::from(self.width)
    }

    /// Exclusive bottom edge.
    pub fn bottom(&self) -> i64 {
        i64::from(self.y) + i64::from(self.height)
    }

    pub fn contains(&self, point: Point) -> bool {
        point.x >= self.x
            && point.y >= self.y
            && i64::from(point.x) < self.right()
            && i64::from(point.y) < self.bottom()
    }

    /// Rectangle between two pairs of edges, in either order. Edges outside
    /// the `i32` range are clamped to it.
    pub(crate) fn from_edges(x0: i64, y0: i64, x1: i64, y1: i64) -> Self {
        let (left, right) = (clamp_i32(x0.min(x1)), clamp_i32(x0.max(x1)));
        let (top, bottom) = (clamp_i32(y0.min(y1)), clamp_i32(y0.max(y1)));
        Self {
            x: left,
            y: top,
            width: span(left, right),
            height: span(top, bottom),
        }
    }

    /// The area a drag from `a` to `b` selects.
    ///
    /// This is `QRect(a, b)` as Qt builds it: the edges sit at `a` and at
    /// `b + 1`, so a forward drag includes both corner pixels. The old tools
    /// painted with exactly this rectangle, so it is kept rather than "fixed".
    pub(crate) fn from_drag(a: Point, b: Point) -> Self {
        Self::from_edges(
            i64::from(a.x),
            i64::from(a.y),
            i64::from(b.x) + 1,
            i64::from(b.y) + 1,
        )
    }

    /// The smallest rectangle holding both. An empty rectangle contributes
    /// nothing.
    pub fn union(&self, other: &Rect) -> Rect {
        if self.is_empty() {
            return *other;
        }
        if other.is_empty() {
            return *self;
        }
        Self::from_edges(
            i64::from(self.x.min(other.x)),
            i64::from(self.y.min(other.y)),
            self.right().max(other.right()),
            self.bottom().max(other.bottom()),
        )
    }

    /// The overlap of both, or an empty rectangle when they do not meet.
    pub fn intersection(&self, other: &Rect) -> Rect {
        let left = self.x.max(other.x);
        let top = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        if right <= i64::from(left) || bottom <= i64::from(top) {
            return Rect::default();
        }
        Self::from_edges(i64::from(left), i64::from(top), right, bottom)
    }

    /// Grown by `by` pixels on every side. An empty rectangle stays empty.
    pub fn inflated(&self, by: u32) -> Rect {
        if self.is_empty() {
            return *self;
        }
        let by = i64::from(by);
        Self::from_edges(
            i64::from(self.x) - by,
            i64::from(self.y) - by,
            self.right() + by,
            self.bottom() + by,
        )
    }
}

/// A point with fractional coordinates, for derived geometry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PointF {
    pub(crate) x: f64,
    pub(crate) y: f64,
}

impl PointF {
    pub(crate) fn along(self, direction: PointF, distance: f64) -> PointF {
        PointF {
            x: self.x + direction.x * distance,
            y: self.y + direction.y * distance,
        }
    }

    pub(crate) fn distance_to(self, other: PointF) -> f64 {
        let (dx, dy) = (other.x - self.x, other.y - self.y);
        (dx * dx + dy * dy).sqrt()
    }

    /// Nearest whole pixel coordinate, halves away from zero like `qRound`.
    pub(crate) fn rounded(self) -> PointF {
        PointF {
            x: self.x.round(),
            y: self.y.round(),
        }
    }
}

impl From<Point> for PointF {
    fn from(point: Point) -> Self {
        PointF {
            x: f64::from(point.x),
            y: f64::from(point.y),
        }
    }
}

/// A rectangle given by its four edges, for derived geometry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RectF {
    pub(crate) left: f64,
    pub(crate) top: f64,
    pub(crate) right: f64,
    pub(crate) bottom: f64,
}

impl RectF {
    pub(crate) fn width(&self) -> f64 {
        self.right - self.left
    }

    pub(crate) fn height(&self) -> f64 {
        self.bottom - self.top
    }
}

impl From<Rect> for RectF {
    fn from(rect: Rect) -> Self {
        RectF {
            left: f64::from(rect.x),
            top: f64::from(rect.y),
            right: rect.right() as f64,
            bottom: rect.bottom() as f64,
        }
    }
}

fn clamp_i32(value: i64) -> i32 {
    // Lossless after the clamp.
    value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

fn span(low: i32, high: i32) -> u32 {
    // `high >= low`, and the difference of two i32 always fits in u32.
    (i64::from(high) - i64::from(low)) as u32
}

/// Which directions a drag snaps to while the constraint key (Shift) is held.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Constraint {
    /// The tool ignores the constraint key.
    None,
    /// Snaps to the two diagonals: squares and circles.
    Diagonal,
    /// Snaps to horizontal, vertical and both diagonals: straight strokes.
    OrthogonalAndDiagonal,
}

/// Where a drag from `start` ends when the cursor is at `cursor` and the
/// constraint key is held.
pub fn constrain(start: Point, cursor: Point, constraint: Constraint) -> Point {
    let vx = i64::from(cursor.x) - i64::from(start.x);
    let vy = i64::from(cursor.y) - i64::from(start.y);
    // Screen y grows downwards; the angle is measured with y up. The sign is
    // flipped on the integer so a horizontal drag yields +0.0, never -0.0:
    // atan2(-0.0, negative) is -pi and would land in a different sector.
    let angle = libm::atan2((-vy) as f64, vx as f64);
    let (dx, dy) = match constraint {
        Constraint::None => (vx, vy),
        Constraint::OrthogonalAndDiagonal => {
            let sector = ((angle / FRAC_PI_4).round() as i64 + 4) % 4;
            match sector {
                0 => (vx, 0),
                2 => (0, vy),
                1 => rising_diagonal(vx, vy),
                _ => falling_diagonal(vx, vy),
            }
        }
        Constraint::Diagonal => {
            // `%` truncates like the C++ it mirrors, so the one negative
            // case (-1) falls through to the falling diagonal, as it did.
            let sector = (((angle - FRAC_PI_4) / FRAC_PI_2).round() as i64 + 2) % 2;
            if sector == 0 {
                rising_diagonal(vx, vy)
            } else {
                falling_diagonal(vx, vy)
            }
        }
    };
    Point {
        x: clamp_i32(i64::from(start.x) + dx),
        y: clamp_i32(i64::from(start.y) + dy),
    }
}

/// Projection onto the "/" diagonal (up-right or down-left on screen).
fn rising_diagonal(vx: i64, vy: i64) -> (i64, i64) {
    let x = (vx - vy) / 2;
    (x, -x)
}

/// Projection onto the "\" diagonal (down-right or up-left on screen).
fn falling_diagonal(vx: i64, vy: i64) -> (i64, i64) {
    let x = (vx + vy) / 2;
    (x, x)
}

/// How far a stroke of `thickness` reaches past its geometry when computing
/// the bounding rectangle used for selection outlines.
pub(crate) fn stroke_bounds_offset(thickness: u32) -> i64 {
    if thickness <= 1 {
        1
    } else {
        (f64::from(thickness) * 0.7 + 0.5).round() as i64
    }
}

/// How far a filled or highlighted shape is inflated past the dragged
/// rectangle. The C++ wrote this as `round(size / 2 + 0.5)` with an integer
/// division, which is `size / 2 + 1` for every size above 1.
pub(crate) fn fill_inflation(size: u32) -> i64 {
    if size <= 1 {
        1
    } else {
        i64::from(size / 2) + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: i32, y: i32) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn drag_rect_includes_both_corners_on_a_forward_drag() {
        assert_eq!(
            Rect::from_drag(p(10, 20), p(14, 22)),
            Rect::new(10, 20, 5, 3)
        );
    }

    #[test]
    fn drag_rect_keeps_qt_edges_on_a_reversed_drag() {
        // Edges at a and b + 1: 14 and 11, 22 and 21.
        assert_eq!(
            Rect::from_drag(p(14, 22), p(10, 20)),
            Rect::new(11, 21, 3, 1)
        );
    }

    #[test]
    fn union_ignores_empty_rectangles() {
        let a = Rect::new(0, 0, 10, 10);
        assert_eq!(a.union(&Rect::default()), a);
        assert_eq!(Rect::default().union(&a), a);
        assert_eq!(a.union(&Rect::new(-5, 8, 3, 4)), Rect::new(-5, 0, 15, 12));
    }

    #[test]
    fn intersection_of_disjoint_rectangles_is_empty() {
        let a = Rect::new(0, 0, 10, 10);
        assert!(a.intersection(&Rect::new(10, 0, 5, 5)).is_empty());
        assert_eq!(
            a.intersection(&Rect::new(5, -5, 20, 8)),
            Rect::new(5, 0, 5, 3)
        );
    }

    #[test]
    fn extreme_coordinates_do_not_overflow() {
        let far = Rect::from_drag(p(i32::MIN, i32::MIN), p(i32::MAX - 1, i32::MAX - 1));
        assert_eq!(far.width, u32::MAX);
        assert_eq!(far.inflated(10).x, i32::MIN);
        assert_eq!(p(i32::MAX, 0).translated(5, -5), p(i32::MAX, -5));
        assert_eq!(
            constrain(
                p(i32::MIN, 0),
                p(i32::MAX, 0),
                Constraint::OrthogonalAndDiagonal
            ),
            p(i32::MAX, 0)
        );
    }

    #[test]
    fn unconstrained_tools_follow_the_cursor() {
        assert_eq!(constrain(p(5, 5), p(40, 17), Constraint::None), p(40, 17));
    }

    #[test]
    fn straight_strokes_snap_to_eight_directions() {
        let c = Constraint::OrthogonalAndDiagonal;
        let o = p(100, 100);
        // Near-horizontal, both ways.
        assert_eq!(constrain(o, p(150, 110), c), p(150, 100));
        assert_eq!(constrain(o, p(50, 95), c), p(50, 100));
        // Exactly horizontal to the left must stay horizontal (the -0.0 trap).
        assert_eq!(constrain(o, p(40, 100), c), p(40, 100));
        // Near-vertical, both ways.
        assert_eq!(constrain(o, p(104, 30), c), p(100, 30));
        assert_eq!(constrain(o, p(96, 170), c), p(100, 170));
        // Up-right: the rising diagonal, x = (vx - vy) / 2.
        assert_eq!(constrain(o, p(140, 70), c), p(135, 65));
        // Down-left: the rising diagonal again.
        assert_eq!(constrain(o, p(60, 130), c), p(65, 135));
        // Down-right and up-left: the falling diagonal, x = (vx + vy) / 2.
        assert_eq!(constrain(o, p(140, 130), c), p(135, 135));
        assert_eq!(constrain(o, p(60, 70), c), p(65, 65));
    }

    #[test]
    fn closed_shapes_snap_to_squares() {
        let c = Constraint::Diagonal;
        let o = p(0, 0);
        assert_eq!(constrain(o, p(10, 3), c), p(6, 6));
        assert_eq!(constrain(o, p(10, -3), c), p(6, -6));
        assert_eq!(constrain(o, p(-10, 3), c), p(-6, 6));
        assert_eq!(constrain(o, p(-10, -3), c), p(-6, -6));
        // A nearly flat drag still gives a square, never a line.
        assert_eq!(constrain(o, p(40, 1), c), p(20, 20));
        for cursor in [p(37, 12), p(-8, 90), p(-50, -3), p(4, -77)] {
            let end = constrain(o, cursor, c);
            assert_eq!(end.x.abs(), end.y.abs(), "{cursor:?} -> {end:?}");
        }
    }

    #[test]
    fn bounds_offsets_follow_the_old_formulas() {
        assert_eq!(stroke_bounds_offset(0), 1);
        assert_eq!(stroke_bounds_offset(1), 1);
        assert_eq!(stroke_bounds_offset(2), 2); // round(1.9)
        assert_eq!(stroke_bounds_offset(5), 4); // round(4.0)
        assert_eq!(stroke_bounds_offset(10), 8); // round(7.5), half away from zero
        assert_eq!(fill_inflation(0), 1);
        assert_eq!(fill_inflation(1), 1);
        assert_eq!(fill_inflation(2), 2);
        assert_eq!(fill_inflation(3), 2);
        assert_eq!(fill_inflation(8), 5);
    }
}
