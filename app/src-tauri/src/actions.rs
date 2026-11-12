//! What the tray, the hotkeys and forwarded commands do. Everything that
//! needs a feature not built yet is a stub here that logs
//! "not implemented yet", for the later phases to fill in.

use std::path::PathBuf;
use std::process::Command;

use tauri::{AppHandle, Manager, Runtime};

use crate::cli::{FullRequest, GuiRequest, ScreenRequest};
use crate::daemon::DaemonState;
use crate::{notify, settings};

fn not_implemented(feature: &str, detail: &dyn std::fmt::Debug) {
    log::info!("{feature}: not implemented yet ({detail:?})");
}

/// `Flameshot::gui` / `requestCapture` in graphical mode: the capture
/// overlay.
pub(crate) fn capture_gui<R: Runtime>(_app: &AppHandle<R>, request: GuiRequest) {
    not_implemented("capture overlay", &request);
}

/// `Flameshot::screen`: one monitor without the overlay, or the overlay
/// limited to it with `--edit`.
pub(crate) fn capture_screen<R: Runtime>(_app: &AppHandle<R>, request: ScreenRequest) {
    not_implemented("screen capture", &request);
}

/// `Flameshot::full`: every monitor.
pub(crate) fn capture_full<R: Runtime>(_app: &AppHandle<R>, request: FullRequest) {
    not_implemented("full capture", &request);
}

/// `Flameshot::launcher`.
pub(crate) fn open_launcher<R: Runtime>(_app: &AppHandle<R>) {
    not_implemented("capture launcher", &());
}

/// `Flameshot::config`: the settings window.
pub(crate) fn open_configuration<R: Runtime>(_app: &AppHandle<R>) {
    not_implemented("settings window", &());
}

/// `phramer config --check`. Runs in the calling process, without a daemon.
pub(crate) fn check_configuration() {
    not_implemented("configuration check", &());
}

/// `EditorWindow::openEmpty`.
pub(crate) fn open_editor<R: Runtime>(_app: &AppHandle<R>) {
    not_implemented("editor", &());
}

/// `FlameshotDaemon::checkForUpdates`.
pub(crate) fn check_for_updates<R: Runtime>(_app: &AppHandle<R>) {
    not_implemented("updater", &());
}

/// `WelcomeTour::showNow`.
pub(crate) fn show_whats_new<R: Runtime>(_app: &AppHandle<R>) {
    not_implemented("what's new tour", &());
}

/// `Flameshot::info`.
pub(crate) fn show_about<R: Runtime>(_app: &AppHandle<R>) {
    not_implemented("about window", &());
}

/// The last file saved this session; kept in memory only, by design. Set by
/// the export, which is not built yet.
pub(crate) fn last_capture() -> Option<PathBuf> {
    None
}

/// `FileHandoff::showInFolder(lastSaved())`.
pub(crate) fn show_last_capture_in_folder<R: Runtime>(_app: &AppHandle<R>) {
    not_implemented("show last capture in folder", &last_capture());
}

/// `Flameshot::recordVideo`: only once the user opted in to video.
pub(crate) fn record_video<R: Runtime>(app: &AppHandle<R>) {
    let current = settings::load(&app.state::<DaemonState>().settings_path);
    if !current.video_capture_enabled() {
        notify::system_message(
            app,
            &current,
            "Video capture is off. Turn it on in Settings, under Advanced.",
        );
        return;
    }
    not_implemented("Snipping Tool recording", &());
}

/// An `ms-screenclip` recording link (Win+Shift+R): Snipping Tool's own
/// recorder. Runs in the process Windows started, without a daemon.
pub(crate) fn start_recording_from_link() -> i32 {
    not_implemented("Snipping Tool recording", &"ms-screenclip link");
    0
}

/// `Flameshot::openSavePath`.
pub(crate) fn open_save_path<R: Runtime>(app: &AppHandle<R>) {
    let current = settings::load(&app.state::<DaemonState>().settings_path);
    let Some(folder) = current.save_path() else {
        return;
    };
    if let Err(error) = Command::new("explorer.exe").arg(&folder).spawn() {
        log::error!("cannot open {}: {error}", folder.display());
    }
}
