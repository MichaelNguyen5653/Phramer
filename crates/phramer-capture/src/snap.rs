//! Window snap: which window is under the mouse.
//!
//! The windows are listed once, when the capture starts, so they match the
//! frozen screenshot rather than whatever has moved since. Their rectangles
//! are physical, the only space in which DWM reports a window's visible
//! frame; map them onto an overlay with
//! [`Monitor::rect_to_logical`](crate::Monitor::rect_to_logical).

use crate::geometry::{PhysicalPoint, PhysicalRect};

/// The first window containing `point`, or `None` over no window. `windows`
/// is topmost first, as [`visible_windows`](crate::visible_windows) lists
/// them, so the first hit is the window the user sees there.
pub fn window_at(windows: &[PhysicalRect], point: PhysicalPoint) -> Option<PhysicalRect> {
    windows
        .iter()
        .copied()
        .find(|window| window.contains(point))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::LogicalRect;
    use crate::monitor::tests::monitor;

    #[test]
    fn picks_the_topmost_window_under_the_point() {
        let windows = [
            PhysicalRect::new(100, 100, 200, 200),
            PhysicalRect::new(0, 0, 1000, 800),
        ];
        assert_eq!(
            window_at(&windows, PhysicalPoint::new(150, 150)),
            Some(windows[0])
        );
        assert_eq!(
            window_at(&windows, PhysicalPoint::new(500, 500)),
            Some(windows[1])
        );
    }

    #[test]
    fn returns_nothing_over_no_window() {
        let windows = [PhysicalRect::new(100, 100, 200, 200)];
        assert_eq!(window_at(&windows, PhysicalPoint::new(5, 5)), None);
        assert_eq!(window_at(&[], PhysicalPoint::new(5, 5)), None);
    }

    #[test]
    fn the_edge_pixel_belongs_to_the_window_behind() {
        let windows = [
            PhysicalRect::new(100, 100, 200, 200),
            PhysicalRect::new(0, 0, 1000, 800),
        ];
        assert_eq!(
            window_at(&windows, PhysicalPoint::new(299, 299)),
            Some(windows[0])
        );
        assert_eq!(
            window_at(&windows, PhysicalPoint::new(300, 299)),
            Some(windows[1])
        );
    }

    #[test]
    fn three_overlapping_windows_resolve_by_z_order() {
        let windows = [
            PhysicalRect::new(300, 300, 100, 100),
            PhysicalRect::new(200, 200, 300, 300),
            PhysicalRect::new(-500, -500, 2000, 2000),
        ];
        assert_eq!(
            window_at(&windows, PhysicalPoint::new(350, 350)),
            Some(windows[0])
        );
        assert_eq!(
            window_at(&windows, PhysicalPoint::new(250, 250)),
            Some(windows[1])
        );
        assert_eq!(
            window_at(&windows, PhysicalPoint::new(-400, -400)),
            Some(windows[2])
        );
    }

    #[test]
    fn a_window_spanning_two_monitors_is_found_from_either() {
        // 100% monitor left of a 125% primary; the window straddles the seam
        let left = monitor("left", (-1920, 0, 1920, 1080), 96);
        let primary = monitor("primary", (0, 0, 1920, 1200), 120);
        let windows = [PhysicalRect::new(-300, 100, 800, 500)];

        let from_left = window_at(&windows, PhysicalPoint::new(-10, 300)).unwrap();
        let from_primary = window_at(&windows, PhysicalPoint::new(10, 300)).unwrap();
        assert_eq!(from_left, from_primary);

        // Each overlay highlights only its own part, at its own scale
        assert_eq!(
            left.rect_to_logical(from_left),
            Some(LogicalRect::new(1620, 100, 300, 500))
        );
        assert_eq!(
            primary.rect_to_logical(from_primary),
            Some(LogicalRect::new(0, 80, 400, 400))
        );
    }

    #[test]
    fn empty_rectangles_are_never_hit() {
        let windows = [
            PhysicalRect::new(0, 0, 0, 0),
            PhysicalRect::new(0, 0, 10, 10),
        ];
        assert_eq!(
            window_at(&windows, PhysicalPoint::new(0, 0)),
            Some(windows[1])
        );
    }
}
