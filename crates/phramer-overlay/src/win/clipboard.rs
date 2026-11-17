//! The Windows clipboard: images as 15.0.0 put them there (`CF_DIB`,
//! `CF_DIBV5` and the registered `PNG` format Qt adds), text, and files as
//! Explorer copies them (`CF_HDROP` with a copy drop effect).

use std::path::Path;

use windows_sys::Win32::Foundation::{GlobalFree, HANDLE, HWND};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows_sys::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows_sys::Win32::System::Ole::{
    CF_DIB, CF_DIBV5, CF_HDROP, CF_UNICODETEXT, DROPEFFECT_COPY,
};
use windows_sys::Win32::UI::Shell::DROPFILES;
use windows_sys::Win32::UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, HWND_MESSAGE};

use super::last_error;
use crate::error::OverlayError;
use crate::export::{CapturedImage, Handoff, ImageFormat, dib, dib_v5, encode};

/// Tries before giving up when another program holds the clipboard open,
/// which the documentation says happens and is only ever brief.
const OPEN_ATTEMPTS: u32 = 10;

/// The system clipboard as the export's [`Handoff`].
#[derive(Debug, Default)]
pub struct SystemClipboard;

impl Handoff for SystemClipboard {
    fn copy_image(&mut self, image: &CapturedImage, jpeg: Option<u8>) -> Result<(), OverlayError> {
        let (name, bytes) = match jpeg {
            // `saveToClipboardMime("jpeg")`; JFIF is the name other Windows
            // programs read JPEG under
            Some(quality) => ("JFIF", encode(image, ImageFormat::Jpeg, quality)?),
            None => ("PNG", encode(image, ImageFormat::Png, 0)?),
        };
        let mut entries = vec![
            (u32::from(CF_DIBV5), dib_v5(image)),
            (u32::from(CF_DIB), dib(image)),
            (registered(name)?, bytes.clone()),
        ];
        if jpeg.is_some() {
            entries.push((registered("image/jpeg")?, bytes));
        }
        set(&entries)
    }

    fn copy_text(&mut self, text: &str) -> Result<(), OverlayError> {
        set_clipboard_text(text)
    }

    fn copy_file(&mut self, path: &Path) -> Result<(), OverlayError> {
        let absolute = std::path::absolute(path).map_err(|source| OverlayError::Write {
            path: path.to_owned(),
            source,
        })?;
        // DROPFILES, then the wide path, then the empty string ending the list
        let header = size_of::<DROPFILES>();
        let mut bytes = vec![0u8; header];
        let files = DROPFILES {
            pFiles: u32::try_from(header).unwrap_or(0),
            fWide: 1,
            ..Default::default()
        };
        // SAFETY: `bytes` holds exactly one DROPFILES; it is written
        // unaligned because a Vec<u8> promises no alignment.
        unsafe { std::ptr::write_unaligned(bytes.as_mut_ptr().cast::<DROPFILES>(), files) };
        for unit in absolute.as_os_str().encode_wide_with_nul() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        bytes.extend_from_slice(&[0, 0]);
        // Without it Explorer is free to treat a paste as a move
        let effect = DROPEFFECT_COPY.to_le_bytes().to_vec();
        set(&[
            (u32::from(CF_HDROP), bytes),
            (registered("Preferred DropEffect")?, effect),
        ])
    }
}

trait EncodeWideWithNul {
    fn encode_wide_with_nul(&self) -> Vec<u16>;
}

impl EncodeWideWithNul for std::ffi::OsStr {
    fn encode_wide_with_nul(&self) -> Vec<u16> {
        use std::os::windows::ffi::OsStrExt;
        self.encode_wide().chain(Some(0)).collect()
    }
}

/// Puts plain text on the clipboard (`CF_UNICODETEXT`).
pub(crate) fn set_clipboard_text(text: &str) -> Result<(), OverlayError> {
    let bytes: Vec<u8> = text
        .encode_utf16()
        .chain(Some(0))
        .flat_map(u16::to_le_bytes)
        .collect();
    set(&[(u32::from(CF_UNICODETEXT), bytes)])
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

fn registered(name: &str) -> Result<u32, OverlayError> {
    let name = wide(name);
    // SAFETY: `name` is NUL-terminated and outlives the call.
    let format = unsafe { RegisterClipboardFormatW(name.as_ptr()) };
    if format == 0 {
        return Err(last_error("RegisterClipboardFormatW"));
    }
    Ok(format)
}

/// A hidden message-only window to own the clipboard while it is set: with
/// no owner `SetClipboardData` is documented to fail. The data stays on
/// the clipboard after the window is gone.
struct Owner(HWND);

impl Owner {
    fn new() -> Result<Self, OverlayError> {
        let class = wide("STATIC");
        // SAFETY: a message-only window of a system class; every pointer is
        // valid or null.
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                std::ptr::null(),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        };
        if hwnd.is_null() {
            return Err(last_error("CreateWindowExW"));
        }
        Ok(Self(hwnd))
    }
}

