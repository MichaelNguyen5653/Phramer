//! Live tests of the 15.0.0 behaviours brought to the overlay: the side
//! panel's Move down on a real overlay window, the mode bar's Video, and
//! the system font lookup for `fontFamily`. Ignored by default: they show
//! the fullscreen overlay or read the system's fonts; they run in Windows
//! Sandbox (`packaging/windows/sandbox/scenario-tests.ps1`). Only messages
//! posted to the overlay's own window; no real input.
//!
//! ```text
//! cargo test -p phramer-overlay --test v15_live -- --ignored --nocapture --test-threads=1
//! ```

#![cfg(windows)]

use std::time::{Duration, Instant};

use phramer_capture::{LogicalRect, enumerate_monitors};
use phramer_overlay::{
    BUNDLED_FONT, Finished, Outcome, OverlayConfig, OverlayError, OverlaySession, Theme,
    mode_bar_layout, run_overlay, side_panel_layout, system_font_faces, system_font_families,
};
use windows_sys::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows_sys::Win32::System::StationsAndDesktops::{
    CloseDesktop, DESKTOP_READOBJECTS, GetUserObjectInformationW, OpenInputDesktop, UOI_NAME,
};
use windows_sys::Win32::System::SystemServices::MK_LBUTTON;
use windows_sys::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{VK_ESCAPE, VK_RETURN, VK_SPACE};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, GetWindowThreadProcessId, IsWindowVisible, PostMessageW, WM_KEYDOWN,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
};

const OVERLAY_CLASS: &str = "PhramerCaptureOverlay";

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

/// Whether the interactive desktop is the user's: only then can a window
/// be shown and the screen grabbed.
fn desktop_is_unlocked() -> bool {
    // SAFETY: opens the input desktop for reading its name; the handle is
    // closed below. A locked session refuses the open.
    let desktop = unsafe { OpenInputDesktop(0, 0, DESKTOP_READOBJECTS) };
    if desktop.is_null() {
        return false;
    }
    let mut name = [0u16; 64];
    let mut needed = 0u32;
    // SAFETY: `name` is a writable buffer of the byte size given.
    let ok = unsafe {
        GetUserObjectInformationW(
            desktop,
            UOI_NAME,
            name.as_mut_ptr().cast(),
            (name.len() * 2) as u32,
            &mut needed,
        )
    };
    // SAFETY: the handle opened above, closed once.
    unsafe { CloseDesktop(desktop) };
    let name = String::from_utf16_lossy(&name);
    ok != 0 && name.trim_end_matches('\0').eq_ignore_ascii_case("Default")
}

