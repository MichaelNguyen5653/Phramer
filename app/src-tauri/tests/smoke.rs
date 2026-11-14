//! Live smoke tests of the daemon shell and the capture overlay. Ignored by
//! default: they start real processes, show a tray icon and the overlay, and
//! read the real settings file. They take turns (one daemon at a time).
//!
//! ```text
//! cargo test -p phramer --test smoke -- --ignored --nocapture
//! ```
//!
//! Runs the binary in `PHRAMER_SMOKE_EXE` when set (the release build from
//! `npm run tauri build -- --no-bundle`), otherwise the debug build cargo
//! made for this test. The settings file and the startup registry entries
//! are snapshotted first and must be unchanged at the end; if not, they are
//! restored and the test fails. Every process is started with
//! `PHRAMER_NO_WELCOME`, so the first-run welcome window stays away whatever
//! the settings file says.

#![cfg(windows)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_SUCCESS, HANDLE, HWND, LPARAM, RECT, WAIT_OBJECT_0,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
};
use windows_sys::Win32::System::Threading::{
    GetExitCodeProcess, OpenMutexW, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_SYNCHRONIZE, QueryFullProcessImageNameW, SYNCHRONIZATION_SYNCHRONIZE,
    WaitForSingleObject,
};
use windows_sys::Win32::Foundation::POINT;
use windows_sys::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromPoint,
};
use windows_sys::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
use windows_sys::Win32::UI::Shell::{NOTIFYICONIDENTIFIER, Shell_NotifyIconGetRect};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    ENDSESSION_CLOSEAPP, EnumWindows, GWL_EXSTYLE, GetClassNameW, GetCursorPos, GetSystemMetrics,
    GetWindowLongW, GetWindowRect, GetWindowThreadProcessId, IsWindowVisible, PostMessageW,
    SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, WM_ENDSESSION, WM_KEYDOWN, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowTextW, WM_CLOSE};
use windows_sys::Win32::UI::WindowsAndMessaging::{WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE};

const LOCK: &str = "app.phramer.desktop-sim";
/// The overlay's window class (`phramer-overlay`).
const OVERLAY_CLASS: &str = "PhramerCaptureOverlay";
/// Command to first frame, as the daemon measures and logs it.
const FIRST_FRAME_BUDGET_MS: u128 = 100;
/// Command to toolbar, for a selection made as soon as the overlay shows:
/// the 150 ms feedback rule of `docs/design/raycast-notes.md`.
const TOOLBAR_BUDGET_MS: u128 = 150;

/// One live daemon at a time: the tests share the single-instance lock.
static LIVE: Mutex<()> = Mutex::new(());
const RUN_KEY: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run";
const APP_PATHS_KEY: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\App Paths\\phramer.exe";

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

fn exe() -> PathBuf {
    std::env::var_os("PHRAMER_SMOKE_EXE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_phramer")))
}

fn wait_until(timeout: Duration, mut done: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if done() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    done()
}

fn lock_exists() -> bool {
    let name = wide(LOCK);
    // SAFETY: `name` is NUL-terminated and outlives the call.
    let handle = unsafe { OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, 0, name.as_ptr()) };
    if handle.is_null() {
        return false;
    }
    // SAFETY: a handle just opened, closed once.
    unsafe { CloseHandle(handle) };
    true
}

fn image_path(pid: u32) -> Option<String> {
    // SAFETY: plain query; the handle is closed below.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return None;
    }
    let mut buffer = [0u16; 1024];
    let mut length = buffer.len() as u32;
    // SAFETY: `buffer` holds `length` characters.
    let ok = unsafe { QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut length) };
    // SAFETY: opened above, closed once.
    unsafe { CloseHandle(process) };
    (ok != 0).then(|| String::from_utf16_lossy(&buffer[..length as usize]))
}

/// Processes running exactly `exe`.
fn processes_of(exe: &Path) -> Vec<u32> {
    let wanted = exe.to_string_lossy().to_lowercase();
    // SAFETY: snapshot of all processes; closed below.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    let mut pids = Vec::new();
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    // SAFETY: `entry.dwSize` is set as the API requires.
    let mut more = unsafe { Process32FirstW(snapshot, &mut entry) };
    while more != 0 {
        if image_path(entry.th32ProcessID).is_some_and(|path| path.to_lowercase() == wanted) {
            pids.push(entry.th32ProcessID);
        }
        // SAFETY: as above.
        more = unsafe { Process32NextW(snapshot, &mut entry) };
    }
    // SAFETY: the snapshot handle is closed once.
    unsafe { CloseHandle(snapshot) };
    pids
}

#[derive(Debug, Clone)]
struct Window {
    hwnd: isize,
    class: String,
    visible: bool,
    tool_window: bool,
    topmost: bool,
    rect: (i32, i32, i32, i32),
}

impl Window {
    /// Something a user could see: shown, not a tool window, with an area.
    fn user_visible(&self) -> bool {
        let (left, top, right, bottom) = self.rect;
        self.visible && !self.tool_window && right > left && bottom > top
    }
}

unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> i32 {
    // SAFETY: `lparam` is the `&mut Vec` passed to EnumWindows below, alive
    // for the whole enumeration.
    let found = unsafe { &mut *(lparam as *mut Vec<isize>) };
    found.push(hwnd as isize);
    1
}

fn windows_of(pid: u32) -> Vec<Window> {
    let mut all: Vec<isize> = Vec::new();
    // SAFETY: `collect` only pushes into `all`, which outlives the call.
    unsafe { EnumWindows(Some(collect), &mut all as *mut Vec<isize> as LPARAM) };
    all.into_iter()
        .filter_map(|raw| {
            let hwnd = raw as HWND;
            let mut owner = 0;
            // SAFETY: `hwnd` came from EnumWindows; a stale one just fails.
            unsafe { GetWindowThreadProcessId(hwnd, &mut owner) };
            if owner != pid {
                return None;
            }
            let mut class = [0u16; 256];
            // SAFETY: `class` holds 256 characters.
            let length = unsafe { GetClassNameW(hwnd, class.as_mut_ptr(), 256) };
            let mut rect = RECT::default();
            // SAFETY: `rect` is a valid out-parameter.
            unsafe { GetWindowRect(hwnd, &mut rect) };
            // SAFETY: read-only queries on a window handle.
            let (visible, ex_style) =
                unsafe { (IsWindowVisible(hwnd) != 0, GetWindowLongW(hwnd, GWL_EXSTYLE)) };
            Some(Window {
                hwnd: raw,
                class: String::from_utf16_lossy(&class[..length.max(0) as usize]),
                visible,
                tool_window: ex_style as u32 & WS_EX_TOOLWINDOW != 0,
                topmost: ex_style as u32 & WS_EX_TOPMOST != 0,
                rect: (rect.left, rect.top, rect.right, rect.bottom),
            })
        })
        .collect()
}

