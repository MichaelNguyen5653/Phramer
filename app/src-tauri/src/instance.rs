//! The single-instance channel between a CLI invocation and the daemon.
//!
//! A running daemon owns the lock of `tauri-plugin-single-instance`, and a
//! later process that builds the app hands its arguments over and exits
//! inside that plugin. When no daemon runs, the CLI process starts one and
//! passes the command along in an environment variable, as 15.0.0 did with
//! `PHRAMER_CAPTURE_ON_START`, rather than turn into the daemon itself.

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// `identifier` in `tauri.conf.json`; the plugin names its lock after it.
const APP_IDENTIFIER: &str = "app.phramer.desktop";

/// Carries the command a CLI process started the daemon for.
const START_COMMAND_VARIABLE: &str = "PHRAMER_COMMAND_ON_START";

/// Arguments handed to the daemon, program name first, and the directory
/// relative paths in them refer to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Forwarded {
    pub(crate) args: Vec<String>,
    pub(crate) cwd: PathBuf,
}

/// Reads and clears the command this daemon was started for, so nothing it
/// starts later (a restart) inherits it.
///
/// Must run before the process starts any thread.
pub(crate) fn take_start_command() -> Option<Forwarded> {
    let encoded = std::env::var(START_COMMAND_VARIABLE).ok()?;
    // SAFETY: called first thing in `run`, while the main thread is the only
    // thread, so nothing can read the environment concurrently.
    unsafe { std::env::remove_var(START_COMMAND_VARIABLE) };
    decode(&encoded)
}

/// Starts a detached daemon that runs `command` once it is up.
pub(crate) fn spawn_daemon(command: &Forwarded) -> io::Result<()> {
    let exe = daemon_executable(&std::env::current_exe()?);
    Command::new(exe)
        .env(START_COMMAND_VARIABLE, encode(command))
        // Its own handles: inheriting a pipe from the shell would keep the
        // shell waiting for as long as the daemon runs
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(drop)
}

/// Always the GUI binary, even when called from a `-cli` companion.
fn daemon_executable(current: &Path) -> PathBuf {
    match current.file_stem().and_then(|stem| stem.to_str()) {
        Some(stem) if stem.ends_with("-cli") => {
            let name = format!("{}.exe", &stem[..stem.len() - "-cli".len()]);
            current.with_file_name(name)
        }
        _ => current.to_owned(),
    }
}

/// Fields as `<byte length>:<text>`, the working directory first: no
/// character is reserved, so any argument survives.
fn encode(command: &Forwarded) -> String {
    let cwd = command.cwd.to_string_lossy();
    std::iter::once(cwd.as_ref())
        .chain(command.args.iter().map(String::as_str))
        .map(|field| format!("{}:{field}", field.len()))
        .collect()
}

fn decode(mut text: &str) -> Option<Forwarded> {
    let mut fields = Vec::new();
    while !text.is_empty() {
        let (length, rest) = text.split_once(':')?;
        let length: usize = length.parse().ok()?;
        fields.push(rest.get(..length)?.to_owned());
        text = rest.get(length..)?;
    }
    let mut fields = fields.into_iter();
    let cwd = PathBuf::from(fields.next()?);
    Some(Forwarded {
        args: fields.collect(),
        cwd,
    })
}

/// Name of the named mutex the single-instance plugin holds.
fn lock_name() -> String {
    format!("{APP_IDENTIFIER}-sim")
}

/// Whether a daemon holds the single-instance lock.
pub(crate) fn daemon_running() -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_FILE_NOT_FOUND, GetLastError};
    use windows_sys::Win32::System::Threading::{OpenMutexW, SYNCHRONIZATION_SYNCHRONIZE};

    let name: Vec<u16> = lock_name().encode_utf16().chain(Some(0)).collect();
    // SAFETY: `name` is a NUL-terminated UTF-16 string that outlives the
    // call; the call takes no other pointers.
    let handle = unsafe { OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, 0, name.as_ptr()) };
    if handle.is_null() {
        // Any failure other than "no such object" (access denied) still
        // means the object exists
        // SAFETY: reads the calling thread's last-error value only.
        return unsafe { GetLastError() } != ERROR_FILE_NOT_FOUND;
    }
    // SAFETY: `handle` was just returned by OpenMutexW and is closed once.
    unsafe { CloseHandle(handle) };
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lock_name_follows_the_app_identifier() {
        let config = include_str!("../tauri.conf.json");
        assert!(
            config.contains(&format!("\"identifier\": \"{APP_IDENTIFIER}\"")),
            "tauri.conf.json identifier changed; update APP_IDENTIFIER"
        );
        assert_eq!(lock_name(), "app.phramer.desktop-sim");
    }

    #[test]
    fn commands_survive_the_environment_round_trip() {
        let command = Forwarded {
            args: vec![
                "C:\\Program Files\\Phramer\\phramer.exe".into(),
                "gui".into(),
                "--path".into(),
                "a:b 12:c|d\u{1f}é".into(),
                String::new(),
            ],
            cwd: PathBuf::from("C:\\Users\\me\\Pictures"),
        };
        assert_eq!(decode(&encode(&command)), Some(command));
    }

    #[test]
    fn malformed_commands_are_dropped() {
        assert_eq!(decode("5:abc"), None);
        assert_eq!(decode("x:abc"), None);
        assert_eq!(decode(""), None);
        assert_eq!(
            decode("1:.3:gui"),
            Some(Forwarded {
                args: vec!["gui".into()],
                cwd: PathBuf::from(".")
            })
        );
    }

    #[test]
    fn the_cli_companion_starts_the_gui_binary() {
        assert_eq!(
            daemon_executable(Path::new("C:\\Phramer\\phramer-cli.exe")),
            Path::new("C:\\Phramer\\phramer.exe")
        );
        assert_eq!(
            daemon_executable(Path::new("C:\\Phramer\\phramer.exe")),
            Path::new("C:\\Phramer\\phramer.exe")
        );
    }
}
