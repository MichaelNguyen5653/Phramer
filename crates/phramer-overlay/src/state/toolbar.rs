//! Where the toolbar's buttons go: 15.0.0's `ButtonHandler::updatePosition`,
//! in logical pixels. Buttons fill the bottom of the selection, then its
//! right, top and left, in rings further out until all are placed; a side
//! too close to the monitor's edge is skipped, and when every side is
//! blocked they go in rows inside the selection, from its bottom up.
//!
//! The rectangles follow `QRect`: `right` and `bottom` are the last pixel,
//! not one past it, because the positions depend on it.

use phramer_capture::{LogicalPoint, LogicalRect};

/// A button's side (`--space-32`), the base size every distance derives
/// from.
pub(crate) const BUTTON_SIZE: i32 = 32;
/// The gap between buttons and the selection (`m_separator`, a quarter
/// button: `--space-8`).
pub(crate) const SEPARATOR: i32 = BUTTON_SIZE / 4;
/// One button and its gap (`m_buttonExtendedSize`).
const EXTENDED: i32 = BUTTON_SIZE + SEPARATOR;
/// A safety stop: each ring is larger, so the loop ends long before.
const MAX_RINGS: usize = 64;

/// A `QRect`, with its inclusive edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Area {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

impl Area {
    fn from_rect(rect: LogicalRect) -> Self {
        Self {
            x: rect.x,
            y: rect.y,
            w: rect.width,
            h: rect.height,
        }
    }

    fn right(&self) -> i32 {
        self.x + self.w - 1
    }

    fn bottom(&self) -> i32 {
        self.y + self.h - 1
    }

    fn center(&self) -> (i32, i32) {
        ((self.x + self.right()) / 2, (self.y + self.bottom()) / 2)
    }

    fn is_empty(&self) -> bool {
        self.w <= 0 || self.h <= 0
    }

    fn contains(&self, x: i32, y: i32) -> bool {
        !self.is_empty() && x >= self.x && x <= self.right() && y >= self.y && y <= self.bottom()
    }

    fn intersected(&self, other: &Area) -> Area {
        let left = self.x.max(other.x);
        let top = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        if right < left || bottom < top {
            return Area {
                x: 0,
                y: 0,
                w: 0,
                h: 0,
            };
        }
        Area {
            x: left,
            y: top,
            w: right - left + 1,
            h: bottom - top + 1,
        }
    }

