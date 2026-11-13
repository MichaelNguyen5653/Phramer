//! A live smoke test of the overlay windows: opens the overlay on the
//! primary monitor, draws a rectangle with posted mouse and key messages,
//! accepts, and exports to a PNG. Ignored by default (it shows a fullscreen
//! window); skipped when the session is locked, when the screen cannot be
//! grabbed and nothing could be shown anyway.
//!
//! ```text
//! cargo test -p phramer-overlay --test smoke -- --ignored --nocapture
//! ```

#![cfg(windows)]

use std::path::Path;
use std::time::{Duration, Instant};

use phramer_capture::{LocalRect, LogicalRect, enumerate_monitors};
use phramer_core::config::Settings;
use phramer_overlay::{
    BUNDLED_FONT, CaptureTasks, CapturedImage, ExportAction, ExportContext, Handoff, LocalTime,
    Outcome, OverlayConfig, OverlayError, OverlaySession, SaveTarget, Theme, export, run_overlay,
    side_panel_layout,
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
    FindWindowExW, FindWindowW, GetWindowThreadProcessId, PostMessageW, WM_KEYDOWN, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MOUSEMOVE,
};

/// The overlay's window class.
const OVERLAY_CLASS: &str = "PhramerCaptureOverlay";

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

/// Whether the interactive desktop is the user's, not the lock screen or
/// a UAC prompt: only then can a window be shown and the screen grabbed.
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

fn find_overlay(timeout: Duration) -> Option<HWND> {
    let class = wide(OVERLAY_CLASS);
    let started = Instant::now();
    while started.elapsed() < timeout {
        // SAFETY: a lookup by class name; the string outlives the call.
        let hwnd = unsafe { FindWindowW(class.as_ptr(), std::ptr::null()) };
        if !hwnd.is_null() {
            return Some(hwnd);
        }
        std::thread::sleep(Duration::from_millis(20));
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
}

fn key(hwnd: HWND, vk: u16) {
    post(hwnd, WM_KEYDOWN, WPARAM::from(vk), 1);
}

/// Hands nothing to the clipboard.
struct NoClipboard;

impl Handoff for NoClipboard {
    fn copy_image(&mut self, _: &CapturedImage, _: Option<u8>) -> Result<(), OverlayError> {
        Ok(())
    }

    fn copy_text(&mut self, _: &str) -> Result<(), OverlayError> {
        Ok(())
    }

    fn copy_file(&mut self, _: &Path) -> Result<(), OverlayError> {
        Ok(())
    }
}

#[test]
#[ignore = "live: shows the capture overlay"]
fn draw_a_rectangle_and_export() {
    if !desktop_is_unlocked() {
        println!("skipped: the session is locked or another desktop has the input");
        return;
    }
    // SAFETY: sets this test process's DPI awareness before any window.
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    let monitors = enumerate_monitors().expect("monitors");
    let active = monitors
        .iter()
        .position(|monitor| monitor.is_primary())
        .unwrap_or(0);
    let config = OverlayConfig {
        show_help: false,
        // Enter accepts, as with `gui -p`
        has_tasks: true,
        ..OverlayConfig::default()
    };
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
    let Some(hwnd) = find_overlay(Duration::from_secs(10)) else {
        let result = overlay.join().expect("overlay thread");
        match result {
            Err(OverlayError::Capture(error)) => {
                println!("skipped: the screen cannot be grabbed here ({error})");
                return;
            }
            other => panic!("no overlay window; run_overlay returned {:?}", other.err()),
        }
    };
    // Select, pick the rectangle tool (R), draw inside, accept (Enter)
    drag(hwnd, (100, 100), (600, 400));
    key(hwnd, u16::from(b'R'));
    drag(hwnd, (200, 200), (400, 300));
    key(hwnd, VK_RETURN);
    let finished = match overlay.join().expect("overlay thread") {
        Ok(finished) => finished,
        Err(error) => {
            // Make sure no window is left behind on an unexpected path
            if let Some(hwnd) = find_overlay(Duration::from_millis(100)) {
                key(hwnd, VK_ESCAPE);
            }
            panic!("overlay failed: {error}");
        }
    };
    let Outcome::Export(done) = finished.outcome else {
        panic!("not exported: {:?}", finished.outcome);
    };
    assert_eq!(done.action, ExportAction::Accept);
    // Rounded through the monitor's scale: within a pixel or two
    let close = |a: i32, b: i32| (a - b).abs() <= 2;
    let LocalRect {
        x,
        y,
        width,
        height,
    } = done.region;
    assert!(
        close(x, 100) && close(y, 100) && close(width, 500) && close(height, 300),
        "{:?}",
        done.region
    );
    let (image, geometry) = finished.capture.expect("pixels");
    // The rectangle (filled, in the default red) covers the image point
    // (300, 250), which is (200, 150) into the selection
    let pixel = ((150 * image.width + 200) * 4) as usize;
    assert_eq!(
        &image.bgra[pixel..pixel + 3],
        [0, 0, 255],
        "no red rectangle in the export"
    );

    let dir = std::env::temp_dir().join(format!("phramer-overlay-smoke-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp folder");
    let file = dir.join("smoke.png");
    let settings = Settings::default();
    let time = LocalTime::now();
    let mut handoff = NoClipboard;
    let mut context = ExportContext {
        settings: &settings,
        time: &time,
        handoff: &mut handoff,
    };
    let report = export(
        &image,
        geometry,
        ExportAction::Accept,
        &CaptureTasks {
            save: Some(SaveTarget::Path(file.clone())),
            ..CaptureTasks::default()
        },
        &mut context,
    );
    assert_eq!(report.saved.as_deref(), Some(file.as_path()), "{report:?}");
    let bytes = std::fs::read(&file).expect("saved png");
    assert_eq!(&bytes[1..4], b"PNG");
    std::fs::remove_dir_all(&dir).expect("temp folder removed");
    println!("drew, accepted and saved {}x{}", image.width, image.height);
}

/// The daemon's log, as far as the overlay writes it.
struct Lines(std::sync::Mutex<Vec<String>>);

impl log::Log for Lines {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &log::Record<'_>) {
        if let Ok(mut lines) = self.0.lock() {
            lines.push(record.args().to_string());
        }
    }

    fn flush(&self) {}
}

static LINES: Lines = Lines(std::sync::Mutex::new(Vec::new()));

/// The milliseconds after `marker` in the last line that has it.
fn logged_ms(marker: &str) -> Option<u128> {
    let lines = LINES.0.lock().ok()?;
    lines
        .iter()
        .rev()
        .filter_map(|line| line.split(marker).nth(1))
        .find_map(|rest| rest.split(' ').next()?.parse().ok())
}

/// An overlay window of this process (another process may show one too).
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
        std::thread::sleep(Duration::from_millis(1));
    }
    None
}

