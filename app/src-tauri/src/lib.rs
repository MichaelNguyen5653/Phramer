//! Phramer's process shell: one binary that is either the background daemon
//! (no arguments) or a command-line invocation that hands its command to the
//! daemon and exits, as 15.0.0's `main.cpp` decided by argument count.

mod actions;
mod capture;
mod cli;
mod console;
mod daemon;
mod editor;
mod handoff;
mod hotkeys;
mod instance;
mod logging;
mod notify;
mod reply;
mod restart;
mod settings;
mod snipping;
mod startup;
mod tray;
mod updater;

use std::path::PathBuf;

use cli::{CliError, Command, ConfigCommand, Parsed, Role};
use instance::Forwarded;

/// `phramer-cli.exe`: the console companion; returns the exit code.
pub fn run_cli() -> i32 {
    console::run()
}

/// Runs the process in the role its arguments give it; returns the exit
/// code.
pub fn run() -> i32 {
    // Before anything can start a thread: it edits the environment
    let start_command = instance::take_start_command();
    logging::init();
    // Before any window or capture; Tao sets the same awareness when the
    // event loop starts, which is too late for anything earlier
    if let Err(error) = phramer_capture::enable_per_monitor_dpi_awareness() {
        log::warn!("per-monitor DPI awareness unavailable: {error}");
    }

    let args: Vec<String> = std::env::args_os()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    // From `phramer-cli`: parsed without its reply pipe, forwarded with it
    let mut plain = args.clone();
    let relayed = reply::take_reply(&mut plain).is_some();
    match cli::classify(&plain) {
        Role::Daemon => daemon::run(start_command),
        Role::Recording => actions::start_recording_from_link(),
        Role::Command(command) => {
            let forwarded = if relayed { args } else { command.clone() };
            run_command(command, move |_, cwd| {
                forward(Forwarded {
                    args: forwarded,
                    cwd,
                })
            })
        }
    }
}

/// Parses `args` and carries the command out here, or hands it to `daemon`
/// with the working directory; returns the exit code.
pub(crate) fn run_command(
    args: Vec<String>,
    daemon: impl FnOnce(Vec<String>, PathBuf) -> i32,
) -> i32 {
    let cwd = std::env::current_dir().unwrap_or_default();
    let command = match cli::parse(&args, &cwd) {
        Ok(Parsed::Help(text)) => {
            print!("{text}");
            return 0;
        }
        Ok(Parsed::Version) => {
            println!("Phramer v{}", phramer_core::VERSION);
            return 0;
        }
        Ok(Parsed::Command(command)) => command,
        Err(error) => return report(&error),
    };
    if command.runs_in_daemon() {
        return daemon(args, cwd);
    }
    match command {
        Command::Config(ConfigCommand::Check) => actions::check_configuration(),
        Command::Config(ConfigCommand::Set(changes)) => {
            if let Err(error) = settings::apply_config_changes(&changes) {
                eprintln!("phramer: error: {error}");
                return 1;
            }
        }
        _ => {}
    }
    0
}

fn report(error: &CliError) -> i32 {
    eprintln!("phramer: error: {error}");
    eprintln!("{}", cli::GENERAL_ERROR);
    error.exit_code()
}

/// Hands the command to the running daemon, or starts one for it.
fn forward(command: Forwarded) -> i32 {
    if instance::daemon_running() {
        // The single-instance plugin sends this process's arguments to the
        // daemon and exits while the app is being built. Should the daemon
        // have quit in the meantime, this process becomes the daemon and
        // runs the command itself.
        return daemon::run(Some(command));
    }
    match instance::spawn_daemon(&command) {
        Ok(()) => 0,
        Err(error) => {
            log::warn!("cannot start the daemon ({error}); running as the daemon instead");
            daemon::run(Some(command))
        }
    }
}
