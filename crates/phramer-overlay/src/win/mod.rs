//! Everything that talks to Win32: the overlay windows, the pins, the
//! clipboard and Show in Folder. All `unsafe` of the crate lives here.

mod clipboard;
mod folder;
mod pin;
mod window;

pub use clipboard::SystemClipboard;
pub use folder::show_in_folder;
pub use pin::{PinHandler, PinOptions, open_pins, spawn_pin};
pub use window::{Finished, OverlaySession, run_overlay};

use windows_sys::Win32::Foundation::GetLastError;

use crate::error::OverlayError;

/// The error for a Win32 call that just failed on this thread.
fn last_error(call: &'static str) -> OverlayError {
    // SAFETY: GetLastError only reads the calling thread's last-error value.
    let code = unsafe { GetLastError() };
    OverlayError::Win32 { call, code }
}
