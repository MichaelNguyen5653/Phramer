//! Everything that talks to Win32. All `unsafe` of the crate lives here.

mod dpi;
mod grab;
mod monitors;
mod windows;

pub use dpi::enable_per_monitor_dpi_awareness;
pub use grab::{grab_monitor, grab_virtual_desktop};
pub use monitors::{cursor_position, enumerate_monitors};
pub use windows::visible_windows;

use windows_sys::Win32::Foundation::GetLastError;

use crate::error::CaptureError;

/// The error for a Win32 call that just failed on this thread. Call it
/// before anything else can overwrite the thread's last error.
fn last_error(call: &'static str) -> CaptureError {
    // SAFETY: GetLastError only reads the calling thread's last-error value.
    let code = unsafe { GetLastError() };
    CaptureError::Win32 { call, code }
}
