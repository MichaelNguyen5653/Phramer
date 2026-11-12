//! Errors of the overlay and the export.

use std::path::PathBuf;

/// Why the overlay could not run or the capture could not be exported.
#[derive(Debug, thiserror::Error)]
pub enum OverlayError {
    #[error("there is no monitor {0}")]
    NoSuchMonitor(usize),
    #[error("the frame is {actual} bytes; {width}x{height} needs {expected}")]
    FrameSize {
        width: u32,
        height: u32,
        expected: usize,
        actual: usize,
    },
    #[error("cannot draw on a {width}x{height} canvas")]
    Canvas { width: u32, height: u32 },
    #[error("the overlay font cannot be read: {0}")]
    Font(String),
    #[error("cannot encode the image: {0}")]
    Encode(String),
    #[error("cannot write {path}: {source}")]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("no folder to save to")]
    NoSaveFolder,
    #[error("{call} failed (code {code:#010x})")]
    Win32 { call: &'static str, code: u32 },
    #[error(transparent)]
    Capture(#[from] phramer_capture::CaptureError),
    #[error("the overlay stopped after a panic")]
    Panicked,
}
