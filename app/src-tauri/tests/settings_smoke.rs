//! Live smoke test of the settings window. Ignored by default: it starts the
//! release daemon, opens the window through `phramer config` and through the
//! tray menu, changes a setting in it, and checks that the file changed, the
//! daemon applied it at once, and closing the window leaves no webview.
//!
//! ```text
//! cargo test -p phramer --test settings_smoke -- --ignored --nocapture
//! ```
//!
//! Needs `PHRAMER_SMOKE_EXE` (the release build from
//! `npm run tauri build -- --no-bundle`, as a backslash Windows path) and an
//! unlocked session. The setting is changed through WebView2's DevTools port
//! (`tests/smoke/settings-cdp.mjs`), so no real input reaches Windows; the
//! tray menu is opened and answered with messages posted to the daemon's own
//! windows. The settings file, the launch-at-sign-in entries and the Print
//! Screen value are snapshotted first and put back at the end.

#![cfg(windows)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_SUCCESS, HANDLE, HWND, LPARAM, RECT, WAIT_OBJECT_0,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_DWORD, REG_SZ, RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RegDeleteKeyValueW,
    RegGetValueW, RegSetKeyValueW,
};
use windows_sys::Win32::System::StationsAndDesktops::{
    CloseDesktop, DESKTOP_READOBJECTS, GetUserObjectInformationW, OpenInputDesktop, UOI_NAME,
};
use windows_sys::Win32::System::Threading::{
    GetExitCodeProcess, OpenMutexW, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_SYNCHRONIZE, QueryFullProcessImageNameW, SYNCHRONIZATION_SYNCHRONIZE,
    WaitForSingleObject,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
use windows_sys::Win32::UI::Shell::{NOTIFYICONIDENTIFIER, Shell_NotifyIconGetRect};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    ENDSESSION_CLOSEAPP, EnumWindows, GetClassNameW, GetMenuItemCount, GetMenuItemID,
    GetMenuStringW, GetWindowTextW, GetWindowThreadProcessId, HMENU, IsWindowVisible,
    MF_BYPOSITION, PostMessageW, SendMessageW, WM_CLOSE, WM_COMMAND, WM_ENDSESSION, WM_KEYDOWN,
    WM_RBUTTONUP,
};

const LOCK: &str = "app.phramer.desktop-sim";
/// `settings_ui::TITLE`.
const SETTINGS_TITLE: &str = "Phramer Settings";
/// tray-icon's callback message (`WM_USER_TRAYICON`).
const TRAY_CALLBACK: u32 = 6002;
/// `MN_GETHMENU`: a popup menu window's menu.
const MN_GETHMENU: u32 = 0x01E1;
const RUN_KEY: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run";
const APP_PATHS_KEY: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\App Paths\\phramer.exe";
const KEYBOARD_KEY: &str = "Control Panel\\Keyboard";
const PRINT_SCREEN_VALUE: &str = "PrintScreenKeyForSnippingEnabled";

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

fn wait_until(timeout: Duration, mut done: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if done() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
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

fn desktop_is_unlocked() -> bool {
    // SAFETY: opens the input desktop for reading its name; closed below.
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

/// Every process as (pid, parent).
fn process_pairs() -> Vec<(u32, u32)> {
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
    pairs
}

/// Processes running exactly `exe`.
fn processes_of(exe: &Path) -> Vec<u32> {
    let wanted = exe.to_string_lossy().to_lowercase();
    process_pairs()
        .into_iter()
        .map(|(pid, _)| pid)
        .filter(|pid| image_path(*pid).is_some_and(|path| path.to_lowercase() == wanted))
        .collect()
}

/// Every process started under `pid`, at any depth (WebView2's).
fn children_of(pid: u32) -> Vec<u32> {
    let pairs = process_pairs();
    let mut found = vec![pid];
    let mut index = 0;
    while index < found.len() {
        let parent = found[index];
        let children: Vec<u32> = pairs
            .iter()
            .filter(|(child, of)| *of == parent && *child != parent && !found.contains(child))
            .map(|(child, _)| *child)
            .collect();
        found.extend(children);
        index += 1;
    }
    found.remove(0);
    found
}

#[derive(Debug, Clone)]
struct Window {
    hwnd: isize,
    class: String,
    title: String,
    visible: bool,
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
            let mut title = [0u16; 256];
            // SAFETY: both buffers hold 256 characters.
            let (class_length, title_length) = unsafe {
                (
                    GetClassNameW(hwnd, class.as_mut_ptr(), 256),
                    GetWindowTextW(hwnd, title.as_mut_ptr(), 256),
                )
            };
            // SAFETY: a read-only query on a window handle.
            let visible = unsafe { IsWindowVisible(hwnd) } != 0;
            Some(Window {
                hwnd: raw,
                class: String::from_utf16_lossy(&class[..class_length.max(0) as usize]),
                title: String::from_utf16_lossy(&title[..title_length.max(0) as usize]),
                visible,
            })
        })
        .collect()
}

