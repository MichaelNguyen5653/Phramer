//! Picking an existing object at a point.
//!
//! The old editor painted each object into a scratch image and probed the
//! pixels around the click: first within 3 pixels, then within 5, topmost
//! object first. Text got 5 pixels more on the first pass and sat out the
//! second. The same radii are applied here to each object's painted area,
//! measured geometrically instead of by rasterising.

use crate::arrow::{HeadSegment, arrow_geometry};
use crate::font::{FontLibrary, text_area};
use crate::geometry::{Point, PointF, Rect, RectF};
use crate::outline::{counter_wedge, dragged_area, filled_area};
use crate::tool::{
    Annotation, COUNTER_RING, ObjectId, Shape, ShapeStyle, ToolKind, counter_radius,
};

const NEAR_RADIUS: f64 = 3.0;
const FAR_RADIUS: f64 = 5.0;
const TEXT_HANDICAP: f64 = 5.0;

/// The topmost object painted at or near `point`.
pub(crate) fn find(objects: &[Annotation], point: Point, fonts: &FontLibrary) -> Option<ObjectId> {
    // The middle of the clicked pixel.
    let probe = PointF {
        x: f64::from(point.x) + 0.5,
        y: f64::from(point.y) + 0.5,
    };
    for (radius, far_pass) in [(NEAR_RADIUS, false), (FAR_RADIUS, true)] {
        for object in objects.iter().rev() {
            let reach = if object.shape.kind() == ToolKind::Text {
                // Text is grabbed by its whole box, which already has a
                // generous margin; widening it again would steal clicks.
                if far_pass {
                    continue;
                }
                radius + TEXT_HANDICAP
            } else {
                radius
            };
            if let Some(distance) = ink_distance(object, probe, fonts)
                && distance <= reach
            {
                return Some(object.id);
            }
        }
    }
    None
}

/// Distance from `p` to the nearest painted part of `object`; zero on it.
fn ink_distance(object: &Annotation, p: PointF, fonts: &FontLibrary) -> Option<f64> {
    if !object.shape.is_drawable() {
        return None;
    }
    let half_stroke = f64::from(object.size.max(1)) / 2.0;
    let distance = match &object.shape {
        Shape::Pencil { points } => {
            let nearest = points
                .windows(2)
                .map(|pair| segment_distance(p, pair[0].into(), pair[1].into()))
                .fold(f64::INFINITY, f64::min);
            nearest - half_stroke
        }
        Shape::Line { start, end } | Shape::Marker { start, end } => {
            segment_distance(p, (*start).into(), (*end).into()) - half_stroke
        }
        Shape::Arrow {
            start,
            end,
            head,
            reversed,
        } => {
            let geometry = arrow_geometry(*start, *end, object.size, *head, *reversed)?;
            let shaft = segment_distance(p, geometry.shaft_start, geometry.shaft_end) - half_stroke;
            let outline: Vec<PointF> = core::iter::once(geometry.tip)
                .chain(geometry.head.iter().map(|segment| match *segment {
                    HeadSegment::Line(to) | HeadSegment::Quad { to, .. } => to,
                }))
                .collect();
            shaft.min(polygon_distance(p, &outline))
        }
        Shape::Rectangle {
            start, end, style, ..
        } => match style {
            ShapeStyle::Outline => {
                rect_border_distance(p, dragged_area(*start, *end)) - half_stroke
            }
            ShapeStyle::Filled => rect_distance(p, filled_area(*start, *end, object.size, true)),
            ShapeStyle::Highlight => {
                rect_distance(p, filled_area(*start, *end, object.size, false))
            }
        },
        Shape::Ellipse {
            start, end, style, ..
        } => match style {
            ShapeStyle::Outline => {
                ellipse_distance(p, dragged_area(*start, *end), true) - half_stroke
            }
            ShapeStyle::Filled => {
                ellipse_distance(p, filled_area(*start, *end, object.size, true), false)
            }
            ShapeStyle::Highlight => {
                ellipse_distance(p, filled_area(*start, *end, object.size, false), false)
            }
        },
        Shape::Text {
            top_left,
            text,
            font,
            ..
        } => rect_distance(
            p,
            text_area(*top_left, text, font, object.size, fonts)?.into(),
        ),
        Shape::Pixelate { start, end, .. } | Shape::Invert { start, end } => {
            rect_distance(p, Rect::from_drag(*start, *end).into())
        }
        Shape::CircleCounter {
            center, pointer, ..
        } => {
            let radius = counter_radius(object.size);
            let bubble =
                p.distance_to((*center).into()) - f64::from(radius) - f64::from(COUNTER_RING);
            match counter_wedge(*center, *pointer, radius) {
                Some(wedge) => bubble.min(polygon_distance(p, &wedge)),
                None => bubble,
            }
        }
    };
    Some(distance.max(0.0))
}