impl Drop for Owner {
    fn drop(&mut self) {
        // SAFETY: created by this thread above, destroyed once.
        unsafe { DestroyWindow(self.0) };
    }
}

/// The clipboard, open for this thread; closed when dropped.
pub(super) struct Open {
    _owner: Option<Owner>,
}

impl Open {
    /// Opens it, retrying briefly while another program has it open.
    /// `owned`: whether a window is needed to put data on it.
    pub(super) fn new(owned: bool) -> Result<Self, OverlayError> {
        let owner = if owned { Some(Owner::new()?) } else { None };
        let hwnd = owner.as_ref().map_or(std::ptr::null_mut(), |owner| owner.0);
        for attempt in 1..=OPEN_ATTEMPTS {
            // SAFETY: `hwnd` is null or the owner window, alive until after
            // the clipboard is closed.
            if unsafe { OpenClipboard(hwnd) } != 0 {
                return Ok(Self { _owner: owner });
            }
            if attempt == OPEN_ATTEMPTS {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        Err(last_error("OpenClipboard"))
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        // SAFETY: the clipboard was opened by this thread in `new`.
        unsafe { CloseClipboard() };
    }
}

/// A moveable global memory block holding `bytes`, as the clipboard takes
/// ownership of.
fn global(bytes: &[u8]) -> Result<HANDLE, OverlayError> {
    // SAFETY: allocates a block of the requested size.
    let handle = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1)) };
    if handle.is_null() {
        return Err(last_error("GlobalAlloc"));
    }
    // SAFETY: `handle` is a live moveable block of at least `bytes.len()`
    // bytes; it is locked for the copy and unlocked right after.
    unsafe {
        let target = GlobalLock(handle);
        if target.is_null() {
            let error = last_error("GlobalLock");
            GlobalFree(handle);
            return Err(error);
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), target.cast::<u8>(), bytes.len());
        GlobalUnlock(handle);
    }
    Ok(handle)
}

/// Replaces the clipboard's contents with `entries` (format, bytes).
fn set(entries: &[(u32, Vec<u8>)]) -> Result<(), OverlayError> {
    let _open = Open::new(true)?;
    // SAFETY: the clipboard is open and owned by this thread's window.
    if unsafe { EmptyClipboard() } == 0 {
        return Err(last_error("EmptyClipboard"));
    }
    for (format, bytes) in entries {
        let handle = global(bytes)?;
        // SAFETY: the clipboard is open; on success the system owns
        // `handle`, on failure it is still ours to free.
        if unsafe { SetClipboardData(*format, handle) }.is_null() {
            let error = last_error("SetClipboardData");
            // SAFETY: not taken by the clipboard, so freed here once.
            unsafe { GlobalFree(handle) };
            return Err(error);
        }
    }
    Ok(())
}

/// Reading the clipboard back, for the tests.
#[cfg(test)]
pub(super) mod read {
    use std::path::PathBuf;

    use windows_sys::Win32::System::DataExchange::{
        EnumClipboardFormats, GetClipboardData, GetClipboardFormatNameW,
    };
    use windows_sys::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
    use windows_sys::Win32::System::Ole::{CF_DIB, CF_HDROP, CF_UNICODETEXT};
    use windows_sys::Win32::UI::Shell::{DragQueryFileW, HDROP};

    use super::Open;

    fn bytes(format: u32) -> Option<Vec<u8>> {
        let _open = Open::new(false).ok()?;
        // SAFETY: the clipboard is open; the handle stays valid while it is,
        // and is only read between lock and unlock.
        unsafe {
            let handle = GetClipboardData(format);
            if handle.is_null() {
                return None;
            }
            let size = GlobalSize(handle);
            let data = GlobalLock(handle);
            if data.is_null() {
                return None;
            }
            let copy = std::slice::from_raw_parts(data.cast::<u8>(), size).to_vec();
            GlobalUnlock(handle);
            Some(copy)
        }
    }

    /// The text on the clipboard, if any.
    pub(crate) fn text() -> Option<String> {
        let bytes = bytes(u32::from(CF_UNICODETEXT))?;
        let units: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .take_while(|&unit| unit != 0)
            .collect();
        Some(String::from_utf16_lossy(&units))
    }

