//! Outlines derived from a shape's points, shared by the renderer and by
//! hit-testing so the area that can be grabbed is the area that is painted.

use crate::geometry::{Point, PointF, Rect, RectF, fill_inflation};

/// The rectangle an outline style strokes: exactly the dragged area.
pub(crate) fn dragged_area(start: Point, end: Point) -> RectF {
    RectF::from(Rect::from_drag(start, end))
}

/// The rectangle a filled or highlighted shape covers.
///
/// The fill is inflated past the drag by an amount derived from the size, so
/// that a fill covers what an outline of the same size would. With
/// `plain_at_zero`, size 0 falls back to the dragged area itself.
pub(crate) fn filled_area(start: Point, end: Point, size: u32, plain_at_zero: bool) -> RectF {
    if size == 0 && plain_at_zero {
        return dragged_area(start, end);
    }
    let by = fill_inflation(size) as f64;
    RectF {
        left: f64::from(start.x.min(end.x)) - by,
        top: f64::from(start.y.min(end.y)) - by,
        right: f64::from(start.x.max(end.x)) + by,
        bottom: f64::from(start.y.max(end.y)) + by,
    }
}

/// The wedge joining a counter bubble to the point it refers to: `center`,
/// one side of the bubble, `pointer`, the other side. `None` while the
/// pointer is still inside the bubble.
pub(crate) fn counter_wedge(center: Point, pointer: Point, radius: u32) -> Option<[PointF; 4]> {
    let (c, p) = (PointF::from(center), PointF::from(pointer));
    let length = c.distance_to(p);
    let radius = f64::from(radius);
    if length <= radius {
        return None;
    }
    let normal = PointF {
        x: (p.y - c.y) / length,
        y: -(p.x - c.x) / length,
    };
    // The side points land on whole pixels, mirrored through the centre.
    let side = c.along(normal, radius).rounded();
    let mirrored = PointF {
        x: 2.0 * c.x - side.x,
        y: 2.0 * c.y - side.y,
    };
    Some([c, side, p, mirrored])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: i32, y: i32) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn outline_follows_the_drag_and_fill_is_inflated_by_the_size() {
        let (a, b) = (p(10, 10), p(50, 30));
        let edges = |r: RectF| (r.left, r.top, r.right, r.bottom);
        assert_eq!(edges(dragged_area(a, b)), (10.0, 10.0, 51.0, 31.0));
        // Size 8 inflates by 8 / 2 + 1 = 5 on every side.
        assert_eq!(edges(filled_area(a, b, 8, true)), (5.0, 5.0, 55.0, 35.0));
        assert_eq!(edges(filled_area(b, a, 8, true)), (5.0, 5.0, 55.0, 35.0));
        // Size 0: the dragged area, or one pixel of inflation.
        assert_eq!(edges(filled_area(a, b, 0, true)), (10.0, 10.0, 51.0, 31.0));
        assert_eq!(edges(filled_area(a, b, 0, false)), (9.0, 9.0, 51.0, 31.0));
    }

    #[test]
    fn counter_has_no_wedge_until_the_pointer_leaves_the_bubble() {
        assert!(counter_wedge(p(0, 0), p(0, 0), 20).is_none());
        assert!(counter_wedge(p(0, 0), p(20, 0), 20).is_none());
        let wedge = counter_wedge(p(0, 0), p(60, 0), 20).unwrap();
        assert_eq!(wedge[1], PointF { x: 0.0, y: -20.0 });
        assert_eq!(wedge[2], PointF { x: 60.0, y: 0.0 });
        assert_eq!(wedge[3], PointF { x: 0.0, y: 20.0 });
    }
}
