//! Where the overlay's other actions send a capture: OCR, the editor and
//! another application (App Launcher). Each runs on the capture's thread
//! and reports through the same notices as an export.
//!
//! OCR and App Launcher open windows of their own (`ocr_ui`, `launcher_ui`)
//! through the daemon's app handle, kept here at start-up ([`set_app`]), so
//! their callers need no handle of their own.

use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use phramer_capture::PhysicalRect;
use phramer_core::config::Settings;
use phramer_ocr::{Rect, RgbaImage};
use phramer_overlay::{
    CapturedImage, ExportReport, ImageFormat, LocalTime, Notice, encode, expand_pattern,
};
use tauri::{AppHandle, Emitter, Runtime};

use crate::{launcher_ui, ocr_ui};

/// The daemon's app handle, for the windows OCR and App Launcher open.
static APP: OnceLock<AppHandle> = OnceLock::new();

/// The event the editor window listens for (`editor::listen`); the payload
/// is the path of a PNG of the capture.
pub(crate) const EDITOR_OPEN_EVENT: &str = "editor:open";
/// Characters Windows refuses in a file name.
const RESERVED: &[char] = &['\\', '/', ':', '*', '?', '"', '<', '>', '|'];

fn notice(text: String, error: bool) -> ExportReport {
    if error {
        log::error!("{text}");
    } else {
        log::info!("{text}");
    }
    ExportReport {
        notices: vec![Notice {
            text,
            path: None,
            error,
        }],
        ..ExportReport::default()
    }
}

/// At start-up: the handle OCR and App Launcher open their windows with.
pub(crate) fn set_app(app: &AppHandle) {
    let _ = APP.set(app.clone());
}

/// The daemon's handle, or the notice that there is none (never seen: it is
/// set before any capture can start).
fn app() -> Result<&'static AppHandle, ExportReport> {
    APP.get()
        .ok_or_else(|| notice("Unable to open the window: Phramer is not ready.".to_owned(), true))
}

/// OCR: the results window for the capture (15.0.0's `OcrResultsWindow`),
/// with no screen position, so recognition only. The text is copied when
/// the user asks, as in 15.0.0.
pub(crate) fn read_text(_settings: &Settings, image: &CapturedImage) -> ExportReport {
    read_text_at(image, None)
}

/// OCR of a capture that sat at `geometry` (physical screen pixels): the
/// exact-text path may read the windows under it.
pub(crate) fn read_text_at(image: &CapturedImage, geometry: Option<PhysicalRect>) -> ExportReport {
    let app = match app() {
        Ok(app) => app,
        Err(report) => return report,
    };
    let rgba = RgbaImage::from_bgra(image.width, image.height, &image.bgra);
    ocr_ui::open(app, rgba, geometry.map(screen_rect));
    ExportReport::default()
}

/// A capture's geometry as the OCR crate's rectangle.
fn screen_rect(geometry: PhysicalRect) -> Rect {
    Rect::new(
        f64::from(geometry.x),
        f64::from(geometry.y),
        f64::from(geometry.width),
        f64::from(geometry.height),
    )
}

/// A name for a new file: time and process, in base 36.
fn unique_suffix(attempt: u32) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let mut value = nanos ^ (u128::from(std::process::id()) << 64) ^ u128::from(attempt);
    let mut text = String::new();
    for _ in 0..6 {
        let digit = (value % 36) as u32;
        text.push(char::from_digit(digit, 36).unwrap_or('0'));
        value /= 36;
    }
    text
}

/// Writes the capture as a new PNG in `folder`, named `<stem><suffix>.png`.
fn write_png(image: &CapturedImage, folder: &Path, stem: &str) -> io::Result<PathBuf> {
    let bytes = encode(image, ImageFormat::Png, 0).map_err(io::Error::other)?;
    std::fs::create_dir_all(folder)?;
    let mut last = io::Error::other("no file name was free");
    for attempt in 0..8 {
        let path = folder.join(format!("{stem}{}.png", unique_suffix(attempt)));
        match File::create_new(&path) {
            Ok(mut file) => {
                file.write_all(&bytes)?;
                return Ok(path);
            }
            Err(error) => last = error,
        }
    }
    Err(last)
}