/// The shell's id and rectangle of the icon a `tray-icon` window owns.
fn tray_icon_rect(window: &Window) -> Option<(u32, RECT)> {
    (0..=64).find_map(|id| {
        let identifier = NOTIFYICONIDENTIFIER {
            cbSize: size_of::<NOTIFYICONIDENTIFIER>() as u32,
            hWnd: window.hwnd as HWND,
            uID: id,
            ..Default::default()
        };
        let mut rect = RECT::default();
        // SAFETY: both structures are initialised and outlive the call.
        let result = unsafe { Shell_NotifyIconGetRect(&identifier, &mut rect) };
        (result == 0).then_some((id, rect))
    })
}

fn read_registry(key: &str, value: &str) -> Option<String> {
    let (key, value) = (wide(key), wide(value));
    let mut buffer = [0u16; 2048];
    let mut size = (buffer.len() * 2) as u32;
    // SAFETY: `buffer` holds `size` bytes.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buffer.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    Some(String::from_utf16_lossy(&buffer[..end]))
}

fn restore_registry(key: &str, value: &str, data: &Option<String>) {
    let (key, value) = (wide(key), wide(value));
    match data {
        Some(data) => {
            let data = wide(data);
            // SAFETY: all strings NUL-terminated; `data` holds the byte count given.
            unsafe {
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    key.as_ptr(),
                    value.as_ptr(),
                    REG_SZ,
                    data.as_ptr().cast(),
                    (data.len() * 2) as u32,
                )
            };
        }
        None => {
            // SAFETY: both strings NUL-terminated.
            unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr()) };
        }
    }
}

/// The settings file and startup registry entries, compared at the end.
#[derive(Debug, PartialEq)]
struct UserState {
    settings: Option<Vec<u8>>,
    run_value: Option<String>,
    app_paths: Option<String>,
}

fn settings_file() -> PathBuf {
    PathBuf::from(std::env::var_os("APPDATA").unwrap_or_default()).join("phramer\\phramer.ini")
}

fn user_state() -> UserState {
    UserState {
        settings: fs::read(settings_file()).ok(),
        run_value: read_registry(RUN_KEY, "Phramer"),
        app_paths: read_registry(APP_PATHS_KEY, "Path"),
    }
}

fn restore(state: &UserState) {
    match &state.settings {
        Some(bytes) => {
            let _ = fs::write(settings_file(), bytes);
        }
        None => {
            let _ = fs::remove_file(settings_file());
        }
    }
    restore_registry(RUN_KEY, "Phramer", &state.run_value);
    restore_registry(APP_PATHS_KEY, "Path", &state.app_paths);
}

/// Stops a daemon the way Restart Manager and sign-out do.
fn end_session(pid: u32) -> bool {
    let target = windows_of(pid)
        .into_iter()
        .find(|window| window.class == "Tao Thread Event Target");
    let Some(target) = target else {
        return false;
    };
    // SAFETY: posting a message to a window handle; a stale one just fails.
    unsafe {
        PostMessageW(
            target.hwnd as HWND,
            WM_ENDSESSION,
            1,
            ENDSESSION_CLOSEAPP as LPARAM,
        ) != 0
    }
}

/// Waits for `pid` to exit; its exit code.
fn wait_for_exit(pid: u32, timeout: Duration) -> Option<u32> {
    // SAFETY: plain open; closed below.
    let process: HANDLE = unsafe {
        OpenProcess(
            PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
            0,
            pid,
        )
    };
    if process.is_null() {
        return None;
    }
    // SAFETY: `process` is open with SYNCHRONIZE.
    let waited = unsafe { WaitForSingleObject(process, timeout.as_millis() as u32) };
    let mut code = 0;
    // SAFETY: as above.
    let got = unsafe { GetExitCodeProcess(process, &mut code) };
    // SAFETY: closed once.
    unsafe { CloseHandle(process) };
    (waited == WAIT_OBJECT_0 && got != 0).then_some(code)
}

/// Kills what the test started and puts the user's settings and registry
/// entries back if the test fails half way.
struct Cleanup {
    exe: PathBuf,
    pids: Vec<u32>,
    before: UserState,
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        for pid in &self.pids {
            // Only processes still running this binary: a pid can be reused
            if processes_of(&self.exe).contains(pid) {
                let _ = Command::new("taskkill")
                    .args(["/F", "/PID", &pid.to_string()])
                    .output();
            }
        }
        if user_state() != self.before {
            restore(&self.before);
            eprintln!("settings file or startup registry entries changed; restored them");
        }
    }
}

/// Checks the daemon; returns its tray-icon window.
fn check_daemon(exe: &Path, pid: u32) -> Window {
    let running = processes_of(exe);
    println!("processes running {}: {running:?}", exe.display());
    assert_eq!(running, [pid], "exactly one daemon process");

    let windows = windows_of(pid);
    for window in &windows {
        println!("  top-level window: {window:?}");
    }
    let shown: Vec<_> = windows.iter().filter(|w| w.user_visible()).collect();
    assert!(shown.is_empty(), "visible windows: {shown:?}");
    assert!(
        !windows.iter().any(|w| w.class.contains("Tauri") || w.class.contains("WebView")),
        "a webview window was created"
    );

    let tray = windows
        .iter()
        .find(|w| w.class == "tray_icon_app")
        .expect("no tray-icon window");
    let (id, rect) = tray_icon_rect(tray).expect("the shell does not know the tray icon");
    println!(
        "tray icon: window {:#x}, id {id}, at ({}, {})-({}, {})",
        tray.hwnd, rect.left, rect.top, rect.right, rect.bottom
    );
    Window {
        hwnd: tray.hwnd,
        class: tray.class.clone(),
        visible: tray.visible,
        tool_window: tray.tool_window,
        topmost: tray.topmost,
        rect: tray.rect,
    }
}