fn segment_distance(p: PointF, a: PointF, b: PointF) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let length_squared = dx * dx + dy * dy;
    if length_squared == 0.0 {
        return p.distance_to(a);
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / length_squared).clamp(0.0, 1.0);
    p.distance_to(PointF {
        x: a.x + dx * t,
        y: a.y + dy * t,
    })
}

/// Zero inside the closed polygon, otherwise the distance to its edge.
fn polygon_distance(p: PointF, polygon: &[PointF]) -> f64 {
    let mut inside = false;
    let mut nearest = f64::INFINITY;
    for (index, &a) in polygon.iter().enumerate() {
        let b = polygon[(index + 1) % polygon.len()];
        nearest = nearest.min(segment_distance(p, a, b));
        // Even-odd rule: count edges crossed by a ray towards +x.
        if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x) {
            inside = !inside;
        }
    }
    if inside { 0.0 } else { nearest }
}

/// Zero inside the rectangle, otherwise the distance to it.
fn rect_distance(p: PointF, rect: RectF) -> f64 {
    let dx = (rect.left - p.x).max(p.x - rect.right).max(0.0);
    let dy = (rect.top - p.y).max(p.y - rect.bottom).max(0.0);
    (dx * dx + dy * dy).sqrt()
}

/// Distance to the rectangle's border, from either side.
fn rect_border_distance(p: PointF, rect: RectF) -> f64 {
    let outside = rect_distance(p, rect);
    if outside > 0.0 {
        return outside;
    }
    (p.x - rect.left)
        .min(rect.right - p.x)
        .min(p.y - rect.top)
        .min(rect.bottom - p.y)
}

