//! Tray balloons, `QSystemTrayIcon::showMessage` in 15.0.0: a
//! `Shell_NotifyIcon` info message on the daemon's own tray icon, which
//! Windows 10 and 11 show as a toast without the app being registered.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use phramer_core::config::Settings;
use tauri::{AppHandle, Runtime};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::UI::Shell::{
    DefSubclassProc, NIF_INFO, NIIF_NONE, NIM_MODIFY, NIN_BALLOONUSERCLICK, NOTIFYICONDATAW,
    SetWindowSubclass, Shell_NotifyIconW,
};

use crate::tray;

/// The callback message `tray-icon` 0.25 gives its icons
/// (`WM_USER_TRAYICON`); balloon clicks arrive as this message.
const TRAY_CALLBACK_MESSAGE: u32 = 6002;
/// Our subclass of the tray icon's window.
const SUBCLASS_ID: usize = 0x5048_5241;

/// The file the last balloon was about (`m_notificationFile`): clicking the
/// balloon shows it in its folder. Cleared by a balloon about nothing.
static NOTIFICATION_FILE: Mutex<Option<PathBuf>> = Mutex::new(None);
/// The tray window already subclassed, as an address.
static SUBCLASSED: Mutex<Option<isize>> = Mutex::new(None);

/// Highest icon id tried. `tray-icon` numbers its icons from a process-wide
/// counter it does not expose; each icon has a window of its own, so the one
/// id that window accepts is the icon's.
const MAX_ICON_ID: u32 = 64;

/// Shows `text` as a balloon on the tray icon. Nothing is shown while the
/// icon is hidden or absent, as with 15.0.0.
pub(crate) fn tray_message<R: Runtime>(app: &AppHandle<R>, title: &str, text: &str) {
    balloon(app, title, text, None);
}

fn balloon<R: Runtime>(app: &AppHandle<R>, title: &str, text: &str, file: Option<&Path>) {
    log::info!("notification: {title}: {text}");
    if let Ok(mut current) = NOTIFICATION_FILE.lock() {
        *current = file.map(Path::to_owned);
    }
    let shown = tray::visible_window(app).is_some_and(|hwnd| {
        watch_clicks(hwnd);
        show_balloon(hwnd as HWND, title, text)
    });
    if !shown {
        log::warn!("notification not shown: no visible tray icon");
    }
}

/// A capture message (`AbstractLogger` with a notification path), already
/// logged by whoever wrote it: shown only when desktop notifications are
/// on; clicking it opens the folder of `file`.
pub(crate) fn capture_message<R: Runtime>(
    app: &AppHandle<R>,
    settings: &Settings,
    text: &str,
    file: Option<&Path>,
    error: bool,
) {
    // The export has logged the text already
    if settings.show_desktop_notification() {
        let title = if error { "Phramer Error" } else { "Phramer Info" };
        balloon(app, title, text, file);
    }
}

/// Subclasses the tray icon's window once, to hear balloon clicks, which
/// `tray-icon` does not report. Must run on the thread that owns the window
/// (the main thread, where notifications are sent from).
fn watch_clicks(hwnd: isize) {
    let Ok(mut subclassed) = SUBCLASSED.lock() else {
        return;
    };
    if *subclassed == Some(hwnd) {
        return;
    }
    // SAFETY: `hwnd` is the tray icon's live window, owned by this thread;
    // `on_tray_message` matches SUBCLASSPROC and outlives the window.
    if unsafe { SetWindowSubclass(hwnd as HWND, Some(on_tray_message), SUBCLASS_ID, 0) } != 0 {
        *subclassed = Some(hwnd);
    }
}

unsafe extern "system" fn on_tray_message(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    if message == TRAY_CALLBACK_MESSAGE && (lparam & 0xffff) as u32 == NIN_BALLOONUSERCLICK {
        let file = NOTIFICATION_FILE.lock().ok().and_then(|file| file.clone());
        if let Some(file) = file
            && let Err(error) = phramer_overlay::show_in_folder(&file)
        {
            log::error!("cannot show {}: {error}", file.display());
        }
    }
    // SAFETY: passes the message on unchanged, as a subclass must.
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

/// `SystemNotification::sendMessage`: only when desktop notifications are on.
pub(crate) fn system_message<R: Runtime>(app: &AppHandle<R>, settings: &Settings, text: &str) {
    if settings.show_desktop_notification() {
        tray_message(app, "Phramer Info", text);
    } else {
        log::info!("{text}");
    }
}

/// `AbstractLogger::warning()`: only when desktop notifications are on.
pub(crate) fn warning_message<R: Runtime>(app: &AppHandle<R>, settings: &Settings, text: &str) {
    log::warn!("{text}");
    if settings.show_desktop_notification() {
        balloon(app, "Phramer Warning", text, None);
    }
}

fn copy_into(target: &mut [u16], text: &str) {
    let capacity = target.len().saturating_sub(1);
    let mut length = 0;
    for (slot, unit) in target.iter_mut().zip(text.encode_utf16().take(capacity)) {
        *slot = unit;
        length += 1;
    }
    if let Some(end) = target.get_mut(length) {
        *end = 0;
    }
}

fn show_balloon(hwnd: HWND, title: &str, text: &str) -> bool {
    let mut data = NOTIFYICONDATAW {
        cbSize: u32::try_from(size_of::<NOTIFYICONDATAW>()).unwrap_or(u32::MAX),
        hWnd: hwnd,
        uFlags: NIF_INFO,
        dwInfoFlags: NIIF_NONE,
        ..Default::default()
    };
    copy_into(&mut data.szInfoTitle, title);
    copy_into(&mut data.szInfo, text);
    (0..=MAX_ICON_ID).any(|id| {
        data.uID = id;
        // SAFETY: `data` is a fully initialised NOTIFYICONDATAW whose size is
        // in `cbSize`; an id the window does not own just fails.
        unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) != 0 }
    })
}

#[cfg(test)]
mod tests {
    use super::copy_into;

    #[test]
    fn text_is_truncated_and_terminated() {
        let mut buffer = [1u16; 4];
        copy_into(&mut buffer, "Phramer");
        assert_eq!(buffer, [u16::from(b'P'), u16::from(b'h'), u16::from(b'r'), 0]);
        let mut buffer = [1u16; 4];
        copy_into(&mut buffer, "ok");
        assert_eq!(buffer, [u16::from(b'o'), u16::from(b'k'), 0, 1]);
    }
}