/// The daemon's overlay windows that are showing.
fn overlays(pid: u32) -> Vec<Window> {
    windows_of(pid)
        .into_iter()
        .filter(|window| window.class == OVERLAY_CLASS && window.visible)
        .collect()
}

/// Waits for an overlay covering `monitor` exactly; returns it and how long
/// it took.
fn wait_for_overlay(
    pid: u32,
    monitor: (i32, i32, i32, i32),
    timeout: Duration,
) -> Option<(Window, Duration)> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Some(window) = overlays(pid).into_iter().find(|window| window.rect == monitor) {
            return Some((window, start.elapsed()));
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    None
}

/// Sends Esc to an overlay and waits for every overlay window to go.
fn close_overlay(pid: u32, overlay: &Window) {
    // SAFETY: posting a message to a window handle; a stale one just fails.
    let posted =
        unsafe { PostMessageW(overlay.hwnd as HWND, WM_KEYDOWN, usize::from(VK_ESCAPE), 0) };
    assert_ne!(posted, 0, "Esc could not be posted");
    assert!(
        wait_until(Duration::from_secs(5), || windows_of(pid)
            .iter()
            .all(|window| window.class != OVERLAY_CLASS)),
        "an overlay window was left behind: {:?}",
        windows_of(pid)
    );
}

/// Makes this test process see physical pixels, as the daemon does.
fn per_monitor_dpi_aware() {
    // SAFETY: a constant context; failing only means it was already set.
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
}

/// The monitor under the cursor, physical (left, top, right, bottom).
fn monitor_under_cursor() -> (i32, i32, i32, i32) {
    let mut cursor = POINT::default();
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: valid out-parameters; the monitor handle comes from the system.
    unsafe {
        GetCursorPos(&mut cursor);
        let monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTOPRIMARY);
        GetMonitorInfoW(monitor, &mut info);
    }
    let rect = info.rcMonitor;
    (rect.left, rect.top, rect.right, rect.bottom)
}

/// The first-frame time the daemon logged last, in milliseconds.
fn logged_first_frame(log: &str) -> Option<u128> {
    log.lines()
        .rev()
        .filter_map(|line| line.split("first frame on screen ").nth(1))
        .find_map(|rest| rest.split(' ').next()?.parse().ok())
}

/// The toolbar time the daemon logged last, in milliseconds.
fn logged_toolbar(log: &str) -> Option<u128> {
    log.lines()
        .rev()
        .filter_map(|line| line.split("toolbar on screen ").nth(1))
        .find_map(|rest| rest.split(' ').next()?.parse().ok())
}

/// Drags out a selection on an overlay with posted mouse messages, in its
/// client (physical) coordinates.
fn select(overlay: &Window, from: (i32, i32), to: (i32, i32)) {
    // `MK_LBUTTON`
    const LEFT: usize = 0x0001;
    let at = |(x, y): (i32, i32)| ((y as u32 & 0xffff) << 16 | (x as u32 & 0xffff)) as LPARAM;
    for (message, wparam, point) in [
        (WM_MOUSEMOVE, 0, from),
        (WM_LBUTTONDOWN, LEFT, from),
        (WM_MOUSEMOVE, LEFT, to),
        (WM_LBUTTONUP, 0, to),
    ] {
        // SAFETY: posting a message to a window handle; a stale one just fails.
        let posted = unsafe { PostMessageW(overlay.hwnd as HWND, message, wparam, at(point)) };
        assert_ne!(posted, 0, "a mouse message could not be posted");
    }
}

/// Width and height from a PNG's IHDR.
fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.get(..8)? != b"\x89PNG\r\n\x1a\n" || bytes.get(12..16)? != b"IHDR" {
        return None;
    }
    let field = |at: usize| Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?));
    Some((field(16)?, field(20)?))
}

fn stop(pid: u32, tray: &Window) {
    assert!(end_session(pid), "no event-loop window to post to");
    let code = wait_for_exit(pid, Duration::from_secs(15));
    println!("daemon {pid} exited with {code:?}");
    assert_eq!(code, Some(0), "clean exit");
    assert!(
        wait_until(Duration::from_secs(5), || !lock_exists()),
        "single-instance lock left behind"
    );
    assert!(
        tray_icon_rect(tray).is_none(),
        "the tray icon was left in the notification area"
    );
    println!("tray icon removed");
}

fn wait_cli(mut child: Child) -> u32 {
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                println!(
                    "cli exited with {status} after {} ms",
                    start.elapsed().as_millis()
                );
                assert!(status.success());
                return child.id();
            }
            Ok(None) if start.elapsed() < Duration::from_secs(30) => {
                std::thread::sleep(Duration::from_millis(50));
            }
            _ => {
                let _ = child.kill();
                panic!("the CLI process did not exit");
            }
        }
    }
}