/// Distance to the ellipse inscribed in `bounds`, measured along the ray
/// from its centre: exact for circles and close enough for a pick radius.
/// With `border_only` the inside counts as distance too.
fn ellipse_distance(p: PointF, bounds: RectF, border_only: bool) -> f64 {
    let (rx, ry) = (bounds.width() / 2.0, bounds.height() / 2.0);
    if rx <= 0.0 || ry <= 0.0 {
        // Collapsed to a line: nothing but its border is left.
        return rect_distance(p, bounds);
    }
    let centre = PointF {
        x: bounds.left + rx,
        y: bounds.top + ry,
    };
    let (dx, dy) = (p.x - centre.x, p.y - centre.y);
    let k = ((dx / rx) * (dx / rx) + (dy / ry) * (dy / ry)).sqrt();
    if k == 0.0 {
        return if border_only { rx.min(ry) } else { 0.0 };
    }
    let from_centre = (dx * dx + dy * dy).sqrt();
    let signed = from_centre * (1.0 - 1.0 / k);
    if border_only {
        signed.abs()
    } else {
        signed.max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;
    use crate::tool::ArrowHead;

    fn p(x: i32, y: i32) -> Point {
        Point::new(x, y)
    }

    fn object(id: u64, size: u32, shape: Shape) -> Annotation {
        Annotation {
            id: ObjectId(id),
            color: Color::BLACK,
            size,
            shape,
        }
    }

    fn hit(objects: &[Annotation], x: i32, y: i32) -> Option<u64> {
        find(objects, p(x, y), &FontLibrary::new()).map(|id| id.0)
    }

    #[test]
    fn a_thin_line_is_picked_within_five_pixels() {
        let objects = [object(
            1,
            1,
            Shape::Line {
                start: p(10, 50),
                end: p(110, 50),
            },
        )];
        assert_eq!(hit(&objects, 60, 50), Some(1));
        assert_eq!(hit(&objects, 60, 46), Some(1));
        assert_eq!(hit(&objects, 60, 55), Some(1));
        assert_eq!(hit(&objects, 60, 57), None);
        // Past the end of the line.
        assert_eq!(hit(&objects, 118, 50), None);
    }

    #[test]
    fn a_thick_stroke_is_picked_across_its_width() {
        let objects = [object(
            1,
            20,
            Shape::Marker {
                start: p(10, 50),
                end: p(110, 50),
            },
        )];
        assert_eq!(hit(&objects, 60, 41), Some(1));
        assert_eq!(hit(&objects, 60, 64), Some(1));
        assert_eq!(hit(&objects, 60, 67), None);
    }

    #[test]
    fn an_outline_is_hollow_and_a_fill_is_not() {
        let outline = [object(
            1,
            2,
            Shape::Rectangle {
                start: p(10, 10),
                end: p(110, 90),
                style: ShapeStyle::Outline,
                outline_width: 2,
            },
        )];
        assert_eq!(hit(&outline, 10, 50), Some(1));
        assert_eq!(hit(&outline, 60, 90), Some(1));
        assert_eq!(hit(&outline, 60, 50), None);
        let filled = [object(
            1,
            2,
            Shape::Rectangle {
                start: p(10, 10),
                end: p(110, 90),
                style: ShapeStyle::Filled,
                outline_width: 2,
            },
        )];
        assert_eq!(hit(&filled, 60, 50), Some(1));
        assert_eq!(hit(&filled, 130, 50), None);
    }

    #[test]
    fn an_ellipse_outline_is_picked_on_its_rim_only() {
        let objects = [object(
            1,
            3,
            Shape::Ellipse {
                start: p(0, 0),
                end: p(99, 59),
                style: ShapeStyle::Outline,
                outline_width: 2,
            },
        )];
        assert_eq!(hit(&objects, 50, 0), Some(1));
        assert_eq!(hit(&objects, 0, 30), Some(1));
        assert_eq!(hit(&objects, 50, 30), None);
        // The corner of the bounding box is well outside the ellipse.
        assert_eq!(hit(&objects, 2, 2), None);
    }

    #[test]
    fn the_topmost_object_wins() {
        let below = object(
            1,
            4,
            Shape::Line {
                start: p(0, 50),
                end: p(100, 50),
            },
        );
        let above = object(
            2,
            4,
            Shape::Line {
                start: p(50, 0),
                end: p(50, 100),
            },
        );
        assert_eq!(hit(&[below.clone(), above.clone()], 50, 50), Some(2));
        assert_eq!(hit(&[above, below], 50, 50), Some(1));
    }

    #[test]
    fn an_exact_hit_beats_a_near_miss_on_a_higher_object() {
        // The click is 4 px from the upper line (far pass only) and right on
        // the lower one (near pass), so the lower one is chosen.
        let lower = object(
            1,
            1,
            Shape::Line {
                start: p(0, 50),
                end: p(100, 50),
            },
        );
        let upper = object(
            2,
            1,
            Shape::Line {
                start: p(0, 55),
                end: p(100, 55),
            },
        );
        assert_eq!(hit(&[lower, upper], 50, 50), Some(1));
    }

    #[test]
    fn region_tools_are_picked_anywhere_inside() {
        let objects = [object(
            1,
            3,
            Shape::Pixelate {
                start: p(20, 20),
                end: p(80, 60),
                style: crate::tool::RedactStyle::Frosted,
            },
        )];
        assert_eq!(hit(&objects, 50, 40), Some(1));
        assert_eq!(hit(&objects, 84, 40), Some(1));
        assert_eq!(hit(&objects, 90, 40), None);
    }

    #[test]
    fn an_arrow_is_picked_by_shaft_and_head() {
        let objects = [object(
            1,
            3,
            Shape::Arrow {
                start: p(0, 50),
                end: p(100, 50),
                head: ArrowHead::Default,
                reversed: false,
            },
        )];
        assert_eq!(hit(&objects, 30, 50), Some(1));
        // Inside the head, off the shaft: the head is 16 px wide at x = 76.
        assert_eq!(hit(&objects, 78, 56), Some(1));
        assert_eq!(hit(&objects, 30, 62), None);
    }

    #[test]
    fn a_counter_is_picked_by_bubble_and_wedge() {
        let objects = [object(
            1,
            5,
            Shape::CircleCounter {
                center: p(50, 50),
                pointer: p(150, 50),
                count: 1,
            },
        )];
        assert_eq!(hit(&objects, 40, 40), Some(1));
        assert_eq!(hit(&objects, 120, 50), Some(1));
        assert_eq!(hit(&objects, 120, 80), None);
        assert_eq!(hit(&objects, 10, 10), None);
    }

    #[test]
    fn undrawable_objects_cannot_be_picked() {
        let objects = [object(
            1,
            5,
            Shape::Line {
                start: p(5, 5),
                end: p(5, 5),
            },
        )];
        assert_eq!(hit(&objects, 5, 5), None);
    }
}
