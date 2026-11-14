//! The background process: tray icon, global hotkeys, the receiving end of
//! forwarded commands. Starts with no window and keeps running when the last
//! window closes.

use std::path::PathBuf;

use tauri::{App, AppHandle, Manager, RunEvent, Runtime};

use crate::cli::{self, Command, ConfigCommand, Parsed, Role};
use crate::instance::Forwarded;
use crate::reply::{self, EXIT_OK, Reply};
use crate::restart::{self, Restart};
use crate::{actions, editor, hotkeys, settings, settings_ui, startup, tray, updater, welcome};
use crate::{handoff, launcher_ui, ocr_ui};

/// Shared with the actions.
pub(crate) struct DaemonState {
    pub(crate) settings_path: PathBuf,
}

/// Runs the daemon until it quits; returns the exit code. `start` is a
/// command to carry out once it is up: the one a CLI process started it
/// for, or this process's own when it found no daemon to forward to.
pub(crate) fn run(start: Option<Forwarded>) -> i32 {
    let settings_path = match settings::path() {
        Ok(path) => path,
        Err(error) => {
            log::error!("{error}");
            return 1;
        }
    };
    let restart = Restart::default();
    let managed_restart = restart.clone();

    let built = tauri::Builder::default()
        // First, so a second instance hands over its arguments and exits
        // before anything else is set up
        .plugin(tauri_plugin_single_instance::init(|app, args, cwd| {
            receive(
                app,
                Forwarded {
                    args,
                    cwd: PathBuf::from(cwd),
                },
            );
        }))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        // The editor's Save As dialog, called from Rust only
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            editor::editor_image,
            editor::editor_ready,
            editor::editor_settings,
            editor::editor_set,
            editor::editor_export,
            editor::editor_empty_state,
            editor::editor_font_families,
            editor::editor_font_faces,
            editor::editor_save_all_begin,
            editor::editor_save_all_end,
            settings_ui::ui_ready,
            settings_ui::settings_snapshot,
            settings_ui::settings_set,
            settings_ui::settings_reset,
            settings_ui::settings_reset_all,
            settings_ui::settings_set_shortcut,
            settings_ui::settings_pause_hotkeys,
            settings_ui::settings_print_screen,
            settings_ui::settings_preview_pattern,
            settings_ui::settings_choose_folder,
            settings_ui::settings_export,
            settings_ui::settings_import,
            settings_ui::settings_open,
            welcome::welcome_state,
            welcome::welcome_reached_end,
            ocr_ui::ocr_init,
            ocr_ui::ocr_languages,
            ocr_ui::ocr_recognize,
            ocr_ui::ocr_set_layout,
            ocr_ui::ocr_set_language,
            ocr_ui::ocr_copy,
            launcher_ui::launcher_init,
            launcher_ui::launcher_apps,
            launcher_ui::launcher_icons,
            launcher_ui::launcher_launch,
            launcher_ui::launcher_open_with,
            launcher_ui::launcher_set_keep_open,
        ])
        .setup(move |app| {
            app.manage(managed_restart);
            setup(app, settings_path, start);
            Ok(())
        })
        .build(tauri::generate_context!());
    let app = match built {
        Ok(app) => app,
        Err(error) => {
            log::error!("cannot start: {error}");
            return 1;
        }
    };

    let code = app.run_return(|app, event| match event {
        // Closing the last window must not end the daemon; an explicit
        // exit (Quit, Restart) carries a code
        RunEvent::ExitRequested {
            code: None, api, ..
        } => api.prevent_exit(),
        // Releases the single-instance lock before a restart starts the
        // next daemon
        RunEvent::Exit => tauri_plugin_single_instance::destroy(app),
        _ => {}
    });

    if let Err(error) = restart::relaunch_if_requested(&restart) {
        log::error!("restart failed: {error}");
    }
    code
}