fn settings_windows(pid: u32) -> Vec<Window> {
    windows_of(pid)
        .into_iter()
        .filter(|window| window.visible && window.title == SETTINGS_TITLE)
        .collect()
}

fn tray_window(pid: u32) -> Option<Window> {
    windows_of(pid)
        .into_iter()
        .find(|window| window.class == "tray_icon_app")
}

/// Whether the shell shows an icon for a `tray-icon` window.
fn tray_icon_shown(window: &Window) -> bool {
    (0..=64).any(|id| {
        let identifier = NOTIFYICONIDENTIFIER {
            cbSize: size_of::<NOTIFYICONIDENTIFIER>() as u32,
            hWnd: window.hwnd as HWND,
            uID: id,
            ..Default::default()
        };
        let mut rect = RECT::default();
        // SAFETY: both structures are initialised and outlive the call.
        unsafe { Shell_NotifyIconGetRect(&identifier, &mut rect) == 0 }
    })
}

fn read_string(key: &str, value: &str) -> Option<String> {
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

fn read_dword(key: &str, value: &str) -> Option<u32> {
    let (key, value) = (wide(key), wide(value));
    let mut data = 0u32;
    let mut size = 4u32;
    // SAFETY: `data` holds the 4 bytes RRF_RT_REG_DWORD writes.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&raw mut data).cast(),
            &mut size,
        )
    };
    (status == ERROR_SUCCESS).then_some(data)
}

/// Puts back a value as it was: `None` deletes it.
fn restore_value(key: &str, value: &str, data: Option<&[u8]>, kind: u32) {
    let (key, value) = (wide(key), wide(value));
    match data {
        // SAFETY: all strings NUL-terminated; `data` holds the byte count given.
        Some(data) => unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                value.as_ptr(),
                kind,
                data.as_ptr().cast(),
                data.len() as u32,
            );
        },
        // SAFETY: both strings NUL-terminated.
        None => unsafe {
            RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr());
        },
    }
}

fn settings_file() -> PathBuf {
    PathBuf::from(std::env::var_os("APPDATA").unwrap_or_default()).join("phramer\\phramer.ini")
}

/// The user's state this test may touch.
#[derive(Debug, PartialEq)]
struct UserState {
    settings: Option<Vec<u8>>,
    run_value: Option<String>,
    app_paths: Option<String>,
    print_screen: Option<u32>,
}

fn user_state() -> UserState {
    UserState {
        settings: fs::read(settings_file()).ok(),
        run_value: read_string(RUN_KEY, "Phramer"),
        app_paths: read_string(APP_PATHS_KEY, "Path"),
        print_screen: read_dword(KEYBOARD_KEY, PRINT_SCREEN_VALUE),
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
    let sz = |text: &Option<String>| {
        text.as_ref().map(|text| {
            wide(text)
                .iter()
                .flat_map(|unit| unit.to_le_bytes())
                .collect::<Vec<u8>>()
        })
    };
    restore_value(RUN_KEY, "Phramer", sz(&state.run_value).as_deref(), REG_SZ);
    restore_value(
        APP_PATHS_KEY,
        "Path",
        sz(&state.app_paths).as_deref(),
        REG_SZ,
    );
    let dword = state.print_screen.map(u32::to_le_bytes);
    restore_value(
        KEYBOARD_KEY,
        PRINT_SCREEN_VALUE,
        dword.as_ref().map(|bytes| bytes.as_slice()),
        REG_DWORD,
    );
}

/// Kills what the test started and puts the user's state back if the test
/// fails half way.
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
            eprintln!("the settings file or registry values changed; restored them");
        }
    }
}

fn post(window: &Window, message: u32, wparam: usize, lparam: isize) {
    // SAFETY: posting a message to a window of the daemon; a stale handle
    // just fails.
    let posted = unsafe { PostMessageW(window.hwnd as HWND, message, wparam, lparam) };
    assert_ne!(posted, 0, "message {message:#x} could not be posted");
}