/// The toolbar of a selection made as soon as the overlay shows, while it
/// follows the cursor over every monitor (`captureRegionMode` 2): on screen
/// within the 150 ms feedback rule of `docs/design/raycast-notes.md`, timed
/// from the request, however many monitors are still to be opened. Run
/// with `--release`, as the daemon is built.
#[test]
#[ignore = "live: shows the capture overlay on every monitor"]
fn toolbar_follows_a_selection_while_the_monitors_open() {
    const TOOLBAR_BUDGET_MS: u128 = 150;
    // SAFETY: sets this test process's DPI awareness before any window.
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    // Another test of this binary may have set it first
    let _ = log::set_logger(&LINES);
    log::set_max_level(log::LevelFilter::Info);
    let monitors = enumerate_monitors().expect("monitors");
    let active = monitors
        .iter()
        .position(|monitor| monitor.is_primary())
        .unwrap_or(0);
    for monitor in &monitors {
        println!("monitor {:?} at {}x", monitor.bounds(), monitor.scale());
    }
    let config = OverlayConfig {
        show_help: false,
        has_tasks: true,
        follow_cursor: true,
        ..OverlayConfig::default()
    };
    let requested_at = Instant::now();
    let overlay = std::thread::spawn(move || {
        run_overlay(OverlaySession {
            monitors,
            active,
            config,
            theme: Theme::default(),
            font: BUNDLED_FONT,
            snap_windows: true,
            requested_at,
            settings_path: None,
        })
    });
    let Some(hwnd) = find_own_overlay(Duration::from_secs(10)) else {
        match overlay.join().expect("overlay thread") {
            Err(OverlayError::Capture(error)) => {
                println!("skipped: the screen cannot be grabbed here ({error})");
                return;
            }
            other => panic!("no overlay window; run_overlay returned {:?}", other.err()),
        }
    };
    // Posted at once, without the pause `drag` makes between messages
    for (message, wparam, point) in [
        (WM_MOUSEMOVE, 0, (200, 200)),
        (WM_LBUTTONDOWN, MK_LBUTTON as WPARAM, (200, 200)),
        (WM_MOUSEMOVE, MK_LBUTTON as WPARAM, (600, 500)),
        (WM_LBUTTONUP, 0, (600, 500)),
    ] {
        // SAFETY: posts a plain message to a window of this process.
        unsafe { PostMessageW(hwnd, message, wparam, at(point.0, point.1)) };
    }
    let started = Instant::now();
    while logged_ms("toolbar on screen ").is_none() && started.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(10));
    }
    key(hwnd, VK_ESCAPE);
    let finished = overlay.join().expect("overlay thread").expect("overlay");
    assert_eq!(finished.outcome, Outcome::Cancelled);
    for line in LINES
        .0
        .lock()
        .map(|lines| lines.clone())
        .unwrap_or_default()
    {
        if line.starts_with("overlay:") {
            println!("{line}");
        }
    }
    let toolbar = logged_ms("toolbar on screen ").expect("no toolbar was painted");
    assert!(
        toolbar < TOOLBAR_BUDGET_MS,
        "the toolbar took {toolbar} ms (budget {TOOLBAR_BUDGET_MS} ms)"
    );
}