    fn grown(&self, by: i32) -> Area {
        Area {
            x: self.x - by,
            y: self.y - by,
            w: self.w + 2 * by,
            h: self.h + 2 * by,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Blocked {
    right: bool,
    left: bool,
    bottom: bool,
    top: bool,
}

impl Blocked {
    /// `updateBlockedSides`: a side is free when a button and its gap fit
    /// past both of its ends.
    fn of(area: &Area, screen: &Area) -> Self {
        let reach = EXTENDED + SEPARATOR;
        let fits =
            |a: (i32, i32), b: (i32, i32)| screen.contains(a.0, a.1) && screen.contains(b.0, b.1);
        let right = area.right() + reach;
        let left = area.x - reach;
        let bottom = area.bottom() + reach;
        let top = area.y - reach;
        Self {
            right: !fits((right, area.bottom()), (right, area.y)),
            left: !fits((left, area.bottom()), (left, area.y)),
            bottom: !fits((area.x, bottom), (area.right(), bottom)),
            top: !fits((area.x, top), (area.right(), top)),
        }
    }

    fn one_horizontal(&self) -> bool {
        self.right != self.left
    }

    fn both_horizontal(&self) -> bool {
        self.right && self.left
    }

    fn all(&self) -> bool {
        self.bottom && self.both_horizontal() && self.top
    }
}

/// `calculateShift`: from the centre to the first button.
fn shift(count: i32, reverse: bool) -> i32 {
    let mut shift = if count % 2 == 0 {
        EXTENDED * (count / 2) - SEPARATOR / 2
    } else {
        EXTENDED * ((count - 1) / 2) + BUTTON_SIZE / 2
    };
    if !reverse {
        shift -= BUTTON_SIZE;
    }
    shift
}

fn horizontal(center: (i32, i32), count: i32, left_to_right: bool) -> Vec<(i32, i32)> {
    let offset = shift(count, left_to_right);
    let (start, step) = if left_to_right {
        (center.0 - offset, EXTENDED)
    } else {
        (center.0 + offset, -EXTENDED)
    };
    (0..count).map(|i| (start + i * step, center.1)).collect()
}

fn vertical(center: (i32, i32), count: i32, top_to_bottom: bool) -> Vec<(i32, i32)> {
    let offset = shift(count, top_to_bottom);
    let (start, step) = if top_to_bottom {
        (center.1 - offset, EXTENDED)
    } else {
        (center.1 + offset, -EXTENDED)
    };
    (0..count).map(|i| (center.0, start + i * step)).collect()
}

/// `adjustHorizontalCenter`: away from a blocked side.
fn adjusted(center: (i32, i32), blocked: &Blocked) -> (i32, i32) {
    if blocked.left {
        (center.0 + EXTENDED / 2, center.1)
    } else if blocked.right {
        (center.0 - EXTENDED / 2, center.1)
    } else {
        center
    }
}

/// The buttons' rectangles for `count` buttons around `selection` on a
/// monitor of logical size `screen`, in order, and whether they ended up
/// inside the selection. Fewer rectangles than buttons come back only when
/// the selection is too narrow to hold even one row inside.
pub(crate) fn layout(
    selection: LogicalRect,
    screen: LogicalRect,
    count: usize,
) -> (Vec<LogicalRect>, bool) {
    let total = i32::try_from(count).unwrap_or(i32::MAX);
    let mut placed: Vec<(i32, i32)> = Vec::with_capacity(count);
    if total == 0 {
        return (Vec::new(), false);
    }
    let screen = Area::from_rect(screen);
    let original = Area::from_rect(selection).intersected(&screen);
    let mut area = original;
    let mut blocked = Blocked::of(&area, &screen);
    // `ensureSelectionMinimumSize`
    if area.w < BUTTON_SIZE {
        if !blocked.left {
            area.x -= (BUTTON_SIZE - area.w) / 2;
        }
        area.w = BUTTON_SIZE;
    }
    if area.h < BUTTON_SIZE {
        if !blocked.top {
            area.y -= (BUTTON_SIZE - area.h) / 2;
        }
        area.h = BUTTON_SIZE;
    }
    let mut inside = false;
    let remaining = |placed: &Vec<(i32, i32)>| total - placed.len() as i32;
    for _ in 0..MAX_RINGS {
        if remaining(&placed) <= 0 {
            break;
        }
        if blocked.all() {
            inside = true;
            place_inside(&original, &screen, total, &mut placed);
            break;
        }
        let per_row = (area.w + SEPARATOR) / EXTENDED;
        let per_column = (area.h + SEPARATOR) / EXTENDED;
        let extra = remaining(&placed) - (per_row + per_column) * 2;
        let mut corners = extra.min(4);
        let max_extra = if blocked.one_horizontal() {
            1
        } else if blocked.both_horizontal() {
            0
        } else {
            2
        };
        let corners_top = corners.clamp(0, max_extra);
        corners -= corners_top;
        let corners_bottom = corners.clamp(0, max_extra);
        let (center_x, center_y) = area.center();

        if !blocked.bottom {
            let n = (per_row + corners_bottom).clamp(0, remaining(&placed));
            let mut center = (center_x, area.bottom() + SEPARATOR);
            if n > per_row {
                center = adjusted(center, &blocked);
            }
            placed.extend(horizontal(center, n, true));
        }
        if !blocked.right && remaining(&placed) > 0 {
            let n = per_column.clamp(0, remaining(&placed));
            placed.extend(vertical((area.right() + SEPARATOR, center_y), n, false));
        }
        if !blocked.top && remaining(&placed) > 0 {
            let n = (per_row + corners_top).clamp(0, remaining(&placed));
            let mut center = (center_x, area.y - EXTENDED);
            if n == per_row + 1 {
                center = adjusted(center, &blocked);
            }
            placed.extend(horizontal(center, n, false));
        }
        if !blocked.left && remaining(&placed) > 0 {
            let n = per_column.clamp(0, remaining(&placed));
            placed.extend(vertical((area.x - EXTENDED, center_y), n, true));
        }
        if remaining(&placed) > 0 {
            area = area.grown(EXTENDED).intersected(&screen);
        }
        blocked = Blocked::of(&area, &screen);
    }
    let rects = placed
        .into_iter()
        .map(|(x, y)| LogicalRect::new(x, y, BUTTON_SIZE, BUTTON_SIZE))
        .collect();
    (rects, inside)
}

/// `positionButtonsInside`: rows along the bottom of the selection, going
/// up.
fn place_inside(selection: &Area, screen: &Area, total: i32, placed: &mut Vec<(i32, i32)>) {
    let area = selection.intersected(screen);
    let per_row = area.w / EXTENDED;
    if per_row == 0 {
        return;
    }
    let (center_x, _) = area.center();
    let mut center = (center_x, area.bottom() - EXTENDED);
    while (placed.len() as i32) < total {
        let n = per_row.min(total - placed.len() as i32);
        placed.extend(horizontal(center, n, true));
        center.1 -= EXTENDED;
    }
}

/// The button under `at`, by index.
pub(crate) fn button_at(rects: &[LogicalRect], at: LogicalPoint) -> Option<usize> {
    rects.iter().position(|rect| rect.contains(at))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen() -> LogicalRect {
        LogicalRect::new(0, 0, 1920, 1080)
    }

    #[test]
    fn a_short_row_sits_centred_under_the_selection() {
        // 400 wide: (400 + 8) / 40 = 10 per row, three fit under; the
        // last row of the selection is 399, plus the gap
        let (rects, inside) = layout(LogicalRect::new(100, 100, 400, 300), screen(), 3);
        assert!(!inside);
        let tops: Vec<i32> = rects.iter().map(|r| r.y).collect();
        assert_eq!(tops, [407, 407, 407]);
        let lefts: Vec<i32> = rects.iter().map(|r| r.x).collect();
        // Centre x 299; shift for 3 = 40 + 16 = 56
        assert_eq!(lefts, [243, 283, 323]);
    }

    #[test]
    fn buttons_wrap_to_the_right_then_the_top() {
        let (rects, _) = layout(LogicalRect::new(400, 300, 200, 120), screen(), 12);
        // (200 + 8) / 40 = 5 per row, (120 + 8) / 40 = 3 per column; extra
        // 12 - 16 < 0, no corners: 5 bottom, 3 right, 4 top
        assert!(rects[..5].iter().all(|r| r.y == 300 + 119 + SEPARATOR));
        assert!(rects[5..8].iter().all(|r| r.x == 400 + 199 + SEPARATOR));
        assert!(rects[8..].iter().all(|r| r.y == 300 - EXTENDED));
    }

    #[test]
    fn a_selection_at_the_bottom_edge_puts_buttons_above_it() {
        let (rects, inside) = layout(LogicalRect::new(400, 900, 400, 180), screen(), 4);
        assert!(!inside);
        // Bottom and right? The right is free, but the bottom is not
        assert!(rects.iter().all(|r| r.bottom() <= 1080));
        assert!(rects.iter().all(|r| r.y < 900 || r.x > 799));
    }

    #[test]
    fn a_whole_monitor_selection_puts_buttons_inside() {
        let (rects, inside) = layout(screen(), screen(), 21);
        assert!(inside);
        assert_eq!(rects.len(), 21);
        assert!(
            rects
                .iter()
                .all(|r| r.x >= 0 && r.right() <= 1920 && r.y >= 0)
        );
        // One row of 21 at the bottom
        assert!(rects.iter().all(|r| r.y == 1079 - EXTENDED));
    }

    #[test]
    fn every_button_gets_a_place_and_none_overlap() {
        for (selection, count) in [
            (LogicalRect::new(900, 500, 20, 10), 21),
            (LogicalRect::new(0, 0, 300, 200), 21),
            (LogicalRect::new(1700, 900, 220, 180), 27),
        ] {
            let (rects, _) = layout(selection, screen(), count);
            assert_eq!(rects.len(), count, "{selection:?}");
            for (i, a) in rects.iter().enumerate() {
                for b in &rects[i + 1..] {
                    assert!(a.intersection(b).is_none(), "{a:?} {b:?} for {selection:?}");
                }
            }
        }
    }
}
