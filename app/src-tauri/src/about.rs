//! The tray's About (15.0.0's `InfoWindow`): the icon, license, version and
//! operating system, with Copy Info. A task dialog on a thread of its own,
//! so the tray stays responsive while it is open; asked for again, the open
//! one comes to the front.

use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};

use phramer_overlay::{Handoff, SystemClipboard};
use windows_sys::Win32::Foundation::{HWND, LPARAM, S_FALSE, S_OK, WPARAM};
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress, LoadLibraryW};
use windows_sys::Win32::UI::Controls::{
    TASKDIALOG_BUTTON, TASKDIALOGCONFIG, TASKDIALOGCONFIG_0, TDCBF_CLOSE_BUTTON,
    TDF_ALLOW_DIALOG_CANCELLATION, TDF_USE_HICON_MAIN, TDN_BUTTON_CLICKED, TDN_CREATED,
    TDN_DESTROYED,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{LoadIconW, SetForegroundWindow};
use windows_sys::core::{BOOL, HRESULT};

const LICENSE: &str = "GPLv3+";
/// Copy Info's button id; the common buttons use the low ones.
const COPY_INFO: i32 = 1000;
/// `tauri_utils::platform::WINDOWS_APP_ICON_RESOURCE_ID`: the app icon
/// tauri-build puts in the executable.
const APP_ICON: u16 = 32512;

/// An About is open or opening.
static RUNNING: AtomicBool = AtomicBool::new(false);
/// Its window, once created.
static OPEN: AtomicPtr<core::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());

