//! The selection rectangle: which part of it is under the pointer, and how
//! drags and keys reshape it (`SelectionWidget` in 15.0.0).

use phramer_capture::{LogicalPoint, LogicalRect};

/// Half the size of the square grab area around each corner, and half the
/// width of the strip along each edge, in logical pixels. 15.0.0 made the
/// area 0.6 of a button (about 21 px at 100%).
pub(crate) const GRAB_RADIUS: i32 = 10;

/// Pixels (Manhattan) a press must travel before it is a drag rather than
/// a click (`MOUSE_DISTANCE_TO_START_MOVING`).
pub(crate) const CLICK_DISTANCE: i32 = 3;

/// The part of the selection a pointer is over, which is also what a drag
/// started there changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    TopLeft,
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
    /// Inside, away from the edges: a drag moves the selection.
    Center,
}

impl Side {
    const fn moves_left(self) -> bool {
        matches!(self, Side::TopLeft | Side::Left | Side::BottomLeft)
    }

    const fn moves_right(self) -> bool {
        matches!(self, Side::TopRight | Side::Right | Side::BottomRight)
    }

    const fn moves_top(self) -> bool {
        matches!(self, Side::TopLeft | Side::Top | Side::TopRight)
    }

    const fn moves_bottom(self) -> bool {
        matches!(self, Side::BottomLeft | Side::Bottom | Side::BottomRight)
    }

    /// The same handle after the edges it moves have crossed over
    /// (`getProperSide`).
    fn flipped(self, horizontal: bool, vertical: bool) -> Self {
        let mut side = self;
        if horizontal {
            side = match side {
                Side::TopLeft => Side::TopRight,
                Side::TopRight => Side::TopLeft,
                Side::Left => Side::Right,
                Side::Right => Side::Left,
                Side::BottomLeft => Side::BottomRight,
                Side::BottomRight => Side::BottomLeft,
                other => other,
            };
        }
        if vertical {
            side = match side {
                Side::TopLeft => Side::BottomLeft,
                Side::BottomLeft => Side::TopLeft,
                Side::Top => Side::Bottom,
                Side::Bottom => Side::Top,
                Side::TopRight => Side::BottomRight,
                Side::BottomRight => Side::TopRight,
                other => other,
            };
        }
        side
    }
}

/// The part of `rect` under `point` (`SelectionWidget::getMouseSide`):
/// corners first, then edges, then the inside. `None` outside all of them.
pub(crate) fn side_at(rect: LogicalRect, point: LogicalPoint) -> Option<Side> {
    let near = |a: i32, b: i32| (a - b).abs() <= GRAB_RADIUS;
    let (left, top, right, bottom) = (rect.x, rect.y, rect.right(), rect.bottom());
    let within_x = point.x >= left - GRAB_RADIUS && point.x <= right + GRAB_RADIUS;
    let within_y = point.y >= top - GRAB_RADIUS && point.y <= bottom + GRAB_RADIUS;
    let corners = [
        (left, top, Side::TopLeft),
        (right, top, Side::TopRight),
        (left, bottom, Side::BottomLeft),
        (right, bottom, Side::BottomRight),
    ];
    if let Some((_, _, side)) = corners
        .iter()
        .find(|(x, y, _)| near(point.x, *x) && near(point.y, *y))
    {
        return Some(*side);
    }
    if near(point.x, left) && within_y {
        return Some(Side::Left);
    }
    if near(point.y, top) && within_x {
        return Some(Side::Top);
    }
    if near(point.x, right) && within_y {
        return Some(Side::Right);
    }
    if near(point.y, bottom) && within_x {
        return Some(Side::Bottom);
    }
    rect.contains(point).then_some(Side::Center)
}

/// The rectangle spanned by two points, at least one pixel each way.
pub(crate) fn spanned(a: LogicalPoint, b: LogicalPoint) -> LogicalRect {
    normalized(a.x, a.y, b.x, b.y).0
}

/// Edges in any order as a rectangle of at least one pixel each way, and
/// whether the horizontal and vertical edges had crossed.
fn normalized(left: i32, top: i32, right: i32, bottom: i32) -> (LogicalRect, bool, bool) {
    let (l, r, crossed_x) = if right < left {
        (right, left, true)
    } else {
        (left, right, false)
    };
    let (t, b, crossed_y) = if bottom < top {
        (bottom, top, true)
    } else {
        (top, bottom, false)
    };
    (
        LogicalRect::from_edges(l, t, r.max(l + 1), b.max(t + 1)),
        crossed_x,
        crossed_y,
    )
}

