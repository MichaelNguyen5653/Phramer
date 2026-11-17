//! A live smoke test of pins: two at once over the primary monitor, a
//! wheel notch zooms one, Esc closes them. Ignored by default (it shows
//! windows); skipped when the session is locked.
//!
//! ```text
//! cargo test -p phramer-overlay --test pin -- --ignored --nocapture
//! ```

#![cfg(windows)]

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use phramer_capture::{PhysicalRect, enumerate_monitors, grab_monitor};
use phramer_overlay::{CapturedImage, PinOptions, PinRequest, open_pins, spawn_pin};
use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT};
use windows_sys::Win32::System::StationsAndDesktops::{
    CloseDesktop, DESKTOP_READOBJECTS, GetUserObjectInformationW, OpenInputDesktop, UOI_NAME,
};
use windows_sys::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput, VK_DOWN, VK_ESCAPE,
    VK_RETURN,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowW, GWL_EXSTYLE, GetClassNameW, GetWindowLongW, GetWindowRect,
    IsWindowVisible, PostMessageW, WM_CONTEXTMENU, WM_KEYDOWN, WM_MOUSEWHEEL, WS_EX_LAYERED,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
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

/// A popup menu is open (its window class is `#32768`).
fn menu_open() -> bool {
    let class: Vec<u16> = "#32768".encode_utf16().chain(Some(0)).collect();
    // SAFETY: a NUL-terminated class name; no title.
    let menu = unsafe { FindWindowW(class.as_ptr(), std::ptr::null()) };
    // SAFETY: a read-only query; a null handle just gives 0.
    !menu.is_null() && unsafe { IsWindowVisible(menu) } != 0
}

fn press(key: u16) {
    let input = |flags| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let inputs = [input(0), input(KEYEVENTF_KEYUP)];
    // SAFETY: two initialised keyboard inputs of the size given.
    unsafe { SendInput(2, inputs.as_ptr(), size_of::<INPUT>() as i32) };
    std::thread::sleep(Duration::from_millis(50));
}

/// Picks the `n`th entry (from 1, separators not counted) of the pin's
/// context menu with the keyboard, as Shift+F10 then the arrows would.
fn choose_from_menu(pin: isize, n: usize) {
    // -1, -1: from the keyboard, at the cursor
    // SAFETY: posting to a window handle; a stale one just fails.
    unsafe { PostMessageW(pin as HWND, WM_CONTEXTMENU, pin as usize, -1) };
    assert!(
        wait_until(Duration::from_secs(5), menu_open),
        "the context menu did not open"
    );
    for _ in 0..n {
        press(VK_DOWN);
    }
    press(VK_RETURN);
    assert!(
        wait_until(Duration::from_secs(5), || !menu_open()),
        "the context menu stayed open"
    );
}

/// The colour on screen at a physical point of the primary monitor.
fn screen_pixel(monitor: &phramer_capture::Monitor, x: i32, y: i32) -> [u8; 3] {
    let frame = grab_monitor(monitor).expect("grab");
    let bounds = frame.bounds();
    let at = (((y - bounds.y) * bounds.width + (x - bounds.x)) * 4) as usize;
    let bgra = frame.bgra();
    [bgra[at], bgra[at + 1], bgra[at + 2]]
}

#[test]
#[ignore = "live: shows a pin and drives its context menu"]
fn pin_menu_changes_opacity_and_asks_for_the_editor() {
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
    let at = PhysicalRect::new(bounds.x + 100, bounds.y + 100, 200, 100);
    let (x, y) = (at.x + 100, at.y + 50);
    let below = screen_pixel(primary, x, y);
    let pin_colour = [40u8, 160, 220];

    let requests = Arc::new(Mutex::new(Vec::new()));
    let heard = Arc::clone(&requests);
    let options = PinOptions {
        accent: (255, 0, 0),
        hover_accent: (0, 0, 255),
        smooth: true,
    };
    spawn_pin(
        image(200, 100),
        at,
        options,
        Box::new(move |request, _| heard.lock().unwrap().push(request)),
    )
    .expect("pin");
    assert!(
        wait_until(Duration::from_secs(5), || pins().len() == 1),
        "no pin"
    );
    let (pin, _, _) = pins()[0];
    std::thread::sleep(Duration::from_millis(300));
    let shown = screen_pixel(primary, x, y);
    println!("below {below:?}, pin {shown:?}");
    assert_eq!(shown, pin_colour, "the pin is not opaque at first");

    // Entries: Copy, Save, Load into editor, Rotate Right, Rotate Left,
    // Increase Opacity, Decrease Opacity, Close
    choose_from_menu(pin, 7);
    // 90 %: SourceConstantAlpha 230 over what is below
    let expected: Vec<i32> = (0..3)
        .map(|c| (i32::from(pin_colour[c]) * 230 + i32::from(below[c]) * 25 + 127) / 255)
        .collect();
    let mut faded = [0u8; 3];
    let matched = wait_until(Duration::from_secs(3), || {
        faded = screen_pixel(primary, x, y);
        (0..3).all(|c| (i32::from(faded[c]) - expected[c]).abs() <= 3)
    });
    println!("Decrease Opacity: {faded:?}, expected {expected:?}");
    if below
        .iter()
        .zip(pin_colour)
        .any(|(b, p)| b.abs_diff(p) > 30)
    {
        assert!(matched, "Decrease Opacity did not fade the pin");
    } else {
        println!("the desktop below is too close to the pin's colour to see the fade");
    }

    choose_from_menu(pin, 6);
    let mut back = [0u8; 3];
    assert!(
        wait_until(Duration::from_secs(3), || {
            back = screen_pixel(primary, x, y);
            back == pin_colour
        }),
        "Increase Opacity did not bring the pin back: {back:?}"
    );

    choose_from_menu(pin, 3);
    assert!(
        wait_until(Duration::from_secs(3), || requests
            .lock()
            .unwrap()
            .contains(&PinRequest::OpenInEditor)),
        "Load into editor asked for nothing: {:?}",
        requests.lock().unwrap()
    );

    // SAFETY: posting to a window handle; a stale one just fails.
    unsafe { PostMessageW(pin as HWND, WM_KEYDOWN, usize::from(VK_ESCAPE), 0) };
    assert!(
        wait_until(Duration::from_secs(5), || pins().is_empty()
            && open_pins() == 0),
        "Esc left the pin open"
    );
}