/// The side panel and colour grab, live: Space opens the panel, its Grab
/// Color button starts grabbing, a click picks a screenshot pixel, which
/// becomes the draw colour (saved to the settings file) and fills a
/// rectangle drawn after. Only messages posted to the overlay's own window;
/// no real input.
#[test]
#[ignore = "live: shows the capture overlay"]
fn side_panel_and_colour_grab() {
    if !desktop_is_unlocked() {
        println!("skipped: the session is locked or another desktop has the input");
        return;
    }
    // SAFETY: sets this test process's DPI awareness before any window.
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    let monitors = enumerate_monitors().expect("monitors");
    let active = monitors
        .iter()
        .position(|monitor| monitor.is_primary())
        .unwrap_or(0);
    let monitor = monitors[active].clone();
    let scale = monitor.scale();
    let (logical_width, logical_height) = monitor.logical_size();
    let physical = |logical: i32| (f64::from(logical) * scale).round() as i32;
    let dir = std::env::temp_dir().join(format!("phramer-overlay-grab-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp folder");
    let settings_path = dir.join("phramer.ini");
    Settings::default().save(&settings_path).expect("settings");
    let config = OverlayConfig {
        show_help: false,
        has_tasks: true,
        ..OverlayConfig::default()
    };
    let settings_for_overlay = settings_path.clone();
    let overlay = std::thread::spawn(move || {
        run_overlay(OverlaySession {
            monitors,
            active,
            config,
            theme: Theme::default(),
            font: BUNDLED_FONT,
            snap_windows: false,
            requested_at: Instant::now(),
            settings_path: Some(settings_for_overlay),
        })
    });
    let Some(hwnd) = find_own_overlay(Duration::from_secs(10)) else {
        let result = overlay.join().expect("overlay thread");
        match result {
            Err(OverlayError::Capture(error)) => {
                println!("skipped: the screen cannot be grabbed here ({error})");
                return;
            }
            other => panic!("no overlay window; run_overlay returned {:?}", other.err()),
        }
    };
    // A selection, then the panel (Space) and its Grab Color button
    drag(hwnd, (100, 100), (600, 400));
    key(hwnd, VK_SPACE);
    let layout = side_panel_layout(LogicalRect::new(0, 0, logical_width, logical_height), 1);
    let grab = (
        physical(layout.grab.x + layout.grab.width / 2),
        physical(layout.grab.y + layout.grab.height / 2),
    );
    post(hwnd, WM_MOUSEMOVE, 0, at(grab.0, grab.1));
    post(
        hwnd,
        WM_LBUTTONDOWN,
        MK_LBUTTON as WPARAM,
        at(grab.0, grab.1),
    );
    post(hwnd, WM_LBUTTONUP, 0, at(grab.0, grab.1));
    // Pick the pixel at logical (150, 150), inside the selection: a point
    // that maps to whole physical pixels both ways at any scale
    let pick = (physical(150), physical(150));
    post(hwnd, WM_MOUSEMOVE, 0, at(pick.0, pick.1));
    post(
        hwnd,
        WM_LBUTTONDOWN,
        MK_LBUTTON as WPARAM,
        at(pick.0, pick.1),
    );
    post(hwnd, WM_LBUTTONUP, 0, at(pick.0, pick.1));
    // The panel came back: Space hides it; then a rectangle in the picked
    // colour, away from the picked pixel, and Enter
    key(hwnd, VK_SPACE);
    key(hwnd, u16::from(b'R'));
    drag(
        hwnd,
        (physical(200), physical(140)),
        (physical(260), physical(180)),
    );
    key(hwnd, VK_RETURN);
    let finished = match overlay.join().expect("overlay thread") {
        Ok(finished) => finished,
        Err(error) => {
            if let Some(hwnd) = find_own_overlay(Duration::from_millis(100)) {
                key(hwnd, VK_ESCAPE);
            }
            panic!("overlay failed: {error}");
        }
    };
    let Outcome::Export(done) = finished.outcome else {
        panic!("not exported: {:?}", finished.outcome);
    };
    let (image, _) = finished.capture.expect("pixels");
    let pixel = |x: i32, y: i32| {
        let at =
            (((y - done.region.y) as u32 * image.width + (x - done.region.x) as u32) * 4) as usize;
        [image.bgra[at + 2], image.bgra[at + 1], image.bgra[at]]
    };
    // The screenshot's pixel where the click was, and the rectangle's fill
    let picked = pixel(pick.0, pick.1);
    let filled = pixel(physical(230), physical(160));
    let saved = Settings::load(&settings_path)
        .expect("settings")
        .draw_color();
    println!(
        "monitor at {scale}x: picked #{:02x}{:02x}{:02x} at {pick:?}; rectangle #{:02x}{:02x}{:02x}; \
         saved drawColor #{:02x}{:02x}{:02x}",
        picked[0], picked[1], picked[2], filled[0], filled[1], filled[2], saved.r, saved.g, saved.b
    );
    std::fs::remove_dir_all(&dir).expect("temp folder removed");
    assert_eq!(filled, picked, "the rectangle is not in the picked colour");
    assert_eq!(
        [saved.r, saved.g, saved.b],
        picked,
        "drawColor was not saved"
    );
}