/// Moves the edges `side` holds to `pointer` (`parentMouseMoveEvent`).
///
/// `symmetric` (Shift) moves the opposite edges the other way, keeping the
/// centre; `keep_aspect` (Ctrl) keeps the width-to-height ratio. Returns
/// the new rectangle and the side now held: dragging an edge past its
/// opposite flips which edge the pointer holds.
pub(crate) fn resize(
    rect: LogicalRect,
    side: Side,
    pointer: LogicalPoint,
    symmetric: bool,
    keep_aspect: bool,
) -> (LogicalRect, Side) {
    let (left, top, right, bottom) = (rect.x, rect.y, rect.right(), rect.bottom());
    let aspect = f64::from(rect.width.max(1)) / f64::from(rect.height.max(1));
    let (mut new_left, mut new_top, mut new_right, mut new_bottom) = (left, top, right, bottom);
    let (x, y) = (pointer.x, pointer.y);
    // The 15.0.0 arithmetic: truncate towards zero
    let by_aspect = |length: i32| (f64::from(length) / aspect) as i32;
    let times_aspect = |length: i32| (f64::from(length) * aspect) as i32;

    match side {
        Side::TopLeft if keep_aspect => {
            if ratio(right - x, bottom - y) > aspect {
                new_left = x;
                new_top = bottom - by_aspect(right - x);
            } else {
                new_top = y;
                new_left = right - times_aspect(bottom - y);
            }
        }
        Side::BottomRight if keep_aspect => {
            if ratio(x - left, y - top) > aspect {
                new_right = x;
                new_bottom = top + by_aspect(x - left);
            } else {
                new_bottom = y;
                new_right = left + times_aspect(y - top);
            }
        }
        Side::TopRight if keep_aspect => {
            if ratio(x - left, bottom - y) > aspect {
                new_right = x;
                new_top = bottom - by_aspect(x - left);
            } else {
                new_top = y;
                new_right = left + times_aspect(bottom - y);
            }
        }
        Side::BottomLeft if keep_aspect => {
            if ratio(right - x, y - top) > aspect {
                new_left = x;
                new_bottom = top + by_aspect(right - x);
            } else {
                new_bottom = y;
                new_left = right - times_aspect(y - top);
            }
        }
        Side::Left => {
            new_left = x;
            if keep_aspect {
                new_bottom = top + by_aspect(right - x);
            }
        }
        Side::Right => {
            new_right = x;
            if keep_aspect {
                new_bottom = top + by_aspect(x - left);
            }
        }
        Side::Top => {
            new_top = y;
            if keep_aspect {
                new_right = left + times_aspect(bottom - y);
            }
        }
        Side::Bottom => {
            new_bottom = y;
            if keep_aspect {
                new_right = left + times_aspect(y - top);
            }
        }
        Side::Center => return (rect, side),
        corner => {
            if corner.moves_left() {
                new_left = x;
            }
            if corner.moves_right() {
                new_right = x;
            }
            if corner.moves_top() {
                new_top = y;
            }
            if corner.moves_bottom() {
                new_bottom = y;
            }
        }
    }

    if symmetric {
        let (moved_left, moved_top) = (new_left - left, new_top - top);
        let (moved_right, moved_bottom) = (new_right - right, new_bottom - bottom);
        new_left = left + moved_left - moved_right;
        new_top = top + moved_top - moved_bottom;
        new_right = right + moved_right - moved_left;
        new_bottom = bottom + moved_bottom - moved_top;
    }

    let (rect, crossed_x, crossed_y) = normalized(new_left, new_top, new_right, new_bottom);
    (rect, side.flipped(crossed_x, crossed_y))
}

fn ratio(width: i32, height: i32) -> f64 {
    f64::from(width) / f64::from(height)
}

/// `rect` shifted by a vector.
pub(crate) fn translated(rect: LogicalRect, dx: i32, dy: i32) -> LogicalRect {
    LogicalRect::new(rect.x + dx, rect.y + dy, rect.width, rect.height)
}