fn setup(app: &mut App, settings_path: PathBuf, start: Option<Forwarded>) {
    let handle = app.handle().clone();
    let mut current = settings::load(&settings_path);
    if let Err(error) = startup::apply_default_startup_launch(&mut current, &settings_path) {
        log::error!("cannot apply the default launch at sign-in: {error}");
    }
    log::info!("settings: {}", settings_path.display());
    app.manage(DaemonState {
        settings_path: settings_path.clone(),
    });
    app.manage(hotkeys::Registered::default());
    app.manage(editor::Editors::default());
    app.manage(welcome::Welcome::default());
    app.manage(ocr_ui::OcrWindows::default());
    app.manage(launcher_ui::Launchers::default());
    handoff::set_app(&handle);
    editor::listen(&handle);

    if let Err(error) = tray::create(&handle, &current) {
        log::error!("cannot create the tray icon: {error}");
    }
    hotkeys::apply(&handle, &current);
    updater::start(&handle);
    welcome::show_if_due(&handle);

    let watcher = handle.clone();
    settings::watch(settings_path, move || {
        let target = watcher.clone();
        watcher
            .run_on_main_thread(move || settings_changed(&target))
            .is_ok()
    });

    if let Some(start) = start {
        // After setup, from the event loop, as 15.0.0 queued it
        let target = handle.clone();
        if let Err(error) = handle.run_on_main_thread(move || receive(&target, start)) {
            log::error!("cannot run the start command: {error}");
        }
    }
    log::info!("daemon started");
}

/// `ConfigHandler::fileChanged` in 15.0.0's daemon and tray.
pub(crate) fn settings_changed<R: Runtime>(app: &AppHandle<R>) {
    let current = settings::load(&app.state::<DaemonState>().settings_path);
    if let Err(error) = tray::refresh(app, &current) {
        log::error!("cannot update the tray icon: {error}");
    }
    hotkeys::apply(app, &current);
    settings_ui::changed(app);
    editor::settings_changed(app);
}

/// Arguments from another process (or this one's start command).
fn receive<R: Runtime>(app: &AppHandle<R>, forwarded: Forwarded) {
    let mut args = forwarded.args;
    // Connected at once, so `phramer-cli` knows the command arrived
    let reply = reply::take_reply(&mut args).and_then(|name| Reply::connect(&name));
    let args = match cli::classify(&args) {
        // A second plain start: 15.0.0's second daemon quit silently
        Role::Daemon => return,
        Role::Recording => {
            actions::start_recording_from_link();
            return;
        }
        Role::Command(args) => args,
    };
    match cli::parse(&args, &forwarded.cwd) {
        Ok(Parsed::Command(command)) => {
            // Deferred: the sender waits on this call, and the action can
            // open windows and wait on them
            let target = app.clone();
            if let Err(error) = app.run_on_main_thread(move || dispatch(&target, command, reply)) {
                log::error!("cannot run forwarded command: {error}");
            }
        }
        other => log::warn!("ignoring forwarded arguments {args:?}: {other:?}"),
    }
}

fn dispatch<R: Runtime>(app: &AppHandle<R>, command: Command, reply: Option<Reply>) {
    log::info!("command: {command:?}");
    let reply = match command {
        Command::Gui(request) => return actions::capture_gui_replying(app, request, reply),
        Command::Screen(request) => return actions::capture_screen(app, request, reply),
        Command::Full(request) => return actions::capture_full(app, request, reply),
        _ => reply,
    };
    if let Some(reply) = reply {
        reply.finish(EXIT_OK);
    }
    match command {
        Command::Gui(_) | Command::Screen(_) | Command::Full(_) => {}
        Command::Launcher => actions::open_launcher(app),
        Command::Config(ConfigCommand::Open) => actions::open_configuration(app),
        Command::Config(ConfigCommand::Check) => {
            actions::check_configuration();
        }
        Command::Config(ConfigCommand::Set(changes)) => {
            if let Err(error) = settings::apply_config_changes(&changes) {
                log::error!("{error}");
            }
        }
    }
}
