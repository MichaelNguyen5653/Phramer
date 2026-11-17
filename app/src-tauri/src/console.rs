//! `phramer-cli.exe`: the console companion. 15.0.0's `windows-cli.cpp`
//! ran `phramer.exe` and copied its output, because a GUI-subsystem
//! process has no console to print to.
//!
//! Here the command is parsed in this process (help, version and errors
//! print directly), then handed to `phramer.exe` to forward to the daemon
//! (or start one) with the name of a pipe this process listens on. The
//! daemon answers there with what 15.0.0 printed and the exit code.

use std::fs::File;
use std::io::{self, Write};
use std::os::windows::io::{FromRawHandle, RawHandle};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_PIPE_CONNECTED, GetLastError, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_INBOUND};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
    PIPE_TYPE_BYTE, PIPE_WAIT,
};

use crate::reply::{EXIT_GENERAL, Frame, PIPE_PREFIX, REPLY_ARGUMENT, decode};
use crate::{instance, run_command};

/// How long the daemon has to take the command: a cold start included.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(60);
const PIPE_BUFFER: u32 = 64 * 1024;

/// Runs `phramer-cli` with this process's arguments; returns the exit code.
pub fn run() -> i32 {
    let args: Vec<String> = std::env::args_os()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    if args.len() < 2 {
        // As 15.0.0: no arguments starts the daemon and does not wait
        println!("Starting phramer in daemon mode");
        return match gui_binary().and_then(|exe| spawn_detached(&exe, &[])) {
            Ok(_) => 0,
            Err(error) => {
                eprintln!("phramer: error: cannot start phramer: {error}");
                EXIT_GENERAL
            }
        };
    }
    run_command(args, relay)
}

/// Makes this process's console handles private. Windows hands every
/// inheritable handle to a child, `Stdio::null` or not: the daemon would
/// hold the caller's output pipe open, and a script reading it would wait
/// until the daemon quits.
fn keep_console_handles() {
    use windows_sys::Win32::Foundation::{HANDLE_FLAG_INHERIT, SetHandleInformation};
    use windows_sys::Win32::System::Console::{
        GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
    };

    for which in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
        // SAFETY: reads this process's standard handle.
        let handle = unsafe { GetStdHandle(which) };
        if !handle.is_null() && handle != INVALID_HANDLE_VALUE {
            // SAFETY: clears a flag of a handle this process owns; this
            // process keeps using it.
            unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0) };
        }
    }
}

fn gui_binary() -> io::Result<PathBuf> {
    Ok(instance::daemon_executable(&std::env::current_exe()?))
}

fn spawn_detached(exe: &Path, args: &[String]) -> io::Result<Child> {
    keep_console_handles();
    Command::new(exe)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
}

/// A name no other pipe has: this process and the time.
fn pipe_name() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    format!("{PIPE_PREFIX}{}-{nanos}", std::process::id())
}

/// The server end of a reply pipe, closed when dropped unless taken.
struct Pipe(HANDLE);

impl Pipe {
    fn create(name: &str) -> io::Result<Self> {
        let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        // SAFETY: `wide` is NUL-terminated and outlives the call; one
        // inbound byte pipe, local clients only, default security (this
        // user, administrators and SYSTEM).
        let handle = unsafe {
            CreateNamedPipeW(
                wide.as_ptr(),
                PIPE_ACCESS_INBOUND | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                0,
                PIPE_BUFFER,
                0,
                std::ptr::null(),
            )
        };
        if handle == INVALID_HANDLE_VALUE || handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(handle))
    }

    /// The pipe as a file to read from; ownership moves to it.
    fn into_file(self) -> File {
        let handle = self.0;
        std::mem::forget(self);
        // SAFETY: `handle` is an open pipe handle owned by `self`, which is
        // forgotten, so the file is now its only owner.
        unsafe { File::from_raw_handle(handle as RawHandle) }
    }
}

impl Drop for Pipe {
    fn drop(&mut self) {
        // SAFETY: the handle is open and owned by this value; closed once.
        unsafe { CloseHandle(self.0) };
    }
}

/// Waits for a client on the pipe whose handle is `raw`.
fn wait_for_client(raw: usize) -> io::Result<()> {
    // SAFETY: `raw` is the pipe's handle, open for the life of this wait
    // (the process exits rather than close it under the wait).
    let connected = unsafe { ConnectNamedPipe(raw as HANDLE, std::ptr::null_mut()) };
    // SAFETY: reads this thread's last-error value.
    if connected != 0 || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Hands `args` to the daemon through `phramer.exe` and relays its answer.
fn relay(args: Vec<String>, cwd: PathBuf) -> i32 {
    match try_relay(args, &cwd) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("phramer: error: {error}");
            EXIT_GENERAL
        }
    }
}

fn try_relay(args: Vec<String>, cwd: &Path) -> io::Result<i32> {
    let name = pipe_name();
    let pipe = Pipe::create(&name)?;
    let mut forwarded: Vec<String> = args.into_iter().skip(1).collect();
    forwarded.push(format!("{REPLY_ARGUMENT}{name}"));
    keep_console_handles();
    let mut child = Command::new(gui_binary()?)
        .args(&forwarded)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    let (sender, receiver) = mpsc::channel();
    let raw = pipe.0 as usize;
    std::thread::spawn(move || {
        let _ = sender.send(wait_for_client(raw));
    });
    let started = Instant::now();
    loop {
        match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(connected) => {
                connected?;
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(io::Error::other("the reply pipe failed"));
            }
        }
        // `phramer.exe` exits 0 once it handed the command over
        if let Some(status) = child.try_wait()?
            && !status.success()
        {
            return Ok(status.code().unwrap_or(EXIT_GENERAL));
        }
        if started.elapsed() > CONNECT_TIMEOUT {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "the Phramer daemon did not take the command",
            ));
        }
    }

    let mut input = io::BufReader::new(pipe.into_file());
    let mut stdout = io::stdout().lock();
    let mut stderr = io::stderr().lock();
    loop {
        match decode(&mut input) {
            Ok(Some(Frame::Stdout(bytes))) => {
                stdout.write_all(&bytes)?;
            }
            Ok(Some(Frame::Stderr(bytes))) => {
                let _ = stderr.write_all(&bytes);
            }
            Ok(Some(Frame::Exit(code))) => {
                stdout.flush()?;
                return Ok(code);
            }
            // The daemon went away mid-capture
            Ok(None) | Err(_) => {
                stdout.flush()?;
                return Err(io::Error::other(
                    "the Phramer daemon closed the connection without an answer",
                ));
            }
        }
    }
}