#[test]
#[ignore = "live: starts the daemon, shows a tray icon"]
fn daemon_tray_forwarding_and_spawn() {
    let _live = LIVE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    per_monitor_dpi_aware();
    let exe = exe();
    println!("binary: {}", exe.display());
    assert!(
        processes_of(&exe).is_empty(),
        "{} is already running",
        exe.display()
    );
    assert!(!lock_exists(), "another Phramer 16 daemon holds the lock");
    let mut cleanup = Cleanup {
        exe: exe.clone(),
        pids: Vec::new(),
        before: user_state(),
    };

    // 1. The daemon: one process, no window, a tray icon
    let log_path = std::env::temp_dir().join(format!("phramer-smoke-{}.log", std::process::id()));
    let log = fs::File::create(&log_path).expect("log file");
    let daemon = Command::new(&exe)
        .env("PHRAMER_NO_WELCOME", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log)
        .spawn()
        .expect("start the daemon");
    let daemon_pid = daemon.id();
    cleanup.pids.push(daemon_pid);
    println!("daemon pid {daemon_pid}, log {}", log_path.display());
    assert!(
        wait_until(Duration::from_secs(20), || {
            lock_exists()
                && windows_of(daemon_pid)
                    .iter()
                    .any(|w| w.class == "tray_icon_app")
        }),
        "the daemon did not come up"
    );
    // Let setup finish (hotkeys, notifications) before looking
    std::thread::sleep(Duration::from_secs(2));
    let tray = check_daemon(&exe, daemon_pid);

    // 2. `phramer gui` forwards and exits; the daemon receives it
    let cli = Command::new(&exe)
        .env("PHRAMER_NO_WELCOME", "1")
        .arg("gui")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start the CLI");
    let cli_pid = cli.id();
    cleanup.pids.push(cli_pid);
    wait_cli(cli);
    assert!(image_path(cli_pid).is_none(), "the CLI process is still there");
    assert_eq!(processes_of(&exe), [daemon_pid], "only the daemon remains");
    // It opens the overlay; Esc closes it
    let (overlay, _) =
        wait_for_overlay(daemon_pid, monitor_under_cursor(), Duration::from_secs(10))
            .expect("the daemon did not open the overlay for the forwarded gui command");
    close_overlay(daemon_pid, &overlay);

    // 3. A clean stop
    stop(daemon_pid, &tray);
    drop(daemon);
    println!("--- daemon log ---\n{}", fs::read_to_string(&log_path).unwrap_or_default());

    // 4. With no daemon, `phramer gui` starts one for the capture and exits
    let cli = Command::new(&exe)
        .env("PHRAMER_NO_WELCOME", "1")
        .arg("gui")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start the CLI");
    let cli_pid = cli.id();
    cleanup.pids.push(cli_pid);
    wait_cli(cli);
    let mut spawned = Vec::new();
    assert!(
        wait_until(Duration::from_secs(20), || {
            spawned = processes_of(&exe);
            spawned.len() == 1 && lock_exists()
        }),
        "no daemon was started: {spawned:?}"
    );
    let spawned_pid = spawned[0];
    cleanup.pids.push(spawned_pid);
    assert_ne!(spawned_pid, cli_pid);
    println!("CLI {cli_pid} started daemon {spawned_pid}");
    assert!(
        wait_until(Duration::from_secs(20), || windows_of(spawned_pid)
            .iter()
            .any(|w| w.class == "tray_icon_app")),
        "the spawned daemon has no tray icon"
    );
    // It opened the overlay for the command it was started for
    let (overlay, _) =
        wait_for_overlay(spawned_pid, monitor_under_cursor(), Duration::from_secs(10))
            .expect("the spawned daemon did not open the overlay");
    close_overlay(spawned_pid, &overlay);
    std::thread::sleep(Duration::from_secs(2));
    // The editor pre-warmed for the capture is released once its build
    // finishes; on a slow machine (Windows Sandbox: ~4 s) that is after Esc.
    wait_until(Duration::from_secs(20), || {
        !windows_of(spawned_pid)
            .iter()
            .any(|w| w.class.contains("Tauri") || w.class.contains("WebView"))
    });
    let tray = check_daemon(&exe, spawned_pid);
    stop(spawned_pid, &tray);

    assert!(
        user_state() == cleanup.before,
        "the settings file or startup registry entries changed"
    );
    println!("settings file and startup registry entries unchanged");
    let _ = fs::remove_file(&log_path);
}