/// An overlay window of this process.
fn find_own_overlay(timeout: Duration) -> Option<HWND> {
    let class = wide(OVERLAY_CLASS);
    let started = Instant::now();
    while started.elapsed() < timeout {
        let mut hwnd = std::ptr::null_mut();
        loop {
            // SAFETY: a lookup by class name after `hwnd` (null: from the
            // top); the string outlives the call.
            hwnd = unsafe {
                FindWindowExW(std::ptr::null_mut(), hwnd, class.as_ptr(), std::ptr::null())
            };
            if hwnd.is_null() {
                break;
            }
            let mut pid = 0;
            // SAFETY: writes the owner's process id to `pid`.
            unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
            if pid == std::process::id() {
                return Some(hwnd);
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    None
}

fn post(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) {
    // SAFETY: posts a plain message to a window of this process.
    unsafe { PostMessageW(hwnd, message, wparam, lparam) };
    std::thread::sleep(Duration::from_millis(15));
}

/// Client coordinates (physical pixels) as `WM_*BUTTON*` carries them.
fn at(x: i32, y: i32) -> LPARAM {
    ((y as u32 & 0xffff) << 16 | (x as u32 & 0xffff)) as LPARAM
}

fn click(hwnd: HWND, (x, y): (i32, i32)) {
    post(hwnd, WM_MOUSEMOVE, 0, at(x, y));
    post(hwnd, WM_LBUTTONDOWN, MK_LBUTTON as WPARAM, at(x, y));
    post(hwnd, WM_LBUTTONUP, 0, at(x, y));
    // Apart from the next click, so no two make a double-click
    std::thread::sleep(Duration::from_millis(600));
}

fn drag(hwnd: HWND, from: (i32, i32), to: (i32, i32)) {
    post(hwnd, WM_MOUSEMOVE, 0, at(from.0, from.1));
    post(
        hwnd,
        WM_LBUTTONDOWN,
        MK_LBUTTON as WPARAM,
        at(from.0, from.1),
    );
    for step in 1..=4 {
        let x = from.0 + (to.0 - from.0) * step / 4;
        let y = from.1 + (to.1 - from.1) * step / 4;
        post(hwnd, WM_MOUSEMOVE, MK_LBUTTON as WPARAM, at(x, y));
    }
    post(hwnd, WM_LBUTTONUP, 0, at(to.0, to.1));
    std::thread::sleep(Duration::from_millis(600));
}

fn key(hwnd: HWND, vk: u16) {
    post(hwnd, WM_KEYDOWN, WPARAM::from(vk), 1);
}

/// Opens the overlay on the primary monitor with `config`; `None` (after
/// saying why) where the screen cannot be grabbed. With the monitor's
/// scale and logical size.
#[allow(clippy::type_complexity)]
fn open(
    config: OverlayConfig,
) -> Option<(
    HWND,
    f64,
    (i32, i32),
    std::thread::JoinHandle<Result<Finished, OverlayError>>,
)> {
    // SAFETY: sets this test process's DPI awareness before any window.
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    let monitors = enumerate_monitors().expect("monitors");
    let active = monitors
        .iter()
        .position(|monitor| monitor.is_primary())
        .unwrap_or(0);
    let scale = monitors[active].scale();
    let size = monitors[active].logical_size();
    println!(
        "monitor {:?} at {scale}x ({} monitors)",
        monitors[active].bounds(),
        monitors.len()
    );
    let overlay = std::thread::spawn(move || {
        run_overlay(OverlaySession {
            monitors,
            active,
            config,
            theme: Theme::default(),
            font: BUNDLED_FONT,
            snap_windows: false,
            requested_at: Instant::now(),
            settings_path: None,
        })
    });
    let Some(hwnd) = find_own_overlay(Duration::from_secs(10)) else {
        match overlay.join().expect("overlay thread") {
            Err(OverlayError::Capture(error)) => {
                println!("skipped: the screen cannot be grabbed here ({error})");
                return None;
            }
            other => panic!("no overlay window; run_overlay returned {:?}", other.err()),
        }
    };
    // Once the window shows, Windows sends it a mouse move at the real
    // cursor, with no button held; arriving in the middle of a posted drag
    // it would end the drag there. Let it come first.
    let shown = Instant::now();
    // SAFETY: a read-only query on a window of this process.
    while unsafe { IsWindowVisible(hwnd) } == 0 && shown.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(10));
    }
    std::thread::sleep(Duration::from_millis(500));
    Some((hwnd, scale, size, overlay))
}

fn finish(overlay: std::thread::JoinHandle<Result<Finished, OverlayError>>) -> Finished {
    match overlay.join().expect("overlay thread") {
        Ok(finished) => finished,
        Err(error) => {
            if let Some(hwnd) = find_own_overlay(Duration::from_millis(100)) {
                key(hwnd, VK_ESCAPE);
            }
            panic!("overlay failed: {error}");
        }
    }
}

/// A red box, then an inverting box over half of it; the panel's Move down
/// puts the red box on top, so the export shows red where the two overlap
/// (the inverted red would be cyan).
#[test]
#[ignore = "live: shows the capture overlay"]
fn side_panel_move_down_reorders_the_export() {
    if !desktop_is_unlocked() {
        println!("skipped: the session is locked or another desktop has the input");
        return;
    }
    let config = OverlayConfig {
        show_help: false,
        has_tasks: true,
        ..OverlayConfig::default()
    };
    let Some((hwnd, scale, (width, height), overlay)) = open(config) else {
        return;
    };
    let physical = |logical: i32| (f64::from(logical) * scale).round() as i32;
    let point = |x: i32, y: i32| (physical(x), physical(y));
    drag(hwnd, point(300, 100), point(800, 500));
    key(hwnd, u16::from(b'R'));
    drag(hwnd, point(400, 200), point(600, 300));
    key(hwnd, u16::from(b'I'));
    drag(hwnd, point(500, 250), point(700, 350));
    // Off with the tool; open the panel: rows <Empty>, red box, invert
    key(hwnd, VK_ESCAPE);
    key(hwnd, VK_SPACE);
    let layout = side_panel_layout(LogicalRect::new(0, 0, width, height), 3);
    assert!(
        layout.rows.len() >= 2,
        "the monitor is too short for the list"
    );
    let center = |rect: LogicalRect| point(rect.x + rect.width / 2, rect.y + rect.height / 2);
    click(hwnd, center(layout.rows[1]));
    click(hwnd, center(layout.move_down));
    key(hwnd, VK_SPACE);
    key(hwnd, VK_RETURN);
    let finished = finish(overlay);
    let Outcome::Export(done) = finished.outcome else {
        panic!("not exported: {:?}", finished.outcome);
    };
    let (image, _) = finished.capture.expect("pixels");
    let pixel = |x: i32, y: i32| {
        // Logical (x, y) on the monitor, into the exported region
        let (px, py) = (physical(x) - done.region.x, physical(y) - done.region.y);
        let at = ((py as u32 * image.width + px as u32) * 4) as usize;
        [image.bgra[at], image.bgra[at + 1], image.bgra[at + 2]]
    };
    let overlap = pixel(550, 275);
    let red_only = pixel(450, 225);
    println!("overlap {overlap:?}, red alone {red_only:?}");
    assert_eq!(red_only, [0, 0, 255], "no red box");
    assert_eq!(
        overlap,
        [0, 0, 255],
        "the red box is not on top after Move down"
    );
}

/// The mode bar's Video segment, clicked: the overlay closes with
/// `RecordVideo` and exports nothing.
#[test]
#[ignore = "live: shows the capture overlay"]
fn mode_bar_video_closes_the_overlay_for_snipping_tool() {
    if !desktop_is_unlocked() {
        println!("skipped: the session is locked or another desktop has the input");
        return;
    }
    let config = OverlayConfig {
        show_help: false,
        video_mode: true,
        ..OverlayConfig::default()
    };
    let Some((hwnd, scale, (width, height), overlay)) = open(config) else {
        return;
    };
    let physical = |logical: i32| (f64::from(logical) * scale).round() as i32;
    let video = mode_bar_layout(LogicalRect::new(0, 0, width, height)).video;
    click(
        hwnd,
        (
            physical(video.x + video.width / 2),
            physical(video.y + video.height / 2),
        ),
    );
    let finished = finish(overlay);
    assert_eq!(finished.outcome, Outcome::RecordVideo);
    assert!(finished.capture.is_none(), "video exported a capture");
    assert!(find_own_overlay(Duration::from_millis(200)).is_none());
}

/// `fontFamily`'s lookup through GDI: an installed family gives its faces
/// as the font files' bytes, a missing one gives none.
#[test]
#[ignore = "live: reads the system's fonts"]
fn system_fonts_are_found_by_family() {
    let families = system_font_families();
    println!(
        "{} families, e.g. {:?}",
        families.len(),
        &families[..families.len().min(8)]
    );
    assert!(!families.is_empty());
    assert!(families.iter().all(|family| !family.starts_with('@')));
    let mut sorted = families.clone();
    sorted.sort_by_key(|family| family.to_lowercase());
    assert_eq!(families, sorted, "not sorted");
    let installed = ["Segoe UI", "Arial", "Calibri", "Consolas"]
        .into_iter()
        .find(|family| families.iter().any(|f| f.eq_ignore_ascii_case(family)))
        .expect("none of the usual Windows families is installed");
    let faces = system_font_faces(installed);
    for face in &faces {
        println!(
            "{installed}: bold {} italic {} index {} ({} bytes)",
            face.bold,
            face.italic,
            face.index,
            face.data.len()
        );
    }
    assert!(!faces.is_empty(), "no faces for {installed}");
    assert!(
        faces.iter().any(|face| !face.bold && !face.italic),
        "no regular face"
    );
    let mut library = phramer_annotate::FontLibrary::new();
    for face in faces {
        library
            .add_face(installed, face.bold, face.italic, face.data, face.index)
            .expect("the engine reads the face");
    }
    assert!(library.has_family(installed));
    assert!(system_font_faces("No Such Font Family 16").is_empty());
}