    /// `CF_DIB`'s header fields: width, height, bits per pixel, and the
    /// first pixel of the top row (BGRA).
    pub(crate) fn dib() -> Option<(i32, i32, u16, [u8; 4])> {
        let bytes = bytes(u32::from(CF_DIB))?;
        let field = |at: usize| {
            i32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
        };
        let (header, width, height) = (field(0) as usize, field(4), field(8));
        let bits = u16::from_le_bytes([bytes[14], bytes[15]]);
        // Bottom-up rows: the top row is the last one
        let stride = width as usize * 4;
        let top = header + stride * (height as usize - 1);
        let pixel = [bytes[top], bytes[top + 1], bytes[top + 2], bytes[top + 3]];
        Some((width, height, bits, pixel))
    }

    /// The first file of `CF_HDROP`.
    pub(crate) fn file() -> Option<PathBuf> {
        let _open = Open::new(false).ok()?;
        // SAFETY: the clipboard is open; the HDROP stays valid while it is,
        // and `buffer` holds the length passed.
        unsafe {
            let handle = GetClipboardData(u32::from(CF_HDROP));
            if handle.is_null() {
                return None;
            }
            let mut buffer = [0u16; 1024];
            let length = DragQueryFileW(handle as HDROP, 0, buffer.as_mut_ptr(), 1024);
            (length > 0)
                .then(|| PathBuf::from(String::from_utf16_lossy(&buffer[..length as usize])))
        }
    }

    /// The names of every format on the clipboard (standard ones by number).
    pub(crate) fn formats() -> Vec<String> {
        let Ok(_open) = Open::new(false) else {
            return Vec::new();
        };
        let mut names = Vec::new();
        let mut format = 0;
        loop {
            // SAFETY: the clipboard is open; enumeration starts at 0.
            format = unsafe { EnumClipboardFormats(format) };
            if format == 0 {
                break;
            }
            let mut name = [0u16; 128];
            // SAFETY: `name` holds the length passed.
            let length = unsafe { GetClipboardFormatNameW(format, name.as_mut_ptr(), 128) };
            names.push(if length > 0 {
                String::from_utf16_lossy(&name[..length as usize])
            } else {
                format.to_string()
            });
        }
        names
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Empties the clipboard (when the user had no text on it).
    fn clear() {
        if let Ok(_open) = Open::new(true) {
            // SAFETY: the clipboard is open and owned by this thread.
            unsafe { EmptyClipboard() };
        }
    }

    /// Puts the user's text back however the test ends.
    struct Restore(Option<String>);

    impl Drop for Restore {
        fn drop(&mut self) {
            match &self.0 {
                Some(text) => {
                    let _ = set_clipboard_text(text);
                }
                None => clear(),
            }
        }
    }

    /// Every clipboard check in one test: the clipboard is one global
    /// resource and tests run in parallel.
    #[test]
    fn image_text_and_file_read_back() {
        let _restore = Restore(read::text());
        let mut clipboard = SystemClipboard;

        // 3x2, top-left pixel red
        let mut bgra = vec![255u8; 3 * 2 * 4];
        bgra[..4].copy_from_slice(&[0, 0, 255, 255]);
        let image = CapturedImage {
            width: 3,
            height: 2,
            bgra,
        };
        clipboard
            .copy_image(&image, None)
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(read::dib(), Some((3, 2, 32, [0, 0, 255, 255])));
        let formats = read::formats();
        assert!(formats.iter().any(|name| name == "PNG"), "{formats:?}");
        // CF_DIBV5 (17) is ours, not synthesised
        assert!(formats.iter().any(|name| name == "17"), "{formats:?}");

        clipboard
            .copy_image(&image, Some(80))
            .unwrap_or_else(|error| panic!("{error}"));
        let formats = read::formats();
        assert!(formats.iter().any(|name| name == "JFIF"), "{formats:?}");
        assert!(!formats.iter().any(|name| name == "PNG"), "{formats:?}");

        clipboard
            .copy_text(r"C:\captures\shot.png")
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(read::text().as_deref(), Some(r"C:\captures\shot.png"));

        let file = std::env::temp_dir().join(format!("phramer-clip-{}.png", std::process::id()));
        std::fs::write(&file, b"x").unwrap_or_else(|error| panic!("{error}"));
        clipboard
            .copy_file(&file)
            .unwrap_or_else(|error| panic!("{error}"));
        let copied = read::file();
        let formats = read::formats();
        let _ = std::fs::remove_file(&file);
        assert_eq!(copied, Some(file));
        assert!(
            formats.iter().any(|name| name == "Preferred DropEffect"),
            "{formats:?}"
        );
    }
}
