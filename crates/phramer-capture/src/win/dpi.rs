//! Per-monitor DPI awareness, the precondition of every other module.

use windows_sys::Win32::UI::HiDpi::{
    AreDpiAwarenessContextsEqual, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    GetThreadDpiAwarenessContext, SetProcessDpiAwarenessContext,
};

use super::last_error;
use crate::error::CaptureError;

/// Makes the process per-monitor DPI aware (v2).
///
/// **The binary must call this first**, before it creates a window and
/// before any other function of this crate. Without it Windows virtualises
/// coordinates: monitor bounds, the cursor position and window rectangles
/// come back scaled to the primary monitor's DPI, and a grab of a scaled
/// monitor is stretched. The awareness of a process can be set only once, so
/// this cannot be deferred until a capture starts.
///
/// Succeeds when the process is already per-monitor aware v2 (set by an
/// earlier call, by the application manifest or by the GUI toolkit). Fails
/// when it was already fixed at a lower awareness.
pub fn enable_per_monitor_dpi_awareness() -> Result<(), CaptureError> {
    // SAFETY: the argument is one of the constant context handles the API
    // defines; the call has no other preconditions.
    if unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) } != 0 {
        return Ok(());
    }
    // Access denied here means the awareness was already set; that is fine
    // if it was set to what is needed
    let error = last_error("SetProcessDpiAwarenessContext");
    if is_per_monitor_v2() {
        Ok(())
    } else {
        Err(error)
    }
}

/// Fails unless the calling thread runs per-monitor DPI aware (v2).
pub(super) fn require_per_monitor_v2() -> Result<(), CaptureError> {
    if is_per_monitor_v2() {
        Ok(())
    } else {
        Err(CaptureError::NotDpiAware)
    }
}

fn is_per_monitor_v2() -> bool {
    // SAFETY: GetThreadDpiAwarenessContext has no preconditions, and both
    // handles passed to the comparison are valid: one was just returned by
    // the system, the other is an API constant.
    unsafe {
        AreDpiAwarenessContextsEqual(
            GetThreadDpiAwarenessContext(),
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        ) != 0
    }
}
