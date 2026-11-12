//! The background process: tray icon, global hotkeys, the receiving end of
//! forwarded commands. Starts with no window and keeps running when the last
//! window closes.

use std::path::PathBuf;

use tauri::{App, AppHandle, Manager, RunEvent, Runtime};

use crate::cli::{self, Command, ConfigCommand, Parsed, Role};
use crate::instance::Forwarded;
use crate::restart::{self, Restart};
use crate::{actions, hotkeys, settings, startup, tray};

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

    if let Err(error) = tray::create(&handle, &current) {
        log::error!("cannot create the tray icon: {error}");
    }
    hotkeys::apply(&handle, &current);

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
fn settings_changed<R: Runtime>(app: &AppHandle<R>) {
    let current = settings::load(&app.state::<DaemonState>().settings_path);
    if let Err(error) = tray::refresh(app, &current) {
        log::error!("cannot update the tray icon: {error}");
    }
    hotkeys::apply(app, &current);
}

/// Arguments from another process (or this one's start command).
fn receive<R: Runtime>(app: &AppHandle<R>, forwarded: Forwarded) {
    let args = match cli::classify(&forwarded.args) {
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
            if let Err(error) = app.run_on_main_thread(move || dispatch(&target, command)) {
                log::error!("cannot run forwarded command: {error}");
            }
        }
        other => log::warn!("ignoring forwarded arguments {args:?}: {other:?}"),
    }
}

fn dispatch<R: Runtime>(app: &AppHandle<R>, command: Command) {
    log::info!("command: {command:?}");
    match command {
        Command::Gui(request) => actions::capture_gui(app, request),
        Command::Screen(request) => actions::capture_screen(app, request),
        Command::Full(request) => actions::capture_full(app, request),
        Command::Launcher => actions::open_launcher(app),
        Command::Config(ConfigCommand::Open) => actions::open_configuration(app),
        Command::Config(ConfigCommand::Check) => actions::check_configuration(),
        Command::Config(ConfigCommand::Set(changes)) => {
            if let Err(error) = settings::apply_config_changes(&changes) {
                log::error!("{error}");
            }
        }
    }
}
