//! Arrow shaft and head geometry.
//!
//! The head grows with the stroke: it is `10 + 2 * thickness` wide and
//! `18 + 2 * thickness` long, and the shaft stops where the head begins so
//! the tip stays sharp. Everything here is plain `f64` arithmetic plus
//! `sqrt`, which gives the same result on every target.

use crate::geometry::{Point, PointF};
use crate::tool::ArrowHead;

const HEAD_WIDTH: f64 = 10.0;
const HEAD_HEIGHT: f64 = 18.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum HeadSegment {
    Line(PointF),
    Quad { control: PointF, to: PointF },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ArrowGeometry {
    pub(crate) shaft_start: PointF,
    pub(crate) shaft_end: PointF,
    /// The curved head has a concave back, so its shaft ends flat inside the
    /// head; the default shaft keeps the square cap of an ordinary stroke.
    pub(crate) flat_cap: bool,
    /// Where the head outline starts and closes.
    pub(crate) tip: PointF,
    pub(crate) head: Vec<HeadSegment>,
}

impl ArrowGeometry {
    /// Every point that defines the head outline, control points included.
    pub(crate) fn head_points(&self) -> impl Iterator<Item = PointF> + '_ {
        core::iter::once(self.tip).chain(self.head.iter().flat_map(|segment| {
            let (control, to) = match *segment {
                HeadSegment::Line(to) => (None, to),
                HeadSegment::Quad { control, to } => (Some(control), to),
            };
            control.into_iter().chain(core::iter::once(to))
        }))
    }
}

