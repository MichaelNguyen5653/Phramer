//! Errors of the capture crate.

/// Why a monitor query, window query or grab failed.
#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    /// A Win32 call failed. `code` is `GetLastError()` or the `HRESULT`.
    #[error("{call} failed (code {code:#010x})")]
    Win32 { call: &'static str, code: u32 },

    /// The process is not per-monitor DPI aware (v2), so Windows would hand
    /// back virtualised coordinates and scaled pixels.
    #[error(
        "process is not per-monitor DPI aware (v2); call enable_per_monitor_dpi_awareness() first"
    )]
    NotDpiAware,

    /// No monitor was reported, which means there is no interactive desktop.
    #[error("no monitors found; is there an interactive desktop session?")]
    NoMonitors,

    /// The area to grab is empty or does not fit a pixel buffer.
    #[error("cannot grab an area of {width}x{height} pixels")]
    InvalidSize { width: i32, height: i32 },
}
