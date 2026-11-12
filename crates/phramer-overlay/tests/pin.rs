//! A live smoke test of pins: two at once over the primary monitor, a
//! wheel notch zooms one, Esc closes them. Ignored by default (it shows
//! windows); skipped when the session is locked.
//!
//! ```text
//! cargo test -p phramer-overlay --test pin -- --ignored --nocapture
//! ```

#![cfg(windows)]

use std::time::{Duration, Instant};

use phramer_capture::{PhysicalRect, enumerate_monitors};
use phramer_overlay::{CapturedImage, PinOptions, open_pins, spawn_pin};
use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT};
use windows_sys::Win32::System::StationsAndDesktops::{
    CloseDesktop, DESKTOP_READOBJECTS, GetUserObjectInformationW, OpenInputDesktop, UOI_NAME,
};
use windows_sys::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GWL_EXSTYLE, GetClassNameW, GetWindowLongW, GetWindowRect, IsWindowVisible,
    PostMessageW, WM_KEYDOWN, WM_MOUSEWHEEL, WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
};

const PIN_CLASS: &str = "PhramerPin";

/// Whether the interactive desktop is the user's, not the lock screen.
fn desktop_is_unlocked() -> bool {
    // SAFETY: opens the input desktop for reading its name; closed below.
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

unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> i32 {
    // SAFETY: `lparam` is the `&mut Vec` passed to EnumWindows below.
    let found = unsafe { &mut *(lparam as *mut Vec<isize>) };
    found.push(hwnd as isize);
    1
}

/// Visible pin windows: handle, rectangle, extended style.
fn pins() -> Vec<(isize, RECT, u32)> {
    let mut all: Vec<isize> = Vec::new();
    // SAFETY: `collect` only pushes into `all`, which outlives the call.
    unsafe { EnumWindows(Some(collect), &mut all as *mut Vec<isize> as LPARAM) };
    all.into_iter()
        .filter_map(|raw| {
            let hwnd = raw as HWND;
            let mut class = [0u16; 64];
            // SAFETY: `class` holds 64 characters.
            let length = unsafe { GetClassNameW(hwnd, class.as_mut_ptr(), 64) };
            let class = String::from_utf16_lossy(&class[..length.max(0) as usize]);
            // SAFETY: a read-only query on a window handle.
            if class != PIN_CLASS || unsafe { IsWindowVisible(hwnd) } == 0 {
                return None;
            }
            let mut rect = RECT::default();
            // SAFETY: `rect` is a valid out-parameter.
            unsafe { GetWindowRect(hwnd, &mut rect) };
            // SAFETY: a read-only query on a window handle.
            let style = unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) } as u32;
            Some((raw, rect, style))
        })
        .collect()
}

fn wait_until(timeout: Duration, mut done: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if done() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    done()
}

fn image(width: u32, height: u32) -> CapturedImage {
    CapturedImage {
        width,
        height,
        bgra: [40, 160, 220, 255].repeat((width * height) as usize),
    }
}

#[test]
#[ignore = "live: shows pinned windows"]
fn pins_show_zoom_and_close() {
    if !desktop_is_unlocked() {
        println!("skipped: the session is locked or another desktop has the input");
        return;
    }
    // SAFETY: a constant context; failing only means it was already set.
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    let monitors = enumerate_monitors().expect("monitors");
    let primary = monitors
        .iter()
        .find(|monitor| monitor.is_primary())
        .expect("a primary monitor");
    let bounds = primary.bounds();
    let margin = (8.0 * primary.scale()).round() as i32;
    let options = PinOptions {
        accent: (255, 0, 0),
        hover_accent: (0, 0, 255),
        smooth: true,
    };
    let at = PhysicalRect::new(bounds.x + 100, bounds.y + 100, 200, 100);
    let other = PhysicalRect::new(bounds.x + 400, bounds.y + 300, 120, 80);
    spawn_pin(image(200, 100), at, options, Box::new(|_, _| {})).expect("first pin");
    spawn_pin(image(120, 80), other, options, Box::new(|_, _| {})).expect("second pin");
    assert!(
        wait_until(Duration::from_secs(5), || pins().len() == 2),
        "two pins expected: {:?}",
        pins().len()
    );
    assert_eq!(open_pins(), 2);

    let shown = pins();
    let (first, rect, style) = *shown
        .iter()
        .find(|(_, rect, _)| rect.left == at.x - margin)
        .expect("the first pin at its capture position");
    println!(
        "pin {first:#x} at ({}, {})-({}, {}), style {style:#x}",
        rect.left, rect.top, rect.right, rect.bottom
    );
    assert_eq!(
        (rect.left, rect.top, rect.right, rect.bottom),
        (
            at.x - margin,
            at.y - margin,
            at.x + 200 + margin,
            at.y + 100 + margin
        ),
        "the image sits pixel for pixel over where it was captured"
    );
    assert_ne!(style & WS_EX_TOPMOST, 0, "not topmost");
    assert_ne!(style & WS_EX_LAYERED, 0, "not layered");
    assert_ne!(style & WS_EX_TOOLWINDOW, 0, "would show in the taskbar");

    // One wheel notch up: 3 % of the original size
    let wheel = (120usize) << 16;
    // SAFETY: posting to a window handle; a stale one just fails.
    unsafe { PostMessageW(first as HWND, WM_MOUSEWHEEL, wheel, 0) };
    assert!(
        wait_until(Duration::from_secs(5), || {
            pins()
                .iter()
                .any(|(hwnd, rect, _)| *hwnd == first && rect.right - rect.left == 206 + 2 * margin)
        }),
        "the wheel did not zoom the pin"
    );

    for (hwnd, _, _) in pins() {
        // SAFETY: posting to a window handle; a stale one just fails.
        unsafe { PostMessageW(hwnd as HWND, WM_KEYDOWN, usize::from(VK_ESCAPE), 0) };
    }
    assert!(
        wait_until(Duration::from_secs(5), || pins().is_empty()
            && open_pins() == 0),
        "Esc left pins open"
    );
}