#[test]
#[ignore = "live: starts the daemon, shows the capture overlay"]
fn overlay_opens_closes_and_full_capture_saves() {
    let _live = LIVE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    per_monitor_dpi_aware();
    let exe = exe();
    println!("binary: {}", exe.display());
    assert!(
        processes_of(&exe).is_empty(),
        "{} is already running",
        exe.display()
    );
    assert!(!lock_exists(), "another Phramer 16 daemon holds the lock");
    let mut cleanup = Cleanup {
        exe: exe.clone(),
        pids: Vec::new(),
        before: user_state(),
    };
    let save_dir =
        std::env::temp_dir().join(format!("phramer-overlay-smoke-{}", std::process::id()));
    let _ = fs::remove_dir_all(&save_dir);
    fs::create_dir_all(&save_dir).expect("save folder");

    // 1. The daemon
    let log_path =
        std::env::temp_dir().join(format!("phramer-overlay-smoke-{}.log", std::process::id()));
    let log = fs::File::create(&log_path).expect("log file");
    let daemon = Command::new(&exe)
        .env("PHRAMER_NO_WELCOME", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log)
        .spawn()
        .expect("start the daemon");
    let daemon_pid = daemon.id();
    cleanup.pids.push(daemon_pid);
    println!("daemon pid {daemon_pid}, log {}", log_path.display());
    assert!(
        wait_until(Duration::from_secs(20), || {
            lock_exists()
                && windows_of(daemon_pid)
                    .iter()
                    .any(|w| w.class == "tray_icon_app")
        }),
        "the daemon did not come up"
    );
    std::thread::sleep(Duration::from_secs(2));
    let tray = check_daemon(&exe, daemon_pid);

    // 2. `phramer gui`: a topmost overlay over the monitor under the cursor
    let target = monitor_under_cursor();
    println!("target monitor {target:?}");
    let cli = Command::new(&exe)
        .env("PHRAMER_NO_WELCOME", "1")
        .arg("gui")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start the CLI");
    cleanup.pids.push(cli.id());
    let (overlay, waited) = wait_for_overlay(daemon_pid, target, Duration::from_secs(10))
        .expect("no overlay covering the target monitor");
    // A selection made as soon as the overlay shows: the toolbar must follow
    select(&overlay, (200, 200), (600, 500));
    println!(
        "overlay {:#x} at {:?} seen {} ms after starting the CLI (topmost {}, tool window {})",
        overlay.hwnd,
        overlay.rect,
        waited.as_millis(),
        overlay.topmost,
        overlay.tool_window
    );
    wait_cli(cli);
    assert!(overlay.topmost, "the overlay is not topmost");
    assert!(
        overlay.tool_window,
        "the overlay would show in the taskbar and Alt+Tab"
    );
    assert!(
        wait_until(Duration::from_secs(5), || fs::read_to_string(&log_path)
            .is_ok_and(|text| logged_first_frame(&text).is_some())),
        "the daemon logged no first-frame time"
    );
    let first_frame = logged_first_frame(&fs::read_to_string(&log_path).unwrap_or_default())
        .expect("first-frame time");
    println!("daemon: first frame {first_frame} ms after the command");
    assert!(
        first_frame < FIRST_FRAME_BUDGET_MS,
        "first frame took {first_frame} ms (budget {FIRST_FRAME_BUDGET_MS} ms)"
    );
    assert!(
        wait_until(Duration::from_secs(5), || fs::read_to_string(&log_path)
            .is_ok_and(|text| logged_toolbar(&text).is_some())),
        "the daemon logged no toolbar time"
    );
    let toolbar =
        logged_toolbar(&fs::read_to_string(&log_path).unwrap_or_default()).expect("toolbar time");
    println!("daemon: toolbar {toolbar} ms after the command");
    assert!(
        toolbar < TOOLBAR_BUDGET_MS,
        "the toolbar took {toolbar} ms (budget {TOOLBAR_BUDGET_MS} ms)"
    );

    // 3. Esc closes every overlay window and exports nothing
    close_overlay(daemon_pid, &overlay);
    assert!(
        wait_until(Duration::from_secs(5), || fs::read_to_string(&log_path)
            .is_ok_and(|text| text.contains("capture cancelled"))),
        "the overlay did not report a cancel"
    );
    let text = fs::read_to_string(&log_path).unwrap_or_default();
    assert!(!text.contains("Capture saved"), "Esc exported something");
    assert_eq!(fs::read_dir(&save_dir).map(Iterator::count).unwrap_or(0), 0);
    println!("Esc: overlay closed, nothing exported");

    // 4. `phramer full -p <folder>`: one PNG the size of the virtual desktop
    let cli = Command::new(&exe)
        .env("PHRAMER_NO_WELCOME", "1")
        .args(["full", "-p"])
        .arg(&save_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start the CLI");
    cleanup.pids.push(cli.id());
    wait_cli(cli);
    assert!(
        wait_until(Duration::from_secs(20), || fs::read_to_string(&log_path)
            .is_ok_and(|text| text.contains("Capture saved as"))),
        "the full capture was not saved"
    );
    let saved: Vec<PathBuf> = fs::read_dir(&save_dir)
        .expect("save folder")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect();
    assert_eq!(saved.len(), 1, "{saved:?}");
    let bytes = fs::read(&saved[0]).expect("saved file");
    // SAFETY: plain metric queries.
    let desktop = unsafe {
        (
            GetSystemMetrics(SM_CXVIRTUALSCREEN) as u32,
            GetSystemMetrics(SM_CYVIRTUALSCREEN) as u32,
        )
    };
    let size = png_size(&bytes);
    println!(
        "saved {} ({} bytes), {size:?}; virtual desktop {desktop:?}",
        saved[0].display(),
        bytes.len()
    );
    assert_eq!(size, Some(desktop));

    // 5. A clean stop
    stop(daemon_pid, &tray);
    drop(daemon);
    println!(
        "--- daemon log ---\n{}",
        fs::read_to_string(&log_path).unwrap_or_default()
    );
    assert!(
        user_state() == cleanup.before,
        "the settings file or startup registry entries changed"
    );
    println!("settings file and startup registry entries unchanged");
    let _ = fs::remove_dir_all(&save_dir);
    let _ = fs::remove_file(&log_path);
}

/// `phramer-cli.exe` next to the binary under test.
fn cli_exe() -> PathBuf {
    match std::env::var_os("PHRAMER_SMOKE_EXE") {
        Some(_) => exe().with_file_name("phramer-cli.exe"),
        None => PathBuf::from(env!("CARGO_BIN_EXE_phramer-cli")),
    }
}

/// Whether the interactive desktop is the user's, not the lock screen or
/// a UAC prompt: only then can the screen be grabbed.
fn desktop_is_unlocked() -> bool {
    use windows_sys::Win32::System::StationsAndDesktops::{
        CloseDesktop, DESKTOP_READOBJECTS, GetUserObjectInformationW, OpenInputDesktop, UOI_NAME,
    };
    // SAFETY: opens the input desktop for reading its name; closed below.
    // A locked session refuses the open.
    let desktop = unsafe { OpenInputDesktop(0, 0, DESKTOP_READOBJECTS) };
    if desktop.is_null() {
        return false;
    }
    let mut name = [0u16; 64];
    let mut needed = 0u32;
    // SAFETY: `name` is a writable buffer of the byte size given.
    let ok = unsafe {
        GetUserObjectInformationW(
            desktop,
            UOI_NAME,
            name.as_mut_ptr().cast(),
            (name.len() * 2) as u32,
            &mut needed,
        )
    };
    // SAFETY: the handle opened above, closed once.
    unsafe { CloseDesktop(desktop) };
    let name = String::from_utf16_lossy(&name);
    ok != 0 && name.trim_end_matches('\0').eq_ignore_ascii_case("Default")
}

/// Runs `phramer-cli` with `args`; its exit code, stdout and stderr.
fn run_cli(args: &[&str]) -> (Option<i32>, Vec<u8>, String) {
    let start = Instant::now();
    let output = Command::new(cli_exe())
        .env("PHRAMER_NO_WELCOME", "1")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("run phramer-cli");
    println!(
        "phramer-cli {args:?}: {} after {} ms, {} bytes out, stderr {:?}",
        output.status,
        start.elapsed().as_millis(),
        output.stdout.len(),
        String::from_utf8_lossy(&output.stderr)
    );
    (
        output.status.code(),
        output.stdout,
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
#[ignore = "live: starts the daemon through phramer-cli and grabs the screen"]
fn cli_prints_raw_captures_and_exit_codes() {
    let _live = LIVE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if !desktop_is_unlocked() {
        println!("skipped: the session is locked or another desktop has the input");
        return;
    }
    per_monitor_dpi_aware();
    let exe = exe();
    println!("binaries: {} and {}", exe.display(), cli_exe().display());
    assert!(
        processes_of(&exe).is_empty(),
        "{} is already running",
        exe.display()
    );
    assert!(!lock_exists(), "another Phramer 16 daemon holds the lock");
    let mut cleanup = Cleanup {
        exe: exe.clone(),
        pids: Vec::new(),
        before: user_state(),
    };

    // 1. Handled in the console process itself
    let (code, stdout, _) = run_cli(&["--version"]);
    assert_eq!(code, Some(0));
    assert_eq!(
        String::from_utf8_lossy(&stdout).trim(),
        format!("Phramer v{}", env!("CARGO_PKG_VERSION"))
    );
    let (code, _, stderr) = run_cli(&["full", "--bogus"]);
    // 15.0.0 exits 0 after a parse error
    assert_eq!(code, Some(0));
    assert!(stderr.contains("phramer: error:"), "{stderr}");
    assert!(
        processes_of(&exe).is_empty(),
        "a parse error started Phramer"
    );

    // SAFETY: plain metric queries.
    let desktop = unsafe {
        (
            GetSystemMetrics(SM_CXVIRTUALSCREEN) as u32,
            GetSystemMetrics(SM_CYVIRTUALSCREEN) as u32,
        )
    };
    // 2. No daemon: one is started for the capture, which comes back as PNG
    let (code, stdout, _) = run_cli(&["full", "--raw"]);
    let daemons = processes_of(&exe);
    cleanup.pids.extend(&daemons);
    assert_eq!(code, Some(0));
    assert_eq!(png_size(&stdout), Some(desktop), "not a desktop-sized PNG");
    assert_eq!(daemons.len(), 1, "one daemon: {daemons:?}");
    let daemon_pid = daemons[0];

    // 3. Through the running daemon
    let (code, stdout, _) = run_cli(&["full", "--raw"]);
    assert_eq!(code, Some(0));
    assert_eq!(png_size(&stdout), Some(desktop));
    assert_eq!(processes_of(&exe), [daemon_pid], "only the daemon remains");

    // 4. A clean stop
    assert!(
        wait_until(Duration::from_secs(20), || windows_of(daemon_pid)
            .iter()
            .any(|w| w.class == "tray_icon_app")),
        "the daemon has no tray icon"
    );
    std::thread::sleep(Duration::from_secs(2));
    let tray = check_daemon(&exe, daemon_pid);
    stop(daemon_pid, &tray);
    assert!(
        user_state() == cleanup.before,
        "the settings file or startup registry entries changed"
    );
    println!("settings file and startup registry entries unchanged");
}

/// The editor window's title (`editor::TITLE`).
const EDITOR_TITLE: &str = "Phramer Editor";

fn window_title(hwnd: isize) -> String {
    let mut text = [0u16; 256];
    // SAFETY: `text` holds 256 characters; a stale handle just fails.
    let length = unsafe { GetWindowTextW(hwnd as HWND, text.as_mut_ptr(), 256) };
    String::from_utf16_lossy(&text[..length.max(0) as usize])
}

/// The daemon's editor windows that are showing.
fn editors(pid: u32) -> Vec<Window> {
    windows_of(pid)
        .into_iter()
        .filter(|window| window.user_visible() && window_title(window.hwnd) == EDITOR_TITLE)
        .collect()
}

/// Every process started under `pid`, at any depth (the WebView2 browser
/// process and its renderer, GPU and utility processes).
fn children_of(pid: u32) -> Vec<u32> {
    // SAFETY: snapshot of all processes; closed below.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    let mut pairs = Vec::new();
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    // SAFETY: `entry.dwSize` is set as the API requires.
    let mut more = unsafe { Process32FirstW(snapshot, &mut entry) };
    while more != 0 {
        pairs.push((entry.th32ProcessID, entry.th32ParentProcessID));
        // SAFETY: as above.
        more = unsafe { Process32NextW(snapshot, &mut entry) };
    }
    // SAFETY: the snapshot handle is closed once.
    unsafe { CloseHandle(snapshot) };
    let mut found = vec![pid];
    let mut index = 0;
    while index < found.len() {
        let parent = found[index];
        found.extend(
            pairs
                .iter()
                .filter(|(child, of)| *of == parent && *child != parent && !found.contains(child))
                .map(|(child, _)| *child)
                .collect::<Vec<_>>(),
        );
        index += 1;
    }
    found.remove(0);
    found
}

/// The private working set of `pids` together, in MB, as Task Manager's
/// Memory column shows it.
fn memory_mb(pids: &[u32]) -> Option<f64> {
    if pids.is_empty() {
        return Some(0.0);
    }
    let list = pids.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
    let script = format!(
        "(Get-CimInstance Win32_PerfRawData_PerfProc_Process | Where-Object {{ @({list}) -contains $_.IDProcess }} | Measure-Object WorkingSetPrivate -Sum).Sum"
    );
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-Command", &script])
        .output()
        .ok()?;
    let bytes: f64 = String::from_utf8_lossy(&output.stdout).trim().parse().ok()?;
    Some(bytes / 1_048_576.0)
}

/// The open time the daemon logged last, in milliseconds.
fn logged_editor_open(log: &str) -> Option<u128> {
    log.lines()
        .rev()
        .filter_map(|line| line.split(" visible ").nth(1))
        .find_map(|rest| rest.split(' ').next()?.parse().ok())
}

/// The window-creation time the daemon logged last, in milliseconds.
fn logged_window_created(log: &str) -> Option<u128> {
    log.lines()
        .rev()
        .filter_map(|line| line.split(" created in ").nth(1))
        .find_map(|rest| rest.split(' ').next()?.parse().ok())
}

/// How long the daemon's event loop takes to answer a forwarded command
/// (`phramer-cli launcher`, answered from the main thread): what a tray
/// click waits for too.
fn round_trip() -> Duration {
    let start = Instant::now();
    let status = Command::new(cli_exe())
        .env("PHRAMER_NO_WELCOME", "1")
        .arg("launcher")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("run phramer-cli");
    assert!(status.success(), "phramer-cli launcher: {status}");
    start.elapsed()
}

/// [`round_trip`] on another thread, started now.
fn round_trip_meanwhile() -> std::thread::JoinHandle<Duration> {
    std::thread::spawn(round_trip)
}

fn daemon_memory(pid: u32) -> (usize, f64) {
    let mut everything = vec![pid];
    everything.extend(children_of(pid));
    (everything.len(), memory_mb(&everything).unwrap_or(-1.0))
}

/// Starts `gui --region 320x200+40+40` and waits for its overlay.
fn region_capture(
    exe: &Path,
    daemon_pid: u32,
    target: (i32, i32, i32, i32),
    cleanup: &mut Cleanup,
) -> Window {
    let cli = Command::new(exe)
        .env("PHRAMER_NO_WELCOME", "1")
        .args(["gui", "--region", "320x200+40+40"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start the CLI");
    cleanup.pids.push(cli.id());
    let (overlay, _) = wait_for_overlay(daemon_pid, target, Duration::from_secs(10))
        .expect("no overlay covering the target monitor");
    wait_cli(cli);
    overlay
}

/// Posts a key to a window of ours.
fn post_key(window: &Window, key: u16) {
    // SAFETY: posting a message to a window handle; a stale one just fails.
    let posted = unsafe { PostMessageW(window.hwnd as HWND, WM_KEYDOWN, usize::from(key), 0) };
    assert_ne!(posted, 0, "key {key:#x} could not be posted");
}

/// Waits, polling every 5 ms, for exactly one editor window to show; it
/// and how long that took.
fn wait_for_editor(daemon_pid: u32, log_path: &Path) -> (Window, Duration) {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(15) {
        let shown = editors(daemon_pid);
        if !shown.is_empty() {
            assert_eq!(shown.len(), 1, "one window per image: {shown:?}");
            return (shown[0].clone(), start.elapsed());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!(
        "no editor window appeared; windows: {:?}\n--- log ---\n{}",
        windows_of(daemon_pid),
        fs::read_to_string(log_path).unwrap_or_default()
    );
}

/// Closes the editor (nothing drawn, so no question) and checks that its
/// webview and temporary images go.
fn close_editor(daemon_pid: u32, editor: &Window, images: &[PathBuf]) {
    // SAFETY: posting a message to a window handle; a stale one just fails.
    let posted = unsafe { PostMessageW(editor.hwnd as HWND, WM_CLOSE, 0, 0) };
    assert_ne!(posted, 0, "WM_CLOSE could not be posted");
    assert!(
        wait_until(Duration::from_secs(10), || editors(daemon_pid).is_empty()),
        "the editor did not close"
    );
    for image in images {
        assert!(
            wait_until(Duration::from_secs(5), || !image.exists()),
            "{} was left behind",
            image.display()
        );
    }
}

/// No webview left: no WebView2 process under the daemon and no Tauri
/// window.
fn assert_no_webview(daemon_pid: u32) {
    assert!(
        wait_until(Duration::from_secs(15), || children_of(daemon_pid)
            .is_empty()),
        "webview processes outlived the editor: {:?}",
        children_of(daemon_pid)
    );
    let windows = windows_of(daemon_pid);
    assert!(
        !windows
            .iter()
            .any(|w| w.class.contains("Tauri") || w.class.contains("WebView")),
        "a webview window was left: {windows:?}"
    );
}

#[test]
#[ignore = "live: starts the daemon, captures a region and opens the editor"]
fn open_in_editor_shows_the_editor_window() {
    let _live = LIVE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if !desktop_is_unlocked() {
        println!("skipped: the session is locked or another desktop has the input");
        return;
    }
    // A debug build loads the page from the Vite dev server, which is not
    // running here
    if std::env::var_os("PHRAMER_SMOKE_EXE").is_none() {
        println!("skipped: set PHRAMER_SMOKE_EXE to a release build (npm run tauri build -- --no-bundle)");
        return;
    }
    per_monitor_dpi_aware();
    let exe = exe();
    println!("binary: {}", exe.display());
    assert!(
        processes_of(&exe).is_empty(),
        "{} is already running",
        exe.display()
    );
    assert!(!lock_exists(), "another Phramer 16 daemon holds the lock");
    let mut cleanup = Cleanup {
        exe: exe.clone(),
        pids: Vec::new(),
        before: user_state(),
    };

    // 1. The daemon, with no webview
    let log_path =
        std::env::temp_dir().join(format!("phramer-editor-smoke-{}.log", std::process::id()));
    let log = fs::File::create(&log_path).expect("log file");
    let daemon = Command::new(&exe)
        .env("PHRAMER_NO_WELCOME", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log)
        .spawn()
        .expect("start the daemon");
    let daemon_pid = daemon.id();
    cleanup.pids.push(daemon_pid);
    assert!(
        wait_until(Duration::from_secs(20), || {
            lock_exists()
                && windows_of(daemon_pid)
                    .iter()
                    .any(|w| w.class == "tray_icon_app")
        }),
        "the daemon did not come up"
    );
    std::thread::sleep(Duration::from_secs(2));
    let tray = check_daemon(&exe, daemon_pid);
    assert!(
        children_of(daemon_pid).is_empty(),
        "the idle daemon runs child processes: {:?}",
        children_of(daemon_pid)
    );
    println!("memory, idle daemon: {:.1} MB", memory_mb(&[daemon_pid]).unwrap_or(-1.0));
    let idle: Vec<u128> = (0..3).map(|_| round_trip().as_millis()).collect();
    println!("event loop, idle: answers in {idle:?} ms");
    let read_log = || fs::read_to_string(&log_path).unwrap_or_default();

    // Twice: the first capture starts WebView2 cold, the second warm
    let target = monitor_under_cursor();
    for round in 1..=2 {
        println!("--- capture, pre-warm, Open in Editor {round} ---");
        let before = read_log();
        // 2. A capture of a small region; the editor is built meanwhile,
        // while the event loop is timed
        let overlay = region_capture(&exe, daemon_pid, target, &mut cleanup);
        let during_build = round_trip_meanwhile();
        assert!(
            wait_until(Duration::from_secs(15), || read_log()[before.len()..]
                .contains(" pre-warmed, window created in ")),
            "no editor was pre-warmed\n--- log ---\n{}",
            read_log()
        );
        let text = read_log();
        println!(
            "pre-warm: window created in {:?} ms; event loop answered in {} ms meanwhile",
            logged_window_created(&text[before.len()..]),
            during_build.join().expect("round trip").as_millis()
        );
        std::thread::sleep(Duration::from_secs(2));
        let (processes, mb) = daemon_memory(daemon_pid);
        println!("memory, capture open + pre-warmed editor ({processes} processes): {mb:.1} MB");
        assert!(processes > 1, "no webview process during the capture");

        // 3. E: the pre-warmed window shows with the region
        post_key(&overlay, u16::from(b'E'));
        let after_e = round_trip_meanwhile();
        let (editor, seen) = wait_for_editor(daemon_pid, &log_path);
        println!(
            "editor {:#x} at {:?}, seen {} ms after E; event loop answered in {} ms meanwhile",
            editor.hwnd,
            editor.rect,
            seen.as_millis(),
            after_e.join().expect("round trip").as_millis()
        );
        let opened = || logged_editor_open(&read_log()[before.len()..]).is_some();
        assert!(
            wait_until(Duration::from_secs(5), opened),
            "the daemon logged no open time"
        );
        let text = read_log();
        let this_round = &text[before.len()..];
        println!(
            "daemon: editor visible {:?} ms after the hand-over",
            logged_editor_open(this_round)
        );
        assert!(
            this_round.contains("(pre-warmed) for"),
            "the pre-warmed window was not used:\n{this_round}"
        );
        assert_eq!(
            this_round.matches(" created in ").count(),
            1,
            "one window built:\n{this_round}"
        );
        let image = this_round
            .lines()
            .rev()
            .find_map(|line| line.split("editor: opening ").nth(1))
            .map(|path| PathBuf::from(path.trim()))
            .expect("the daemon logged the image it opened");
        let size = fs::read(&image).ok().and_then(|bytes| png_size(&bytes));
        assert_eq!(size, Some((320, 200)), "the editor got the whole monitor");
        std::thread::sleep(Duration::from_secs(2));
        let (processes, mb) = daemon_memory(daemon_pid);
        println!("memory, daemon + editor ({processes} processes): {mb:.1} MB");

        // 3b. With the editor open, another capture's Open in Editor joins
        // it (15.0.0's session): no window is pre-warmed or built, and the
        // one window shows the image in its filmstrip
        let mut images = vec![image];
        if round == 2 {
            println!("--- Open in Editor with the editor open ---");
            let before = read_log();
            let overlay = region_capture(&exe, daemon_pid, target, &mut cleanup);
            assert!(
                wait_until(Duration::from_secs(15), || read_log()[before.len()..]
                    .contains(" is open; nothing to pre-warm")),
                "a window was pre-warmed with the editor open\n--- log ---\n{}",
                read_log()
            );
            post_key(&overlay, u16::from(b'E'));
            let joined = || {
                let text = read_log();
                let this_round = &text[before.len()..];
                this_round.contains(" gets image 2 (") && logged_editor_open(this_round).is_some()
            };
            assert!(
                wait_until(Duration::from_secs(15), joined),
                "the image did not join the open editor\n--- log ---\n{}",
                read_log()
            );
            let text = read_log();
            let this_round = &text[before.len()..];
            println!(
                "second image drawn {:?} ms after it was handed over",
                logged_editor_open(this_round)
            );
            assert!(
                !this_round.contains(" created in "),
                "a window was built:\n{this_round}"
            );
            let shown = editors(daemon_pid);
            assert_eq!(shown.len(), 1, "one editor window: {shown:?}");
            assert_eq!(
                shown[0].hwnd, editor.hwnd,
                "the image went to another window"
            );
            let second = this_round
                .lines()
                .rev()
                .find_map(|line| line.split("editor: opening ").nth(1))
                .map(|path| PathBuf::from(path.trim()))
                .expect("the daemon logged the second image");
            assert!(second.exists(), "{} is missing", second.display());
            images.push(second);
        }

        // 4. Closing it frees the webview and the temporary images
        close_editor(daemon_pid, &editor, &images);
        assert_no_webview(daemon_pid);
        println!(
            "after closing: {:.1} MB, no child processes",
            memory_mb(&[daemon_pid]).unwrap_or(-1.0)
        );
    }

    // 5. E at once, before the pre-warm can have finished: still one window
    println!("--- Open in Editor straight away ---");
    let before = read_log();
    let overlay = region_capture(&exe, daemon_pid, target, &mut cleanup);
    post_key(&overlay, u16::from(b'E'));
    let (editor, seen) = wait_for_editor(daemon_pid, &log_path);
    println!("editor seen {} ms after E", seen.as_millis());
    std::thread::sleep(Duration::from_secs(1));
    let text = read_log();
    let this_round = &text[before.len()..];
    assert_eq!(
        this_round.matches(" created in ").count(),
        1,
        "one window built:\n{this_round}"
    );
    println!(
        "used the pre-warmed window: {}",
        this_round.contains("(pre-warmed) for")
    );
    let image = this_round
        .lines()
        .rev()
        .find_map(|line| line.split("editor: opening ").nth(1))
        .map(|path| PathBuf::from(path.trim()))
        .expect("the daemon logged the image it opened");
    close_editor(daemon_pid, &editor, std::slice::from_ref(&image));
    assert_no_webview(daemon_pid);

    // 6. A capture ended with Esc leaves no webview behind
    println!("--- capture, pre-warm, Esc ---");
    let before = read_log();
    let overlay = region_capture(&exe, daemon_pid, target, &mut cleanup);
    assert!(
        wait_until(Duration::from_secs(15), || read_log()[before.len()..]
            .contains(" pre-warmed, window created in ")),
        "no editor was pre-warmed"
    );
    assert!(
        !children_of(daemon_pid).is_empty(),
        "no webview during the capture"
    );
    close_overlay(daemon_pid, &overlay);
    assert_no_webview(daemon_pid);
    assert!(
        read_log()[before.len()..].contains("released unused"),
        "the pre-warmed window was not released"
    );
    std::thread::sleep(Duration::from_secs(2));
    println!(
        "memory, idle daemon after Esc: {:.1} MB, no child processes",
        memory_mb(&[daemon_pid]).unwrap_or(-1.0)
    );
    let idle: Vec<u128> = (0..3).map(|_| round_trip().as_millis()).collect();
    println!("event loop, idle again: answers in {idle:?} ms");

    // 7. A clean stop leaves nothing running
    stop(daemon_pid, &tray);
    drop(daemon);
    println!("--- daemon log ---\n{}", read_log());
    assert!(
        user_state() == cleanup.before,
        "the settings file or startup registry entries changed"
    );
    let _ = fs::remove_file(&log_path);
}