/// Geometry of an arrow dragged from `start` to `end`. The head sits at
/// `end`, or at `start` when `reversed`. `None` for a zero-length drag.
pub(crate) fn arrow_geometry(
    start: Point,
    end: Point,
    thickness: u32,
    head: ArrowHead,
    reversed: bool,
) -> Option<ArrowGeometry> {
    let (tail, tip) = if reversed { (end, start) } else { (start, end) };
    let (tail, tip) = (PointF::from(tail), PointF::from(tip));
    let length = tail.distance_to(tip);
    if length == 0.0 {
        return None;
    }
    let direction = PointF {
        x: (tip.x - tail.x) / length,
        y: (tip.y - tail.y) / length,
    };
    let t = f64::from(thickness);
    let head_length = HEAD_HEIGHT + t * 2.0;
    let half_width = (HEAD_WIDTH + t * 2.0) / 2.0;
    let short = length < head_length;

    // Where the shaft would stop so it does not poke through the tip. On an
    // arrow shorter than its head only a stub is left, which keeps the angle.
    let shaft_stop = if short {
        tail.along(direction, f64::from(thickness / 4))
    } else {
        tail.along(direction, length - head_length)
    }
    .rounded();

    match head {
        ArrowHead::Default => {
            // The head keeps its full length unless the arrow is shorter
            // than that; then it shrinks to the arrow, in whole pixels.
            let base_distance = if short {
                length.fract()
            } else {
                length - head_length
            };
            let base = tail.along(direction, base_distance);
            let across = PointF {
                x: direction.y,
                y: -direction.x,
            };
            Some(ArrowGeometry {
                shaft_start: tail,
                shaft_end: shaft_stop,
                flat_cap: false,
                tip,
                head: vec![
                    HeadSegment::Line(base.along(across, -half_width)),
                    HeadSegment::Line(base.along(across, half_width)),
                ],
            })
        }
        ArrowHead::Curved => {
            let normal = PointF {
                x: -direction.y,
                y: direction.x,
            };
            let notch_depth = (shaft_stop.distance_to(tip) * 0.45).min(half_width);
            let notch = shaft_stop.along(direction, notch_depth);
            // One pixel of overlap hides the seam between shaft and head.
            let shaft_end = shaft_stop.along(direction, notch_depth + 1.0);
            Some(ArrowGeometry {
                shaft_start: tail,
                shaft_end,
                flat_cap: true,
                tip,
                head: vec![
                    HeadSegment::Line(shaft_stop.along(normal, half_width)),
                    HeadSegment::Quad {
                        control: shaft_stop.along(normal, half_width * 0.25),
                        to: notch,
                    },
                    HeadSegment::Quad {
                        control: shaft_stop.along(normal, -half_width * 0.25),
                        to: shaft_stop.along(normal, -half_width),
                    },
                ],
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pf(x: f64, y: f64) -> PointF {
        PointF { x, y }
    }

    fn close(a: PointF, b: PointF) -> bool {
        (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9
    }

    #[test]
    fn zero_length_arrow_has_no_geometry() {
        let at = Point::new(4, 4);
        assert!(arrow_geometry(at, at, 3, ArrowHead::Default, false).is_none());
    }

    #[test]
    fn default_head_is_sized_from_the_thickness() {
        // Horizontal, 100 long, thickness 3: head 24 long and 16 wide.
        let g = arrow_geometry(
            Point::new(0, 0),
            Point::new(100, 0),
            3,
            ArrowHead::Default,
            false,
        )
        .unwrap();
        assert!(close(g.tip, pf(100.0, 0.0)));
        assert!(close(g.shaft_start, pf(0.0, 0.0)));
        assert!(close(g.shaft_end, pf(76.0, 0.0)));
        assert!(!g.flat_cap);
        assert_eq!(g.head.len(), 2);
        let HeadSegment::Line(a) = g.head[0] else {
            panic!("line expected")
        };
        let HeadSegment::Line(b) = g.head[1] else {
            panic!("line expected")
        };
        assert!(close(a, pf(76.0, 8.0)), "{a:?}");
        assert!(close(b, pf(76.0, -8.0)), "{b:?}");
    }

    #[test]
    fn head_follows_the_drag_end_unless_reversed() {
        let (a, b) = (Point::new(10, 10), Point::new(10, 110));
        let forward = arrow_geometry(a, b, 2, ArrowHead::Default, false).unwrap();
        let reversed = arrow_geometry(a, b, 2, ArrowHead::Default, true).unwrap();
        assert!(close(forward.tip, pf(10.0, 110.0)));
        assert!(close(reversed.tip, pf(10.0, 10.0)));
        assert!(close(reversed.shaft_start, pf(10.0, 110.0)));
    }

    #[test]
    fn short_arrow_shrinks_the_head_and_keeps_a_stub() {
        // 12 long with thickness 8: the head would be 34 long.
        let g = arrow_geometry(
            Point::new(0, 0),
            Point::new(12, 0),
            8,
            ArrowHead::Default,
            false,
        )
        .unwrap();
        // Stub of thickness / 4 = 2 pixels.
        assert!(close(g.shaft_end, pf(2.0, 0.0)));
        // Head base at the tail (the fractional part of the length is zero),
        // full width 10 + 2 * 8 = 26.
        let HeadSegment::Line(a) = g.head[0] else {
            panic!("line expected")
        };
        assert!(close(a, pf(0.0, 13.0)), "{a:?}");
    }

    #[test]
    fn curved_head_has_a_notch_and_a_flat_overlapping_shaft() {
        let g = arrow_geometry(
            Point::new(0, 0),
            Point::new(100, 0),
            3,
            ArrowHead::Curved,
            false,
        )
        .unwrap();
        assert!(g.flat_cap);
        // Base centre at 76; notch depth min(24 * 0.45, 8) = 8.
        let HeadSegment::Line(left) = g.head[0] else {
            panic!("line expected")
        };
        let HeadSegment::Quad { control, to: notch } = g.head[1] else {
            panic!("quad expected")
        };
        let HeadSegment::Quad { to: right, .. } = g.head[2] else {
            panic!("quad expected")
        };
        assert!(close(left, pf(76.0, 8.0)), "{left:?}");
        assert!(close(control, pf(76.0, 2.0)), "{control:?}");
        assert!(close(notch, pf(84.0, 0.0)), "{notch:?}");
        assert!(close(right, pf(76.0, -8.0)), "{right:?}");
        assert!(close(g.shaft_end, pf(85.0, 0.0)));
        assert_eq!(g.head_points().count(), 6);
    }
}