/// Shows About, or brings the open one to the front.
pub(crate) fn show() {
    if RUNNING.swap(true, Ordering::AcqRel) {
        let open = OPEN.load(Ordering::Acquire);
        if !open.is_null() {
            // SAFETY: a window handle; a stale one only makes the call fail.
            unsafe { SetForegroundWindow(open) };
        }
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("about".to_owned())
        .spawn(|| {
            run();
            RUNNING.store(false, Ordering::Release);
        });
    if let Err(error) = spawned {
        RUNNING.store(false, Ordering::Release);
        log::error!("cannot show About: {error}");
    }
}

/// "Windows 11 (10.0.26300)": the product from the build number, as
/// Windows 11 still reports itself as 10.0.
fn os_description(major: u32, minor: u32, build: u32) -> String {
    let product = if major == 10 && build >= 22000 {
        "Windows 11".to_owned()
    } else if major == 10 {
        "Windows 10".to_owned()
    } else {
        "Windows".to_owned()
    };
    format!("{product} ({major}.{minor}.{build})")
}

/// What Copy Info puts on the clipboard (`versionInfo` and the OS).
fn info_text(version: &str, os: &str) -> String {
    format!("Phramer {version}\n{os}")
}

/// The real version: `GetVersionEx` reports what the manifest declares.
fn current_os() -> String {
    use windows_sys::Wdk::System::SystemServices::RtlGetVersion;
    use windows_sys::Win32::System::SystemInformation::OSVERSIONINFOW;

    let mut info = OSVERSIONINFOW {
        dwOSVersionInfoSize: size_of::<OSVERSIONINFOW>() as u32,
        ..Default::default()
    };
    // SAFETY: `info` is a valid OSVERSIONINFOW with its size set.
    if unsafe { RtlGetVersion(&mut info) } < 0 {
        return "Windows".to_owned();
    }
    os_description(info.dwMajorVersion, info.dwMinorVersion, info.dwBuildNumber)
}

fn run() {
    let Some(task_dialog) = task_dialog_indirect() else {
        log::error!("cannot show About: no TaskDialogIndirect");
        return;
    };
    let os = current_os();
    let version = phramer_core::VERSION;
    let info = info_text(version, &os);
    let wide = |text: &str| -> Vec<u16> { text.encode_utf16().chain(Some(0)).collect() };
    let title = wide("About");
    let heading = wide("Phramer");
    let content = wide(&format!(
        "License: {LICENSE}\nVersion: Phramer {version}\nOperating system: {os}"
    ));
    let copy = wide("Copy Info");
    let buttons = [TASKDIALOG_BUTTON {
        nButtonID: COPY_INFO,
        pszButtonText: copy.as_ptr(),
    }];
    // SAFETY: the module of this process and a resource id; a missing icon
    // gives null, and the dialog then shows none.
    let icon = unsafe { LoadIconW(GetModuleHandleW(std::ptr::null()), APP_ICON as usize as _) };
    let config = TASKDIALOGCONFIG {
        cbSize: size_of::<TASKDIALOGCONFIG>() as u32,
        dwFlags: TDF_ALLOW_DIALOG_CANCELLATION
            | if icon.is_null() {
                0
            } else {
                TDF_USE_HICON_MAIN
            },
        dwCommonButtons: TDCBF_CLOSE_BUTTON,
        pszWindowTitle: title.as_ptr(),
        Anonymous1: TASKDIALOGCONFIG_0 { hMainIcon: icon },
        pszMainInstruction: heading.as_ptr(),
        pszContent: content.as_ptr(),
        cButtons: buttons.len() as u32,
        pButtons: buttons.as_ptr(),
        pfCallback: Some(callback),
        lpCallbackData: (&raw const info) as isize,
        ..Default::default()
    };
    // SAFETY: `config`, the strings, the buttons and `info` (the callback's
    // data) all outlive the modal call; the out-parameters may be null.
    let result = unsafe {
        task_dialog(
            &config,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if result < 0 {
        log::error!("cannot show About ({result:#x})");
    }
}

/// Copy Info copies and keeps the dialog open, as 15.0.0's button did.
unsafe extern "system" fn callback(
    hwnd: HWND,
    notification: u32,
    wparam: WPARAM,
    _lparam: LPARAM,
    data: isize,
) -> HRESULT {
    // TASKDIALOG_NOTIFICATIONS are i32, the callback's parameter u32
    match notification as i32 {
        TDN_CREATED => OPEN.store(hwnd, Ordering::Release),
        TDN_DESTROYED => OPEN.store(std::ptr::null_mut(), Ordering::Release),
        TDN_BUTTON_CLICKED if wparam as i32 == COPY_INFO => {
            // SAFETY: `data` is the `info` that `run` keeps alive for the
            // whole dialog.
            let info = unsafe { &*(data as *const String) };
            if let Err(error) = SystemClipboard.copy_text(info) {
                log::error!("About: cannot copy the info: {error}");
            }
            return S_FALSE;
        }
        _ => {}
    }
    S_OK
}

type TaskDialogIndirect =
    unsafe extern "system" fn(*const TASKDIALOGCONFIG, *mut i32, *mut i32, *mut BOOL) -> HRESULT;

/// Looked up rather than imported: only Common Controls 6, which the app's
/// manifest selects, has it, and the test binaries carry no manifest.
fn task_dialog_indirect() -> Option<TaskDialogIndirect> {
    let library: Vec<u16> = "comctl32.dll".encode_utf16().chain(Some(0)).collect();
    // SAFETY: a NUL-terminated name; comctl32 stays loaded for the process.
    let module = unsafe { LoadLibraryW(library.as_ptr()) };
    if module.is_null() {
        return None;
    }
    // SAFETY: a loaded module and a NUL-terminated ANSI name.
    let address = unsafe { GetProcAddress(module, c"TaskDialogIndirect".as_ptr().cast()) }?;
    // SAFETY: TaskDialogIndirect has exactly this signature.
    Some(unsafe {
        std::mem::transmute::<unsafe extern "system" fn() -> isize, TaskDialogIndirect>(address)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_11_is_told_apart_by_its_build() {
        assert_eq!(os_description(10, 0, 26300), "Windows 11 (10.0.26300)");
        assert_eq!(os_description(10, 0, 22000), "Windows 11 (10.0.22000)");
        assert_eq!(os_description(10, 0, 19045), "Windows 10 (10.0.19045)");
    }

    #[test]
    fn copy_info_names_the_version_and_the_system() {
        assert_eq!(
            info_text("16.0.0", "Windows 11 (10.0.26300)"),
            "Phramer 16.0.0\nWindows 11 (10.0.26300)"
        );
    }

    #[test]
    fn this_system_is_described() {
        assert!(current_os().starts_with("Windows"));
    }
}
