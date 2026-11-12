//! Which monitor a GUI capture starts on (`captureRegionMode`).

use crate::geometry::PhysicalPoint;
use crate::monitor::{Monitor, monitor_at_or_primary};

/// The `captureRegionMode` setting. The discriminants are the stored values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionMode {
    /// 0: the user picks a monitor first.
    SelectMonitor = 0,
    /// 1: the monitor under the cursor, no questions asked.
    ActiveMonitor = 1,
    /// 2: every monitor is offered; the overlay follows the cursor.
    AllMonitors = 2,
}

/// What a GUI capture should do next, given the monitors and the cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureTarget<'a> {
    /// Open the overlay on this monitor.
    Single(&'a Monitor),

    /// Ask the user. `candidates` are in display order, left to right, and
    /// `highlighted` indexes the one to preselect (the one under the cursor).
    Pick {
        candidates: Vec<&'a Monitor>,
        highlighted: usize,
    },

    /// Open the overlay on `active` and dim `dimmed`. Until the first mouse
    /// press the overlay moves to whichever monitor the cursor enters (call
    /// this again with the new cursor position); the press commits to one
    /// monitor and removes the dimming. Each monitor keeps its own window:
    /// a single window cannot span monitors of different scale factors.
    Follow {
        active: &'a Monitor,
        dimmed: Vec<&'a Monitor>,
    },
}

/// Chooses the target monitor(s) for a GUI capture. `None` only when there
/// are no monitors.
///
/// With a single monitor every mode captures it. A cursor that is on no
/// monitor counts as being on the primary one.
pub fn choose_target(
    monitors: &[Monitor],
    cursor: PhysicalPoint,
    mode: RegionMode,
) -> Option<CaptureTarget<'_>> {
    let under_cursor = monitor_at_or_primary(monitors, cursor)?;
    if monitors.len() == 1 {
        return Some(CaptureTarget::Single(under_cursor));
    }

    Some(match mode {
        RegionMode::ActiveMonitor => CaptureTarget::Single(under_cursor),
        RegionMode::AllMonitors => CaptureTarget::Follow {
            active: under_cursor,
            dimmed: monitors
                .iter()
                .filter(|monitor| !std::ptr::eq(*monitor, under_cursor))
                .collect(),
        },
        RegionMode::SelectMonitor => {
            let mut candidates: Vec<&Monitor> = monitors.iter().collect();
            // Stable, so monitors stacked at the same x keep their order
            candidates.sort_by_key(|monitor| monitor.bounds().x);
            let highlighted = candidates
                .iter()
                .position(|monitor| std::ptr::eq(*monitor, under_cursor))
                .unwrap_or(0);
            CaptureTarget::Pick {
                candidates,
                highlighted,
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monitor::tests::{mixed_dpi, monitor};

    fn ids<'a>(monitors: &[&'a Monitor]) -> Vec<&'a str> {
        monitors
            .iter()
            .map(|monitor| monitor.id().as_str())
            .collect()
    }

    #[test]
    fn no_monitors_no_target() {
        assert_eq!(
            choose_target(&[], PhysicalPoint::new(0, 0), RegionMode::AllMonitors),
            None
        );
    }

    #[test]
    fn a_single_monitor_is_captured_in_every_mode() {
        let monitors = vec![monitor("only", (0, 0, 1920, 1080), 96)];
        for mode in [
            RegionMode::SelectMonitor,
            RegionMode::ActiveMonitor,
            RegionMode::AllMonitors,
        ] {
            // Even with the cursor somewhere it cannot be
            let target = choose_target(&monitors, PhysicalPoint::new(-50, 9000), mode);
            assert_eq!(
                target,
                Some(CaptureTarget::Single(&monitors[0])),
                "{mode:?}"
            );
        }
    }

    #[test]
    fn active_monitor_is_the_one_under_the_cursor() {
        let monitors = mixed_dpi();
        let target = |x, y| {
            choose_target(
                &monitors,
                PhysicalPoint::new(x, y),
                RegionMode::ActiveMonitor,
            )
        };
        assert_eq!(target(500, 500), Some(CaptureTarget::Single(&monitors[0])));
        assert_eq!(target(-500, 500), Some(CaptureTarget::Single(&monitors[1])));
        assert_eq!(
            target(2000, -500),
            Some(CaptureTarget::Single(&monitors[2]))
        );
    }

    #[test]
    fn active_monitor_falls_back_to_the_primary_off_the_desktop() {
        let monitors = mixed_dpi();
        let target = choose_target(
            &monitors,
            PhysicalPoint::new(-100, 100),
            RegionMode::ActiveMonitor,
        );
        assert_eq!(target, Some(CaptureTarget::Single(&monitors[0])));
    }

    #[test]
    fn all_monitors_follows_the_cursor_and_dims_the_rest() {
        let monitors = mixed_dpi();
        let Some(CaptureTarget::Follow { active, dimmed }) = choose_target(
            &monitors,
            PhysicalPoint::new(-1, 300),
            RegionMode::AllMonitors,
        ) else {
            panic!("expected a follow target");
        };
        assert_eq!(active.id().as_str(), "left");
        assert_eq!(ids(&dimmed), ["primary", "above"]);

        // The cursor crosses onto the monitor above: same call, new answer
        let Some(CaptureTarget::Follow { active, dimmed }) = choose_target(
            &monitors,
            PhysicalPoint::new(-1, -1),
            RegionMode::AllMonitors,
        ) else {
            panic!("expected a follow target");
        };
        assert_eq!(active.id().as_str(), "above");
        assert_eq!(ids(&dimmed), ["primary", "left"]);
    }

    #[test]
    fn select_lists_monitors_left_to_right_and_highlights_the_cursor_one() {
        let monitors = mixed_dpi();
        let Some(CaptureTarget::Pick {
            candidates,
            highlighted,
        }) = choose_target(
            &monitors,
            PhysicalPoint::new(10, 10),
            RegionMode::SelectMonitor,
        )
        else {
            panic!("expected a pick target");
        };
        assert_eq!(ids(&candidates), ["left", "above", "primary"]);
        assert_eq!(candidates[highlighted].id().as_str(), "primary");
    }

    #[test]
    fn select_keeps_stacked_monitors_in_enumeration_order() {
        let monitors = vec![
            monitor("bottom", (0, 0, 1920, 1080), 96),
            monitor("top", (0, -1080, 1920, 1080), 96),
            monitor("left", (-1280, 0, 1280, 1024), 96),
        ];
        let Some(CaptureTarget::Pick {
            candidates,
            highlighted,
        }) = choose_target(
            &monitors,
            PhysicalPoint::new(100, -100),
            RegionMode::SelectMonitor,
        )
        else {
            panic!("expected a pick target");
        };
        assert_eq!(ids(&candidates), ["left", "bottom", "top"]);
        assert_eq!(candidates[highlighted].id().as_str(), "top");
    }

    #[test]
    fn select_highlights_the_primary_when_the_cursor_is_in_a_gap() {
        let monitors = vec![
            monitor("far", (-2120, 0, 1920, 1080), 96),
            monitor("primary", (0, 0, 1920, 1080), 96),
        ];
        let Some(CaptureTarget::Pick {
            candidates,
            highlighted,
        }) = choose_target(
            &monitors,
            PhysicalPoint::new(-100, 10),
            RegionMode::SelectMonitor,
        )
        else {
            panic!("expected a pick target");
        };
        assert_eq!(candidates[highlighted].id().as_str(), "primary");
    }
}
