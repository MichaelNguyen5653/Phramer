//! Snipping Tool's screen recorder, 15.0.0's `utils/snippingtool.cpp`:
//! video is handed to Windows' own tool. The Win+Shift+R link goes to
//! Snipping Tool by package family; failing that, the app is started by its
//! application id.
//!
//! The link is never launched without that target: Phramer can be the
//! registered `ms-screenclip` handler and would be handed it back.

use std::sync::Once;

use windows::System::{Launcher, LauncherOptions};
use windows::Win32::System::Com::{CLSCTX_LOCAL_SERVER, CoCreateInstance, CoIncrementMTAUsage};
use windows::Win32::UI::Shell::{
    AO_NONE, ApplicationActivationManager, IApplicationActivationManager,
};
use windows::core::{HSTRING, PCWSTR};

const PACKAGE_FAMILY: &str = "Microsoft.ScreenSketch_8wekyb3d8bbwe";
const APPLICATION_ID: &str = "Microsoft.ScreenSketch_8wekyb3d8bbwe!App";
/// The form Windows sends for Win+Shift+R, as Snipping Tool spells it.
const RECORDING_URI: &str = "ms-screenclip:capture?mode=default&type=recording&source=Phramer";

/// Keeps the process's multithreaded apartment up for good: WinRT calls
/// from threads of no apartment of their own run there, and `windows`
/// caches factories that must not outlive it.
fn ensure_apartment() {
    static MTA: Once = Once::new();
    MTA.call_once(|| {
        // SAFETY: no arguments; the usage it takes is never released.
        if let Err(error) = unsafe { CoIncrementMTAUsage() } {
            log::warn!("no multithreaded apartment: {error}");
        }
    });
}

/// Whether Snipping Tool is installed for this user.
pub(crate) fn is_available() -> bool {
    use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS};
    use windows_sys::Win32::Storage::Packaging::Appx::{
        FindPackagesByPackageFamily, PACKAGE_FILTER_HEAD,
    };

    let family: Vec<u16> = PACKAGE_FAMILY.encode_utf16().chain(Some(0)).collect();
    let (mut count, mut length) = (0u32, 0u32);
    // SAFETY: a first call with no buffers only reports the sizes; `family`
    // is NUL-terminated and the counts are valid out-parameters.
    let status = unsafe {
        FindPackagesByPackageFamily(
            family.as_ptr(),
            PACKAGE_FILTER_HEAD,
            &mut count,
            std::ptr::null_mut(),
            &mut length,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    (status == ERROR_SUCCESS || status == ERROR_INSUFFICIENT_BUFFER) && count > 0
}

/// The recording link, aimed at Snipping Tool.
fn launch_recording_link() -> windows::core::Result<bool> {
    let options = LauncherOptions::new()?;
    options.SetTargetApplicationPackageFamilyName(&HSTRING::from(PACKAGE_FAMILY))?;
    let uri = windows::Foundation::Uri::CreateUri(&HSTRING::from(RECORDING_URI))?;
    Launcher::LaunchUriWithOptionsAsync(&uri, &options)?.join()
}

/// Snipping Tool itself, when the link was refused.
fn launch_app() -> windows::core::Result<()> {
    // SAFETY: creates the system's activation manager in its own server.
    let manager: IApplicationActivationManager =
        unsafe { CoCreateInstance(&ApplicationActivationManager, None, CLSCTX_LOCAL_SERVER)? };
    let id = HSTRING::from(APPLICATION_ID);
    // SAFETY: `id` is a NUL-terminated wide string that outlives the call;
    // no arguments are passed.
    unsafe { manager.ActivateApplication(&id, PCWSTR::null(), AO_NONE) }.map(drop)
}

fn launch() -> bool {
    ensure_apartment();
    match launch_recording_link() {
        Ok(true) => return true,
        Ok(false) => log::warn!("Windows did not open the recording link"),
        Err(error) => log::warn!("cannot open the recording link: {error}"),
    }
    match launch_app() {
        Ok(()) => true,
        Err(error) => {
            log::error!("cannot start Snipping Tool: {error}");
            false
        }
    }
}

/// Starts a recording and waits for Windows to act on it: for the
/// short-lived process Windows starts for a link, which would otherwise
/// exit first. On a thread of its own, which has no apartment of its own.
pub(crate) fn start_recording() -> bool {
    std::thread::spawn(launch).join().unwrap_or(false)
}

/// Starts a recording without waiting; `done` hears whether it started, on
/// the launching thread.
pub(crate) fn start_recording_async(done: impl FnOnce(bool) + Send + 'static) {
    let spawned = std::thread::Builder::new()
        .name("snipping-tool".to_owned())
        .spawn(move || done(launch()));
    if let Err(error) = spawned {
        log::error!("cannot start Snipping Tool: {error}");
    }
}