/// Open in Editor: the capture as a PNG for the editor. Kept in the
/// temporary folder, as 15.0.0 kept App Launcher's; the editor removes it
/// when its window closes.
pub(crate) fn save_for_editor(image: &CapturedImage) -> Result<PathBuf, ExportReport> {
    let folder = std::env::temp_dir().join("phramer");
    write_png(image, &folder, "editor-").map_err(|error| {
        notice(
            format!("Unable to write in {}: {error}", folder.display()),
            true,
        )
    })
}

/// On the main thread: tells the editor to open `path`.
pub(crate) fn open_in_editor<R: Runtime>(app: &AppHandle<R>, path: &Path) {
    let path = path.display().to_string();
    log::info!("editor: opening {path}");
    if let Err(error) = app.emit(EDITOR_OPEN_EVENT, &path) {
        log::error!("cannot send {EDITOR_OPEN_EVENT}: {error}");
    }
}

/// App Launcher: the capture saved to the temporary folder, named after the
/// filename pattern (as 15.0.0 named it), then the launcher's list of apps
/// for it.
pub(crate) fn open_with(settings: &Settings, image: &CapturedImage) -> ExportReport {
    let app = match app() {
        Ok(app) => app,
        Err(report) => return report,
    };
    let folder = std::env::temp_dir();
    let stem: String = expand_pattern(&settings.filename_pattern(), &LocalTime::now())
        .chars()
        .map(|c| if RESERVED.contains(&c) { '_' } else { c })
        .collect();
    match write_png(image, &folder, &format!("{stem}_")) {
        Ok(path) => {
            launcher_ui::open(app, path);
            ExportReport::default()
        }
        Err(error) => notice(
            format!("Unable to write in {}: {error}", folder.display()),
            true,
        ),
    }
}

/// Windows' "Open with" dialog for `path`: modal and as long as the user
/// likes, so on a thread of its own.
pub(crate) fn show_open_with(path: PathBuf) -> io::Result<()> {
    std::thread::Builder::new()
        .name("open-with".to_owned())
        .spawn(move || {
            if let Err(error) = open_with_dialog(&path) {
                log::error!("cannot open {} with another app: {error}", path.display());
            }
        })
        .map(|_| ())
}

/// `SHOpenWithDialog`: lets the user pick an app, and runs it on `path`.
fn open_with_dialog(path: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::Com::{
        COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize,
    };
    use windows_sys::Win32::UI::Shell::{
        OAIF_ALLOW_REGISTRATION, OAIF_EXEC, OPENASINFO, SHOpenWithDialog,
    };

    let file: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let info = OPENASINFO {
        pcszFile: file.as_ptr(),
        pcszClass: std::ptr::null(),
        oaifInFlags: OAIF_ALLOW_REGISTRATION | OAIF_EXEC,
    };
    // SAFETY: a new thread's own apartment, left below; `info` and `file`
    // outlive the modal call.
    let result = unsafe {
        let initialised = CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32) >= 0;
        let result = SHOpenWithDialog(std::ptr::null_mut(), &info);
        if initialised {
            CoUninitialize();
        }
        result
    };
    if result < 0 {
        // Cancelling the dialog is not an error
        const CANCELLED: i32 = 0x8007_04C7_u32 as i32;
        if result == CANCELLED {
            return Ok(());
        }
        return Err(io::Error::from_raw_os_error(result));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suffixes_are_six_base_36_digits() {
        let suffix = unique_suffix(0);
        assert_eq!(suffix.len(), 6);
        assert!(suffix.chars().all(|c| c.is_ascii_alphanumeric()));
        assert_ne!(unique_suffix(0), unique_suffix(1));
    }

    #[test]
    fn captures_are_written_as_new_pngs() {
        let folder =
            std::env::temp_dir().join(format!("phramer-handoff-test-{}", std::process::id()));
        let image = CapturedImage {
            width: 2,
            height: 1,
            bgra: vec![0, 0, 255, 255, 0, 255, 0, 255],
        };
        let first = write_png(&image, &folder, "x_").unwrap();
        let second = write_png(&image, &folder, "x_").unwrap();
        assert_ne!(first, second);
        let bytes = std::fs::read(&first).unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        let name = first.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("x_") && name.ends_with(".png"), "{name}");
        let _ = std::fs::remove_dir_all(&folder);
    }
}
