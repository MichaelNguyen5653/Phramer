//! Tray balloons, `QSystemTrayIcon::showMessage` in 15.0.0: a
//! `Shell_NotifyIcon` info message on the daemon's own tray icon, which
//! Windows 10 and 11 show as a toast without the app being registered.

use phramer_core::config::Settings;
use tauri::{AppHandle, Runtime};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::Shell::{
    NIF_INFO, NIIF_NONE, NIM_MODIFY, NOTIFYICONDATAW, Shell_NotifyIconW,
};

use crate::tray;

/// Highest icon id tried. `tray-icon` numbers its icons from a process-wide
/// counter it does not expose; each icon has a window of its own, so the one
/// id that window accepts is the icon's.
const MAX_ICON_ID: u32 = 64;

/// Shows `text` as a balloon on the tray icon. Nothing is shown while the
/// icon is hidden or absent, as with 15.0.0.
pub(crate) fn tray_message<R: Runtime>(app: &AppHandle<R>, title: &str, text: &str) {
    log::info!("notification: {title}: {text}");
    let shown = tray::visible_window(app).is_some_and(|hwnd| show_balloon(hwnd as HWND, title, text));
    if !shown {
        log::warn!("notification not shown: no visible tray icon");
    }
}

/// `SystemNotification::sendMessage`: only when desktop notifications are on.
pub(crate) fn system_message<R: Runtime>(app: &AppHandle<R>, settings: &Settings, text: &str) {
    if settings.show_desktop_notification() {
        tray_message(app, "Phramer Info", text);
    } else {
        log::info!("{text}");
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
