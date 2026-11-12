//! Show in Folder (`FileHandoff::showInFolder`): Explorer with the file
//! selected.

use std::path::Path;

use windows_sys::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows_sys::Win32::UI::Shell::{ILCreateFromPathW, ILFree, SHOpenFolderAndSelectItems};

use crate::error::OverlayError;

/// Opens the folder of `path` with the file selected, reusing an Explorer
/// window that already shows the folder. Runs on a thread of its own, since
/// the shell call needs COM and can take a moment; returns at once.
pub fn show_in_folder(path: &Path) -> Result<(), OverlayError> {
    if !path.exists() {
        return Err(OverlayError::Write {
            path: path.to_owned(),
            source: std::io::Error::from(std::io::ErrorKind::NotFound),
        });
    }
    let path = std::path::absolute(path).map_err(|source| OverlayError::Write {
        path: path.to_owned(),
        source,
    })?;
    std::thread::Builder::new()
        .name("show-in-folder".into())
        .spawn(move || {
            if !select_in_explorer(&path) {
                // The shell call failed: at least open the folder
                let folder = path.parent().unwrap_or(&path).to_owned();
                if let Err(error) = std::process::Command::new("explorer.exe")
                    .arg(&folder)
                    .spawn()
                {
                    log::error!("cannot open {}: {error}", folder.display());
                }
            }
        })
        .map(drop)
        .map_err(|source| OverlayError::Write {
            path: std::path::PathBuf::new(),
            source,
        })
}

fn select_in_explorer(path: &Path) -> bool {
    use std::os::windows::ffi::OsStrExt;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: COM is initialised for this new thread and uninitialised at
    // the end; `wide` is NUL-terminated; the item list is freed once.
    unsafe {
        let initialised = CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32) >= 0;
        let item = ILCreateFromPathW(wide.as_ptr());
        let shown = !item.is_null() && {
            let result = SHOpenFolderAndSelectItems(item, 0, std::ptr::null(), 0);
            ILFree(item);
            result >= 0
        };
        if initialised {
            CoUninitialize();
        }
        shown
    }
}
