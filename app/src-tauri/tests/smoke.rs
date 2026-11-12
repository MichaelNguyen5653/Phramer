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
//! restored and the test fails.

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

const LOCK: &str = "app.phramer.desktop-sim";
/// The overlay's window class (`phramer-overlay`).
const OVERLAY_CLASS: &str = "PhramerCaptureOverlay";
/// Command to first frame, as the daemon measures and logs it.
const FIRST_FRAME_BUDGET_MS: u128 = 100;

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

#[derive(Debug)]
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
        .arg("gui")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start the CLI");
    cleanup.pids.push(cli.id());
    let (overlay, waited) = wait_for_overlay(daemon_pid, target, Duration::from_secs(10))
        .expect("no overlay covering the target monitor");
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
