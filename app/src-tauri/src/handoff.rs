//! Where the overlay's other actions send a capture: OCR, the editor and
//! another application (App Launcher). Each runs on the capture's thread
//! and reports through the same notices as an export.

use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use phramer_core::config::Settings;
use phramer_ocr::{OcrStatus, OcrTextLayout, RgbaImage, status_message};
use phramer_overlay::{
    CapturedImage, ExportReport, Handoff, ImageFormat, LocalTime, Notice, SystemClipboard, encode,
    expand_pattern,
};
use tauri::{AppHandle, Emitter, Runtime};

/// The event the editor window listens for (`editor::listen`); the payload
/// is the path of a PNG of the capture.
pub(crate) const EDITOR_OPEN_EVENT: &str = "editor:open";
/// What the user is told once the text is on the clipboard.
const TEXT_COPIED: &str = "Text copied to clipboard.";
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

/// OCR: the text in the capture, laid out as `ocrTextLayout` says, goes on
/// the clipboard. 15.0.0 showed it in a results window; until there is a
/// window, the clipboard and a notification stand in for it.
pub(crate) fn read_text(settings: &Settings, image: &CapturedImage) -> ExportReport {
    let rgba = RgbaImage::from_bgra(image.width, image.height, &image.bgra);
    let language = settings.ocr_language();
    let layout = OcrTextLayout::from_setting(i64::from(settings.ocr_text_layout()));
    // A thread of its own: WinRT is blocked on there, which a thread with a
    // single-threaded apartment must not do
    let read = std::thread::Builder::new()
        .name("ocr".to_owned())
        .spawn(move || phramer_ocr::read_text(&rgba, &language, layout))
        .map_err(|error| error.to_string())
        .and_then(|worker| {
            worker
                .join()
                .map_err(|_| "the recognizer stopped".to_owned())
        });
    match read {
        Ok(Ok(text)) => match SystemClipboard.copy_text(&text) {
            Ok(()) => notice(TEXT_COPIED.to_owned(), false),
            Err(error) => notice(format!("Error while saving to clipboard: {error}"), true),
        },
        Ok(Err(status)) => {
            let text = status_message(&status).unwrap_or_default();
            notice(text, status != OcrStatus::NoTextFound)
        }
        Err(error) => notice(
            status_message(&OcrStatus::EngineError(error)).unwrap_or_default(),
            true,
        ),
    }
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
/// filename pattern, then Windows' "Open with" dialog for it.
pub(crate) fn open_with(settings: &Settings, image: &CapturedImage) -> ExportReport {
    let folder = std::env::temp_dir();
    let stem: String = expand_pattern(&settings.filename_pattern(), &LocalTime::now())
        .chars()
        .map(|c| if RESERVED.contains(&c) { '_' } else { c })
        .collect();
    let path = match write_png(image, &folder, &format!("{stem}_")) {
        Ok(path) => path,
        Err(error) => {
            return notice(
                format!("Unable to write in {}: {error}", folder.display()),
                true,
            );
        }
    };
    // Modal and as long as the user likes: off the capture's thread, so the
    // next capture need not wait for it
    let spawned = std::thread::Builder::new()
        .name("open-with".to_owned())
        .spawn(move || {
            if let Err(error) = open_with_dialog(&path) {
                log::error!("cannot open {} with another app: {error}", path.display());
            }
        });
    match spawned {
        Ok(_) => ExportReport::default(),
        Err(error) => notice(format!("Unable to open the capture: {error}"), true),
    }
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
