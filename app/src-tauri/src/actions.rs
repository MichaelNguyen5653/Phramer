//! What the tray, the hotkeys and forwarded commands do. Everything that
//! needs a feature not built yet is a stub here that logs
//! "not implemented yet", for the later phases to fill in.

use std::path::PathBuf;
use std::process::Command;

use tauri::{AppHandle, Manager, Runtime};

use crate::capture::{self, OverlayPlan, Placement};
use crate::cli::{FullRequest, GuiRequest, ScreenRequest};
use crate::daemon::DaemonState;
use crate::reply::Reply;
use crate::{notify, settings, settings_ui, snipping, updater, welcome};

fn not_implemented(feature: &str, detail: &dyn std::fmt::Debug) {
    log::info!("{feature}: not implemented yet ({detail:?})");
}

/// `Flameshot::gui` / `requestCapture` in graphical mode: the capture
/// overlay.
pub(crate) fn capture_gui<R: Runtime>(app: &AppHandle<R>, request: GuiRequest) {
    capture_gui_replying(app, request, None);
}

/// [`capture_gui`] for a command that `reply` answers.
pub(crate) fn capture_gui_replying<R: Runtime>(
    app: &AppHandle<R>,
    request: GuiRequest,
    reply: Option<Reply>,
) {
    capture::overlay(
        app,
        OverlayPlan {
            delay_ms: request.delay_ms,
            placement: request.monitor.map_or(Placement::Settings, Placement::Monitor),
            initial_selection: request.initial_selection,
            tasks: request.tasks,
        },
        reply,
    );
}

/// `Flameshot::screen`: one monitor without the overlay, or the overlay
/// limited to it with `--edit`.
pub(crate) fn capture_screen<R: Runtime>(
    app: &AppHandle<R>,
    request: ScreenRequest,
    reply: Option<Reply>,
) {
    if request.edit {
        let initial_selection = request
            .region
            .map_or(crate::cli::InitialSelection::None, crate::cli::InitialSelection::Region);
        capture::overlay(
            app,
            OverlayPlan {
                delay_ms: request.delay_ms,
                placement: request.monitor.map_or(Placement::UnderCursor, Placement::Monitor),
                initial_selection,
                tasks: request.tasks,
            },
            reply,
        );
    } else {
        capture::screen(
            app,
            request.delay_ms,
            request.monitor,
            request.region,
            request.tasks,
            reply,
        );
    }
}

/// `Flameshot::full`: every monitor.
pub(crate) fn capture_full<R: Runtime>(
    app: &AppHandle<R>,
    request: FullRequest,
    reply: Option<Reply>,
) {
    capture::full(app, request.delay_ms, request.tasks, reply);
}

/// `Flameshot::launcher`.
pub(crate) fn open_launcher<R: Runtime>(_app: &AppHandle<R>) {
    not_implemented("capture launcher", &());
}

/// `Flameshot::config`: the settings window.
pub(crate) fn open_configuration<R: Runtime>(app: &AppHandle<R>) {
    settings_ui::open(app);
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
pub(crate) fn check_for_updates<R: Runtime>(app: &AppHandle<R>) {
    updater::check_now(app);
}

/// `WelcomeTour::showNow`: the welcome, which can be closed at any step.
pub(crate) fn show_whats_new<R: Runtime>(app: &AppHandle<R>) {
    welcome::open(app, false);
}

/// `Flameshot::info`.
pub(crate) fn show_about<R: Runtime>(_app: &AppHandle<R>) {
    not_implemented("about window", &());
}

/// The last file saved this session; kept in memory only, by design.
pub(crate) fn last_capture() -> Option<PathBuf> {
    capture::last_saved()
}

/// `FileHandoff::showInFolder(lastSaved())`.
pub(crate) fn show_last_capture_in_folder<R: Runtime>(_app: &AppHandle<R>) {
    let Some(path) = last_capture() else {
        return;
    };
    if let Err(error) = phramer_overlay::show_in_folder(&path) {
        log::error!("cannot show {}: {error}", path.display());
    }
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
    if !snipping::is_available() {
        notify::warning_message(
            app,
            &current,
            "Video capture needs the Windows Snipping Tool, which is not installed on this \
             computer.",
        );
        return;
    }
    let target = app.clone();
    snipping::start_recording_async(move |launched| {
        if launched {
            return;
        }
        // The notification belongs to the main thread
        let main = target.clone();
        let posted = target.run_on_main_thread(move || {
            let current = settings::load(&main.state::<DaemonState>().settings_path);
            notify::warning_message(
                &main,
                &current,
                "Windows did not allow the Snipping Tool to start. It may be blocked on this \
                 computer.",
            );
        });
        if let Err(error) = posted {
            log::error!("cannot report the Snipping Tool failure: {error}");
        }
    });
}

/// An `ms-screenclip` recording link (Win+Shift+R): Snipping Tool's own
/// recorder. Runs in the process Windows started, without a daemon.
pub(crate) fn start_recording_from_link() -> i32 {
    if snipping::start_recording() { 0 } else { 1 }
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
