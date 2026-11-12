//! Smoke tests against the real desktop session of the machine running them.
//!
//! They need an interactive desktop. Without one they fail (no monitors, or
//! a refused or blank grab) rather than pass, because a capture crate that
//! cannot capture is not a pass.

#![cfg(windows)]

use std::collections::HashSet;

use phramer_capture::{
    CaptureTarget, Frame, Monitor, PhysicalRect, RegionMode, choose_target, cursor_position,
    enable_per_monitor_dpi_awareness, enumerate_monitors, grab_monitor, grab_virtual_desktop,
    monitor_at, virtual_bounds, visible_windows,
};

fn live_monitors() -> Vec<Monitor> {
    enable_per_monitor_dpi_awareness().expect("the test process must become per-monitor DPI aware");
    enumerate_monitors().expect("monitor enumeration needs an interactive desktop")
}

/// Size and opacity, which must hold for every grab.
fn assert_well_formed(frame: &Frame, what: &str) {
    let pixels = frame.bgra();
    assert_eq!(
        pixels.len(),
        frame.width() as usize * frame.height() as usize * 4,
        "{what}: buffer does not match its dimensions"
    );
    assert!(
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| pixel[3] == 255),
        "{what}: alpha must be opaque"
    );
}

/// The colour of a frame that is one colour throughout.
fn uniform_colour(frame: &Frame) -> Option<[u8; 4]> {
    let pixels = frame.bgra().as_chunks::<4>().0;
    let first = *pixels.first()?;
    pixels.iter().all(|pixel| *pixel == first).then_some(first)
}

fn assert_is_a_picture(frame: &Frame, what: &str) {
    assert_well_formed(frame, what);
    if let Some(colour) = uniform_colour(frame) {
        panic!(
            "{what}: every pixel is {colour:?}; the grab is blank (no interactive desktop, or a              session that refuses capture?)"
        );
    }
}

#[test]
fn enumeration_reports_sane_monitors() {
    let monitors = live_monitors();
    assert!(!monitors.is_empty());

    let primaries: Vec<_> = monitors
        .iter()
        .filter(|monitor| monitor.is_primary())
        .collect();
    assert_eq!(
        primaries.len(),
        1,
        "exactly one primary monitor: {monitors:#?}"
    );
    assert!(
        monitors[0].is_primary(),
        "the primary monitor is listed first"
    );
    // Windows defines the virtual desktop's origin as the primary's corner
    assert_eq!((primaries[0].bounds().x, primaries[0].bounds().y), (0, 0));

    let mut ids = HashSet::new();
    for monitor in &monitors {
        let bounds = monitor.bounds();
        assert!(
            (320..=16384).contains(&bounds.width),
            "implausible width: {monitor:?}"
        );
        assert!(
            (200..=16384).contains(&bounds.height),
            "implausible height: {monitor:?}"
        );
        assert!(
            (96..=960).contains(&monitor.dpi()),
            "implausible DPI: {monitor:?}"
        );
        assert!(!monitor.id().as_str().is_empty());
        assert!(
            ids.insert(monitor.id().clone()),
            "duplicate id: {monitor:?}"
        );
    }
    for (index, monitor) in monitors.iter().enumerate() {
        for other in &monitors[index + 1..] {
            assert_eq!(
                monitor.bounds().intersection(&other.bounds()),
                None,
                "monitors overlap, so the bounds are not physical: {monitor:?} {other:?}"
            );
        }
    }
}

#[test]
fn enumeration_is_stable() {
    assert_eq!(live_monitors(), live_monitors());
}

#[test]
fn a_monitor_grab_has_the_monitors_physical_size() {
    for monitor in live_monitors() {
        let what = monitor.id().as_str();
        let frame = grab_monitor(&monitor).expect("grabbing a monitor");
        assert_eq!(frame.bounds(), monitor.bounds());
        assert_eq!(frame.width() as i32, monitor.bounds().width);
        assert_eq!(frame.height() as i32, monitor.bounds().height);

        if monitor.is_primary() {
            // Something is always drawn on the primary monitor of an
            // interactive desktop: the taskbar, the lock screen, the wallpaper
            assert_is_a_picture(&frame, what);
        } else {
            // A secondary monitor may truly be one colour (a plain wallpaper
            // with nothing open, or the lock screen, which blanks every
            // monitor but the primary), so that alone is not a failure
            assert_well_formed(&frame, what);
            if let Some(colour) = uniform_colour(&frame) {
                eprintln!("note: {what} is uniformly {colour:?}; its content was not verified");
            }
        }
    }
}

#[test]
fn a_desktop_grab_is_the_union_of_the_monitors() {
    let monitors = live_monitors();
    let union = monitors
        .iter()
        .map(Monitor::bounds)
        .reduce(|all, bounds| all.union(&bounds))
        .expect("at least one monitor");
    assert_eq!(virtual_bounds(&monitors), Some(union));

    let frame = grab_virtual_desktop(&monitors).expect("grabbing the virtual desktop");
    assert_eq!(frame.bounds(), union);
    assert_eq!(
        (frame.width() as i32, frame.height() as i32),
        (union.width, union.height)
    );
    assert_is_a_picture(&frame, "virtual desktop");
}

#[test]
fn the_cursor_is_on_a_monitor_and_every_mode_finds_a_target() {
    let monitors = live_monitors();
    let cursor = cursor_position().expect("reading the cursor position");
    let under = monitor_at(&monitors, cursor)
        .unwrap_or_else(|| panic!("cursor {cursor:?} is on no monitor: {monitors:#?}"));

    let active = choose_target(&monitors, cursor, RegionMode::ActiveMonitor);
    assert_eq!(active, Some(CaptureTarget::Single(under)));
    for mode in [RegionMode::SelectMonitor, RegionMode::AllMonitors] {
        assert!(choose_target(&monitors, cursor, mode).is_some(), "{mode:?}");
    }
}

#[test]
fn the_window_list_holds_real_windows_on_the_desktop() {
    let monitors = live_monitors();
    let desktop: PhysicalRect = virtual_bounds(&monitors).expect("at least one monitor");
    let windows = visible_windows().expect("listing windows");

    // The shell's desktop window alone guarantees one
    assert!(
        !windows.is_empty(),
        "an interactive desktop always has a visible window"
    );
    assert!(windows.iter().all(|window| !window.is_empty()));
    assert!(
        windows
            .iter()
            .any(|window| window.intersection(&desktop).is_some()),
        "no window is on the desktop: {windows:?}"
    );
}
