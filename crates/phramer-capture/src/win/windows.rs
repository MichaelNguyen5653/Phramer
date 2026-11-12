//! The window list for window snap.

use std::ffi::c_void;
use std::mem::size_of;

use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT, TRUE};
use windows_sys::Win32::Graphics::Dwm::{
    DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute,
};
use windows_sys::Win32::System::Threading::GetCurrentProcessId;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GWL_EXSTYLE, GetWindowLongW, GetWindowRect, GetWindowThreadProcessId, IsIconic,
    IsWindowVisible, WS_EX_TRANSPARENT,
};
use windows_sys::core::BOOL;

use super::dpi::require_per_monitor_v2;
use super::last_error;
use crate::error::CaptureError;
use crate::geometry::PhysicalRect;

struct Enumeration {
    own_process: u32,
    windows: Vec<PhysicalRect>,
}

/// Visible top-level windows of other processes, topmost first, as their
/// visible frame (without the drop shadow and the invisible resize border).
///
/// Call it once, when the capture starts and before the overlay is shown, so
/// the list matches the frozen screenshot. Minimised, cloaked (other virtual
/// desktop, suspended app) and click-through windows are left out. The
/// desktop itself is a window and comes last.
pub fn visible_windows() -> Result<Vec<PhysicalRect>, CaptureError> {
    require_per_monitor_v2()?;

    // SAFETY: GetCurrentProcessId has no preconditions.
    let own_process = unsafe { GetCurrentProcessId() };
    let mut state = Enumeration {
        own_process,
        windows: Vec::new(),
    };
    // EnumWindows reports top-level windows in z-order, topmost first, which
    // is the order `window_at` relies on
    // SAFETY: `collect_window` matches WNDENUMPROC and treats the LPARAM as
    // the `Enumeration` passed here. The enumeration is synchronous, so
    // `state` outlives every callback and nothing else touches it meanwhile.
    if unsafe { EnumWindows(Some(collect_window), &raw mut state as LPARAM) } == 0 {
        return Err(last_error("EnumWindows"));
    }
    Ok(state.windows)
}

unsafe extern "system" fn collect_window(window: HWND, data: LPARAM) -> BOOL {
    // SAFETY: `data` is the `&mut Enumeration` that `visible_windows` handed
    // to EnumWindows; it is alive and exclusively ours for the whole
    // synchronous enumeration.
    let state = unsafe { &mut *(data as *mut Enumeration) };
    if let Some(frame) = snap_frame(window, state.own_process) {
        state.windows.push(frame);
    }
    TRUE
}

/// The visible frame of `window` if it is one the mouse could pick.
fn snap_frame(window: HWND, own_process: u32) -> Option<PhysicalRect> {
    let mut process = 0u32;
    // SAFETY: `window` is a handle EnumWindows just reported; a window that
    // has closed since only makes these queries fail. `process` is a valid
    // out pointer.
    let pickable = unsafe {
        GetWindowThreadProcessId(window, &raw mut process);
        process != own_process
            && IsWindowVisible(window) != 0
            && IsIconic(window) == 0
            && !is_cloaked(window)
            // Click-through windows (overlays, notification hosts) are
            // invisible to the mouse, so the mouse must not pick them either
            && GetWindowLongW(window, GWL_EXSTYLE) as u32 & WS_EX_TRANSPARENT == 0
    };
    if !pickable {
        return None;
    }

    // The extended frame bounds are the window as drawn. GetWindowRect
    // includes the invisible resize border and the shadow, which would put a
    // band of the window behind into the capture.
    let mut bounds = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: `bounds` is a writable RECT and the size passed is its size,
    // as DWMWA_EXTENDED_FRAME_BOUNDS requires; `window` as above.
    let found = unsafe {
        DwmGetWindowAttribute(
            window,
            DWMWA_EXTENDED_FRAME_BOUNDS as u32,
            (&raw mut bounds).cast::<c_void>(),
            size_of::<RECT>() as u32,
        ) >= 0
            || GetWindowRect(window, &raw mut bounds) != 0
    };
    let frame = PhysicalRect::from_edges(bounds.left, bounds.top, bounds.right, bounds.bottom);
    (found && !frame.is_empty()).then_some(frame)
}

/// Suspended UWP apps and windows on other virtual desktops are cloaked:
/// invisible, yet `IsWindowVisible` still says yes.
fn is_cloaked(window: HWND) -> bool {
    let mut cloaked = 0u32;
    // SAFETY: `cloaked` is a writable 32-bit value and the size passed is
    // its size, as DWMWA_CLOAKED requires. A stale `window` only makes the
    // call fail.
    let result = unsafe {
        DwmGetWindowAttribute(
            window,
            DWMWA_CLOAKED as u32,
            (&raw mut cloaked).cast::<c_void>(),
            size_of::<u32>() as u32,
        )
    };
    result >= 0 && cloaked != 0
}