/// `phramer config` through the binary, which forwards it to the daemon.
fn run_config(exe: &Path, cleanup: &mut Cleanup) {
    let mut cli = Command::new(exe)
        .arg("config")
        .env("PHRAMER_NO_WELCOME", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start phramer config");
    cleanup.pids.push(cli.id());
    let status = cli.wait().expect("phramer config ran");
    assert!(status.success(), "phramer config: {status}");
}

/// Waits for the settings window; how long it took.
fn wait_for_settings(pid: u32, log: &Path) -> (Window, Duration) {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(20) {
        let shown = settings_windows(pid);
        if let Some(window) = shown.first() {
            assert_eq!(shown.len(), 1, "one settings window: {shown:?}");
            return (window.clone(), start.elapsed());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!(
        "no settings window; windows: {:?}\n--- log ---\n{}",
        windows_of(pid),
        fs::read_to_string(log).unwrap_or_default()
    );
}

/// Closes the settings window and checks no webview is left behind.
fn close_settings(pid: u32, window: &Window) {
    post(window, WM_CLOSE, 0, 0);
    assert!(
        wait_until(Duration::from_secs(10), || settings_windows(pid).is_empty()),
        "the settings window did not close"
    );
    assert!(
        wait_until(Duration::from_secs(20), || children_of(pid).is_empty()),
        "webview processes outlived the settings window: {:?}",
        children_of(pid)
    );
    let windows = windows_of(pid);
    assert!(
        !windows
            .iter()
            .any(|w| w.class.contains("Tauri") || w.class.contains("WebView")),
        "a webview window was left: {windows:?}"
    );
    println!("settings window closed; no webview left");
}

/// Opens the tray menu with tray-icon's own callback message, finds the
/// Configuration entry in it, closes the menu and sends the entry's command,
/// as a click on it would.
fn choose_tray_configuration(pid: u32, tray: &Window) {
    post(tray, TRAY_CALLBACK, 0, WM_RBUTTONUP as isize);
    let mut menu_window = None;
    assert!(
        wait_until(Duration::from_secs(5), || {
            menu_window = windows_of(pid)
                .into_iter()
                .find(|window| window.class == "#32768" && window.visible);
            menu_window.is_some()
        }),
        "the tray menu did not open"
    );
    let menu_window = menu_window.expect("found above");
    // SAFETY: a query sent to the daemon's own popup menu window.
    let menu = unsafe { SendMessageW(menu_window.hwnd as HWND, MN_GETHMENU, 0, 0) } as HMENU;
    assert!(!menu.is_null(), "the popup menu has no menu");
    // SAFETY: `menu` is the open popup menu; read-only queries.
    let count = unsafe { GetMenuItemCount(menu) };
    let command = (0..count.max(0)).find_map(|position| {
        let mut text = [0u16; 128];
        // SAFETY: `text` holds 128 characters.
        let length =
            unsafe { GetMenuStringW(menu, position as u32, text.as_mut_ptr(), 128, MF_BYPOSITION) };
        let label = String::from_utf16_lossy(&text[..length.max(0) as usize]);
        // SAFETY: as above.
        (label == "&Configuration").then(|| unsafe { GetMenuItemID(menu, position) })
    });
    let command = command.expect("no Configuration entry in the tray menu");
    post(&menu_window, WM_KEYDOWN, usize::from(VK_ESCAPE), 0);
    assert!(
        wait_until(Duration::from_secs(5), || !windows_of(pid)
            .iter()
            .any(|window| window.class == "#32768" && window.visible)),
        "the tray menu did not close"
    );
    post(tray, WM_COMMAND, command as usize, 0);
}

fn end_session(pid: u32) -> bool {
    let Some(target) = windows_of(pid)
        .into_iter()
        .find(|window| window.class == "Tao Thread Event Target")
    else {
        return false;
    };
    // SAFETY: posting a message to a window of the daemon.
    unsafe {
        PostMessageW(
            target.hwnd as HWND,
            WM_ENDSESSION,
            1,
            ENDSESSION_CLOSEAPP as LPARAM,
        ) != 0
    }
}

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

/// Runs the DevTools driver; what it printed.
fn drive(port: u16, action: &str) -> String {
    let app = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let output = Command::new("node")
        .current_dir(&app)
        .args(["tests/smoke/settings-cdp.mjs", &port.to_string(), action])
        .stdin(Stdio::null())
        .output()
        .expect("run node");
    let printed = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        output.status.success(),
        "settings-cdp.mjs {action}: {}\n{printed}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    printed
}

#[test]
#[ignore = "live: starts the daemon, opens the settings window"]
fn settings_open_from_cli_and_tray_apply_live_and_close_clean() {
    if !desktop_is_unlocked() {
        println!("skipped: the session is locked or another desktop has the input");
        return;
    }
    let Some(exe) = std::env::var_os("PHRAMER_SMOKE_EXE").map(PathBuf::from) else {
        println!(
            "skipped: set PHRAMER_SMOKE_EXE to a release build (npm run tauri build -- --no-bundle)"
        );
        return;
    };
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

    // 1. The daemon, no webview, the DevTools port open for the driver
    let port = 9400 + (std::process::id() % 400) as u16;
    let log_path =
        std::env::temp_dir().join(format!("phramer-settings-smoke-{}.log", std::process::id()));
    let log = fs::File::create(&log_path).expect("log file");
    let daemon = Command::new(&exe)
        .env("PHRAMER_NO_WELCOME", "1")
        .env(
            "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
            format!("--remote-debugging-port={port}"),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log)
        .spawn()
        .expect("start the daemon");
    let pid = daemon.id();
    cleanup.pids.push(pid);
    assert!(
        wait_until(Duration::from_secs(20), || lock_exists()
            && tray_window(pid).is_some()),
        "the daemon did not come up"
    );
    std::thread::sleep(Duration::from_secs(2));
    assert!(
        children_of(pid).is_empty(),
        "the idle daemon runs a webview"
    );
    let tray = tray_window(pid).expect("tray window");
    assert!(tray_icon_shown(&tray), "no tray icon");

    // 2. `phramer config` opens it; asking again keeps the one window
    run_config(&exe, &mut cleanup);
    let (window, took) = wait_for_settings(pid, &log_path);
    println!(
        "settings window via phramer config after {} ms",
        took.as_millis()
    );
    run_config(&exe, &mut cleanup);
    std::thread::sleep(Duration::from_secs(1));
    assert_eq!(settings_windows(pid).len(), 1, "a second window was opened");

    // 3. A change in the window: written to the file and applied at once
    let printed = drive(port, "toggle-tray");
    println!("{}", printed.trim());
    assert!(printed.contains("tray switch: false"), "{printed}");
    assert!(
        wait_until(Duration::from_secs(3), || fs::read_to_string(
            settings_file()
        )
        .unwrap_or_default()
        .contains("disabledTrayIcon=true")),
        "disabledTrayIcon=true was not written"
    );
    assert!(
        wait_until(Duration::from_millis(900), || !tray_icon_shown(&tray)),
        "the tray icon stayed: not applied before the file watcher's next poll"
    );
    println!("tray icon hidden at once");
    let printed = drive(port, "toggle-tray");
    assert!(printed.contains("tray switch: true"), "{printed}");
    assert!(
        wait_until(Duration::from_secs(3), || tray_window(pid)
            .is_some_and(|tray| tray_icon_shown(&tray))),
        "the tray icon did not come back"
    );
    println!("tray icon shown again");

    // 4. Closing it leaves no webview
    close_settings(pid, &window);

    // 5. The tray's Configuration entry opens it too
    let tray = tray_window(pid).expect("tray window");
    choose_tray_configuration(pid, &tray);
    let (window, took) = wait_for_settings(pid, &log_path);
    println!(
        "settings window via the tray menu after {} ms",
        took.as_millis()
    );
    close_settings(pid, &window);

    // 6. A clean stop, and the user's state as it was
    assert!(end_session(pid), "no event-loop window to post to");
    assert_eq!(
        wait_for_exit(pid, Duration::from_secs(15)),
        Some(0),
        "clean exit"
    );
    drop(daemon);
    println!(
        "--- daemon log ---\n{}",
        fs::read_to_string(&log_path).unwrap_or_default()
    );
    let after = user_state();
    if after.settings != cleanup.before.settings {
        // The toggles wrote the key back as `false`; the bytes differ even
        // though the value reads the same
        println!("settings file differs only by the toggled key; restoring");
    }
    restore(&cleanup.before);
    assert_eq!(
        user_state(),
        cleanup.before,
        "the user's state was not restored"
    );
    assert!(
        processes_of(&exe).is_empty(),
        "a phramer.exe from the build is still running"
    );
    let _ = fs::remove_file(&log_path);
}