/// A keyboard nudge (`SelectionWidget::setGeometryByKeyboard`): the edges
/// moved by the given amounts, kept on the monitor and at least one pixel
/// each way.
pub(crate) fn nudged(
    rect: LogicalRect,
    monitor: LogicalRect,
    (left, top, right, bottom): (i32, i32, i32, i32),
) -> LogicalRect {
    // Each axis on its own: a width squeezed to nothing keeps the height
    let axis = |start: i32, end: i32, min: i32, max: i32| {
        let start = start.clamp(min, max - 1);
        (start, (end.min(max) - start).max(1))
    };
    let (x, width) = axis(
        rect.x + left,
        rect.right() + right,
        monitor.x,
        monitor.right(),
    );
    let (y, height) = axis(
        rect.y + top,
        rect.bottom() + bottom,
        monitor.y,
        monitor.bottom(),
    );
    LogicalRect::new(x, y, width, height)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: i32, y: i32) -> LogicalPoint {
        LogicalPoint::new(x, y)
    }

    #[test]
    fn corners_win_over_edges_and_edges_over_the_inside() {
        let rect = LogicalRect::new(100, 100, 200, 100);
        assert_eq!(side_at(rect, p(100, 100)), Some(Side::TopLeft));
        assert_eq!(side_at(rect, p(91, 109)), Some(Side::TopLeft));
        assert_eq!(side_at(rect, p(300, 200)), Some(Side::BottomRight));
        assert_eq!(side_at(rect, p(100, 150)), Some(Side::Left));
        assert_eq!(side_at(rect, p(200, 95)), Some(Side::Top));
        assert_eq!(side_at(rect, p(305, 150)), Some(Side::Right));
        assert_eq!(side_at(rect, p(200, 200)), Some(Side::Bottom));
        assert_eq!(side_at(rect, p(200, 150)), Some(Side::Center));
        assert_eq!(side_at(rect, p(80, 150)), None);
        assert_eq!(side_at(rect, p(200, 260)), None);
    }

    #[test]
    fn dragging_an_edge_past_its_opposite_flips_the_handle() {
        let rect = LogicalRect::new(100, 100, 200, 100);
        let (resized, side) = resize(rect, Side::Left, p(400, 150), false, false);
        assert_eq!(resized, LogicalRect::from_edges(300, 100, 400, 200));
        assert_eq!(side, Side::Right);
        let (resized, side) = resize(rect, Side::TopLeft, p(350, 250), false, false);
        assert_eq!(resized, LogicalRect::from_edges(300, 200, 350, 250));
        assert_eq!(side, Side::BottomRight);
    }

    #[test]
    fn shift_resizes_around_the_centre() {
        let rect = LogicalRect::new(100, 100, 200, 100);
        let (resized, _) = resize(rect, Side::Right, p(320, 150), true, false);
        assert_eq!(resized, LogicalRect::from_edges(80, 100, 320, 200));
    }

    #[test]
    fn ctrl_keeps_the_aspect_ratio() {
        let rect = LogicalRect::new(0, 0, 200, 100);
        let (resized, _) = resize(rect, Side::BottomRight, p(400, 120), false, true);
        assert_eq!(resized, LogicalRect::new(0, 0, 400, 200));
        let (resized, _) = resize(rect, Side::Right, p(300, 50), false, true);
        assert_eq!(resized, LogicalRect::new(0, 0, 300, 150));
    }

    #[test]
    fn nudges_stay_on_the_monitor_and_keep_a_pixel() {
        let monitor = LogicalRect::new(0, 0, 800, 600);
        let rect = LogicalRect::new(0, 10, 50, 50);
        assert_eq!(
            nudged(rect, monitor, (-1, 0, -1, 0)),
            LogicalRect::new(0, 10, 49, 50)
        );
        let thin = LogicalRect::new(10, 10, 1, 50);
        assert_eq!(
            nudged(thin, monitor, (0, 0, -1, 0)),
            LogicalRect::new(10, 10, 1, 50)
        );
        let edge = LogicalRect::new(790, 0, 10, 10);
        assert_eq!(
            nudged(edge, monitor, (1, 0, 1, 0)),
            LogicalRect::new(791, 0, 9, 10)
        );
    }
}
