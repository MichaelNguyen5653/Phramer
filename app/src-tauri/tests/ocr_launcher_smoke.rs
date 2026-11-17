//! Live smoke test of the OCR results window and App Launcher. Ignored by
//! default and meant for Windows Sandbox (`scenario-tests.ps1`): it shows a
//! window of known text, captures it, reads it through OCR, opens App
//! Launcher on a capture and starts Notepad with it through a Start Menu
//! shortcut it creates.
//!
//! ```text
//! cargo test -p phramer --test ocr_launcher_smoke -- --ignored --nocapture
//! ```
//!
//! Needs `PHRAMER_SMOKE_EXE` (the release build, as a backslash Windows
//! path), an unlocked session and Node.js: the windows are driven through
//! WebView2's DevTools port (`tests/smoke/panels-cdp.mjs`). The settings
//! file (App Launcher is bound to `L` for the test, since a posted Ctrl+O
//! carries no Ctrl) and the startup registry entries are put back at the
//! end; the shortcut is removed.

#![cfg(windows)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{CloseHandle, ERROR_SUCCESS, HWND, LPARAM, WAIT_OBJECT_0};
use windows_sys::Win32::Graphics::Gdi::{
    ANTIALIASED_QUALITY, CLIP_DEFAULT_PRECIS, CreateFontW, DEFAULT_CHARSET, FW_NORMAL,
    OUT_DEFAULT_PRECIS,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
};
use windows_sys::Win32::System::StationsAndDesktops::{
    CloseDesktop, DESKTOP_READOBJECTS, GetUserObjectInformationW, OpenInputDesktop, UOI_NAME,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentThreadId, GetExitCodeProcess, OpenMutexW, OpenProcess,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, QueryFullProcessImageNameW,
    SYNCHRONIZATION_SYNCHRONIZE, WaitForSingleObject,
};
use windows_sys::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DispatchMessageW, ENDSESSION_CLOSEAPP, EnumWindows, GetClassNameW,
    GetMessageW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible, MSG, PostMessageW,
    PostThreadMessageW, SendMessageW, TranslateMessage, WM_CLOSE, WM_ENDSESSION, WM_KEYDOWN,
    WM_QUIT, WM_SETFONT, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
};

const LOCK: &str = "app.phramer.desktop-sim";
const OVERLAY_CLASS: &str = "PhramerCaptureOverlay";
/// `ocr_ui::TITLE` and `launcher_ui::TITLE`.
const OCR_TITLE: &str = "Extracted text";
const LAUNCHER_TITLE: &str = "Open With";
const RUN_KEY: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run";
const APP_PATHS_KEY: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\App Paths\\phramer.exe";
/// What the test window shows, and where (physical pixels).
const KNOWN_TEXT: &str = "The quick brown fox jumps over\nthe lazy dog 12345";
const TEXT_WINDOW: (i32, i32, i32, i32) = (80, 80, 820, 160);
/// How long a new window may take to show: WebView2 starts cold in a fresh
/// sandbox.
const WINDOW_WAIT: Duration = Duration::from_secs(60);
/// The Start Menu shortcut the launcher must list and start.
const SHORTCUT_NAME: &str = "Phramer Test Notepad";

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
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

/// Every process as (pid, parent, executable file name).
fn processes() -> Vec<(u32, u32, String)> {
    // SAFETY: snapshot of all processes; closed below.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    let mut found = Vec::new();
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    // SAFETY: `entry.dwSize` is set as the API requires.
    let mut more = unsafe { Process32FirstW(snapshot, &mut entry) };
    while more != 0 {
        let end = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(0);
        found.push((
            entry.th32ProcessID,
            entry.th32ParentProcessID,
            String::from_utf16_lossy(&entry.szExeFile[..end]),
        ));
        // SAFETY: as above.
        more = unsafe { Process32NextW(snapshot, &mut entry) };
    }
    // SAFETY: the snapshot handle is closed once.
    unsafe { CloseHandle(snapshot) };
    found
}

fn processes_of(exe: &Path) -> Vec<u32> {
    let wanted = exe.to_string_lossy().to_lowercase();
    processes()
        .into_iter()
        .map(|(pid, _, _)| pid)
        .filter(|pid| image_path(*pid).is_some_and(|path| path.to_lowercase() == wanted))
        .collect()
}

/// Every process started under `pid`, at any depth, with its file name.
fn children_of(pid: u32) -> Vec<(u32, String)> {
    let all = processes();
    let mut found = vec![(pid, String::new())];
    let mut index = 0;
    while index < found.len() {
        let parent = found[index].0;
        let children: Vec<(u32, String)> = all
            .iter()
            .filter(|(child, of, _)| {
                *of == parent && *child != parent && !found.iter().any(|(seen, _)| seen == child)
            })
            .map(|(child, _, name)| (*child, name.clone()))
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
            let (class_length, title_length, visible) = unsafe {
                (
                    GetClassNameW(hwnd, class.as_mut_ptr(), 256),
                    GetWindowTextW(hwnd, title.as_mut_ptr(), 256),
                    IsWindowVisible(hwnd) != 0,
                )
            };
            Some(Window {
                hwnd: raw,
                class: String::from_utf16_lossy(&class[..class_length.max(0) as usize]),
                title: String::from_utf16_lossy(&title[..title_length.max(0) as usize]),
                visible,
            })
        })
        .collect()
}

fn titled(pid: u32, title: &str) -> Vec<Window> {
    windows_of(pid)
        .into_iter()
        .filter(|window| window.visible && window.title == title)
        .collect()
}

fn post(window: &Window, message: u32, wparam: usize) {
    // SAFETY: posting a message to a window handle; a stale one just fails.
    let posted = unsafe { PostMessageW(window.hwnd as HWND, message, wparam, 0) };
    assert_ne!(posted, 0, "message {message:#x} could not be posted");
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

fn settings_file() -> PathBuf {
    PathBuf::from(std::env::var_os("APPDATA").unwrap_or_default()).join("phramer\\phramer.ini")
}

fn shortcut_file() -> PathBuf {
    PathBuf::from(std::env::var_os("APPDATA").unwrap_or_default()).join(format!(
        "Microsoft\\Windows\\Start Menu\\Programs\\{SHORTCUT_NAME}.lnk"
    ))
}

#[derive(Debug, PartialEq)]
struct UserState {
    settings: Option<Vec<u8>>,
    run_value: Option<String>,
    app_paths: Option<String>,
}

fn user_state() -> UserState {
    UserState {
        settings: fs::read(settings_file()).ok(),
        run_value: read_registry(RUN_KEY, "Phramer"),
        app_paths: read_registry(APP_PATHS_KEY, "Path"),
    }
}

/// Kills what the test started and puts the user's state back.
struct Cleanup {
    exe: PathBuf,
    pids: Vec<u32>,
    before: UserState,
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        for pid in &self.pids {
            if processes_of(&self.exe).contains(pid) {
                let _ = Command::new("taskkill")
                    .args(["/F", "/T", "/PID", &pid.to_string()])
                    .output();
            }
        }
        match &self.before.settings {
            Some(bytes) => {
                let _ = fs::write(settings_file(), bytes);
            }
            None => {
                let _ = fs::remove_file(settings_file());
            }
        }
        restore_registry(RUN_KEY, "Phramer", &self.before.run_value);
        restore_registry(APP_PATHS_KEY, "Path", &self.before.app_paths);
        let _ = fs::remove_file(shortcut_file());
    }
}

fn end_session(pid: u32) -> bool {
    let Some(target) = windows_of(pid)
        .into_iter()
        .find(|window| window.class == "Tao Thread Event Target")
    else {
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

fn wait_for_exit(pid: u32, timeout: Duration) -> Option<u32> {
    // SAFETY: plain open; closed below.
    let process = unsafe {
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

/// A topmost window showing [`KNOWN_TEXT`] in 40 px Segoe UI, with its own
/// message loop; dropping it closes the window.
struct TextWindow {
    thread: u32,
    joined: Option<std::thread::JoinHandle<()>>,
}

impl TextWindow {
    fn show() -> Self {
        let (sender, receiver) = std::sync::mpsc::channel();
        let joined = std::thread::spawn(move || {
            let (x, y, width, height) = TEXT_WINDOW;
            let class = wide("STATIC");
            let text = wide(&KNOWN_TEXT.replace('\n', "\r\n"));
            let face = wide("Segoe UI");
            // SAFETY: a system class; all strings NUL-terminated and alive
            // while the window exists (this thread's whole life).
            unsafe {
                let window = CreateWindowExW(
                    WS_EX_TOPMOST,
                    class.as_ptr(),
                    text.as_ptr(),
                    WS_POPUP | WS_VISIBLE,
                    x,
                    y,
                    width,
                    height,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null(),
                );
                let font = CreateFontW(
                    -40,
                    0,
                    0,
                    0,
                    FW_NORMAL as i32,
                    0,
                    0,
                    0,
                    u32::from(DEFAULT_CHARSET),
                    u32::from(OUT_DEFAULT_PRECIS),
                    u32::from(CLIP_DEFAULT_PRECIS),
                    u32::from(ANTIALIASED_QUALITY),
                    0,
                    face.as_ptr(),
                );
                SendMessageW(window, WM_SETFONT, font as usize, 1);
                let _ = sender.send(GetCurrentThreadId());
                let mut message: MSG = std::mem::zeroed();
                while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
        });
        let thread = receiver.recv().expect("the text window's thread");
        std::thread::sleep(Duration::from_millis(500));
        Self {
            thread,
            joined: Some(joined),
        }
    }
}

impl Drop for TextWindow {
    fn drop(&mut self) {
        // SAFETY: a thread id this test started; WM_QUIT ends its loop.
        unsafe { PostThreadMessageW(self.thread, WM_QUIT, 0, 0) };
        if let Some(joined) = self.joined.take() {
            let _ = joined.join();
        }
    }
}

/// Runs the DevTools driver; what it printed, parsed.
fn drive(port: u16, action: &[&str]) -> serde_json::Value {
    let app = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let output = Command::new("node")
        .current_dir(&app)
        .arg("tests/smoke/panels-cdp.mjs")
        .arg(port.to_string())
        .args(action)
        .stdin(Stdio::null())
        .output()
        .expect("run node");
    let printed = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        output.status.success(),
        "panels-cdp.mjs {action:?}: {}\n{printed}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_str(printed.trim().lines().last().unwrap_or("null"))
        .unwrap_or_else(|error| panic!("{error}: {printed}"))
}

/// What a failure report needs: the daemon's windows and child processes,
/// the DevTools page list (or why there is none) and the daemon's log.
fn evidence(daemon: u32, port: u16, log: &str) -> String {
    let app = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let pages = Command::new("node")
        .current_dir(&app)
        .args(["tests/smoke/panels-cdp.mjs", &port.to_string(), "pages"])
        .stdin(Stdio::null())
        .output()
        .map(|output| {
            format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
                    .lines()
                    .take(3)
                    .collect::<Vec<_>>()
                    .join(" | ")
            )
        })
        .unwrap_or_else(|error| error.to_string());
    let windows: Vec<String> = windows_of(daemon)
        .iter()
        .map(|w| format!("{:?} {:?} visible={}", w.class, w.title, w.visible))
        .collect();
    format!(
        "windows: {windows:#?}\nchild processes: {:?}\npages: {pages}\n--- log ---\n{log}",
        children_of(daemon)
    )
}

/// `phramer gui --region` over `rect` and its overlay.
fn capture(exe: &Path, daemon: u32, rect: (i32, i32, i32, i32), cleanup: &mut Cleanup) -> Window {
    let (x, y, width, height) = rect;
    let mut cli = Command::new(exe)
        .env("PHRAMER_NO_WELCOME", "1")
        .args(["gui", "--region", &format!("{width}x{height}+{x}+{y}")])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start the CLI");
    cleanup.pids.push(cli.id());
    let mut overlay = None;
    assert!(
        wait_until(Duration::from_secs(10), || {
            overlay = windows_of(daemon)
                .into_iter()
                .find(|window| window.class == OVERLAY_CLASS && window.visible);
            overlay.is_some()
        }),
        "no overlay"
    );
    // The CLI only hands the command over
    assert!(
        cli.wait().is_ok_and(|status| status.success()),
        "the CLI failed"
    );
    // Let it take the region and draw its toolbar
    std::thread::sleep(Duration::from_secs(1));
    overlay.expect("overlay")
}

/// Closes the window titled `title` and waits for it to go.
fn close(daemon: u32, title: &str) {
    for window in titled(daemon, title) {
        post(&window, WM_CLOSE, 0);
    }
    assert!(
        wait_until(Duration::from_secs(10), || titled(daemon, title).is_empty()),
        "{title} did not close"
    );
}

/// No WebView2 process under the daemon and no Tauri window left.
fn assert_no_webview(daemon: u32) {
    assert!(
        wait_until(Duration::from_secs(15), || children_of(daemon).is_empty()),
        "processes outlived the windows: {:?}",
        children_of(daemon)
    );
    let windows = windows_of(daemon);
    assert!(
        !windows
            .iter()
            .any(|w| w.class.contains("Tauri") || w.class.contains("WebView")),
        "a webview window was left: {windows:?}"
    );
}

fn create_shortcut(target: &str) {
    let script = format!(
        "$s = (New-Object -ComObject WScript.Shell).CreateShortcut('{}'); $s.TargetPath = '{target}'; $s.Save()",
        shortcut_file().display()
    );
    let status = Command::new("powershell.exe")
        .args(["-NoProfile", "-Command", &script])
        .status()
        .expect("run powershell");
    assert!(status.success(), "the test shortcut was not created");
}

/// Command lines of the running `name` processes.
fn command_lines(name: &str) -> String {
    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-Command",
            &format!("Get-CimInstance Win32_Process -Filter \"Name='{name}'\" | ForEach-Object {{ \"$($_.ProcessId) $($_.CommandLine)\" }}"),
        ])
        .output()
        .expect("run powershell");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
#[ignore = "live: starts the daemon, captures text on screen, opens the OCR and App Launcher windows, starts Notepad"]
fn ocr_window_reads_the_capture_and_the_launcher_opens_it_in_an_app() {
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
    // SAFETY: a constant context; failing only means it was already set.
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
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

    // App Launcher on a key a posted message can carry
    let mut settings = phramer_core::config::Settings::load(&settings_file()).unwrap_or_default();
    settings
        .set_shortcut("TYPE_OPEN_APP", "L")
        .expect("TYPE_OPEN_APP=L");
    if let Some(folder) = settings_file().parent() {
        fs::create_dir_all(folder).expect("settings folder");
    }
    settings.save(&settings_file()).expect("settings saved");
    let notepad =
        PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into()))
            .join("System32\\notepad.exe");
    create_shortcut(&notepad.display().to_string());

    // 1. The daemon, with the DevTools port open for the driver
    let port = 9400 + (std::process::id() % 400) as u16;
    let log_path =
        std::env::temp_dir().join(format!("phramer-panels-smoke-{}.log", std::process::id()));
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
            && windows_of(pid).iter().any(|w| w.class == "tray_icon_app")),
        "the daemon did not come up"
    );
    std::thread::sleep(Duration::from_secs(2));
    let read_log = || fs::read_to_string(&log_path).unwrap_or_default();

    // 2. OCR of known text: the results window shows it, nothing copied
    let text_window = TextWindow::show();
    let overlay = capture(&exe, pid, TEXT_WINDOW, &mut cleanup);
    post(&overlay, WM_KEYDOWN, usize::from(b'O'));
    let asked = Instant::now();
    if !wait_until(WINDOW_WAIT, || titled(pid, OCR_TITLE).len() == 1) {
        panic!(
            "no OCR window after {} s\n{}",
            asked.elapsed().as_secs(),
            evidence(pid, port, &read_log())
        );
    }
    println!(
        "OCR window shown {} ms after O",
        asked.elapsed().as_millis()
    );
    drop(text_window);
    let started = Instant::now();
    let result = drive(port, &["ocr-result"]);
    println!("OCR after {} ms: {result}", started.elapsed().as_millis());
    let text = result["text"].as_str().unwrap_or_default();
    let status = result["status"].as_str().unwrap_or_default();
    let words = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
    if status.starts_with("No OCR language is installed") {
        // Windows Sandbox ships without one, and the test must not install
        // one: the window, its message (15.0.0's) and Try harder are checked;
        // the text cannot be
        println!("NOTE: this Windows has no OCR language pack; recognised text not checked");
        assert_eq!(
            status,
            "No OCR language is installed. Add one under Settings > Time & Language > Language & \
             region > Add a language; language packs include text recognition."
        );
        assert_eq!(text, "");
        let harder = drive(port, &["ocr-harder"]);
        println!("Try harder: {harder}");
        assert_eq!(harder["status"], result["status"], "{harder}");
    } else {
        assert_eq!(words(text), words(KNOWN_TEXT), "recognised text\n{result}");
        assert_eq!(
            result["hint"].as_str(),
            Some(
                "Recognized from the captured image. Capture close up and unblurred for the best result."
            )
        );

        // 3. Try harder runs every pass and still reads it
        let harder = drive(port, &["ocr-harder"]);
        println!("Try harder: {harder}");
        assert_eq!(
            words(harder["text"].as_str().unwrap_or_default()),
            words(KNOWN_TEXT)
        );
        assert!(
            harder["diagnostics"]
                .as_str()
                .unwrap_or_default()
                .contains("pass C (binarized)"),
            "{harder}"
        );
    }
    assert!(
        wait_until(Duration::from_secs(5), || read_log()
            .contains(" (High, Recognized) in ")),
        "no high-effort run logged\n{}",
        read_log()
    );

    // 4. Closing it leaves no webview
    close(pid, OCR_TITLE);
    assert_no_webview(pid);
    println!("OCR window closed, no webview left");

    // 5. App Launcher lists the Start Menu and starts Notepad on the capture
    let before = command_lines("notepad.exe");
    let overlay = capture(&exe, pid, (40, 40, 320, 200), &mut cleanup);
    post(&overlay, WM_KEYDOWN, usize::from(b'L'));
    let asked = Instant::now();
    if !wait_until(WINDOW_WAIT, || titled(pid, LAUNCHER_TITLE).len() == 1) {
        panic!(
            "no launcher window after {} s\n{}",
            asked.elapsed().as_secs(),
            evidence(pid, port, &read_log())
        );
    }
    println!(
        "launcher window shown {} ms after L",
        asked.elapsed().as_millis()
    );
    let listed = drive(port, &["launcher-list"]);
    let names: Vec<&str> = listed["names"]
        .as_array()
        .map(|names| names.iter().filter_map(|name| name.as_str()).collect())
        .unwrap_or_default();
    println!("launcher lists {} apps: {names:?}", names.len());
    assert!(names.contains(&SHORTCUT_NAME), "{names:?}");
    drive(port, &["launcher-pick", SHORTCUT_NAME]);
    // Keep open is off: the window closes, Notepad runs with the PNG
    assert!(
        wait_until(Duration::from_secs(10), || titled(pid, LAUNCHER_TITLE)
            .is_empty()),
        "the launcher stayed open"
    );
    let mut started_with = String::new();
    assert!(
        wait_until(Duration::from_secs(15), || {
            started_with = command_lines("notepad.exe")
                .lines()
                .find(|line| !before.contains(*line) && line.to_lowercase().contains(".png"))
                .unwrap_or_default()
                .to_owned();
            !started_with.is_empty()
        }),
        "Notepad was not started on the capture: {}\n--- log ---\n{}",
        command_lines("notepad.exe"),
        read_log()
    );
    println!("started: {started_with}");
    let file = started_with
        .split('"')
        .find(|part| part.to_lowercase().ends_with(".png"))
        .map(PathBuf::from)
        .expect("the PNG on Notepad's command line");
    assert!(file.is_file(), "{} does not exist", file.display());
    let notepad_pid: u32 = started_with
        .split_whitespace()
        .next()
        .and_then(|pid| pid.parse().ok())
        .expect("Notepad's pid");
    let _ = Command::new("taskkill")
        .args(["/F", "/PID", &notepad_pid.to_string()])
        .output();
    let _ = fs::remove_file(&file);
    assert_no_webview(pid);
    println!("launcher closed, no webview left");

    // 6. A clean stop
    assert!(end_session(pid), "no event-loop window to post to");
    assert_eq!(
        wait_for_exit(pid, Duration::from_secs(15)),
        Some(0),
        "clean exit"
    );
    drop(daemon);
    println!("--- daemon log ---\n{}", read_log());
    let _ = fs::remove_file(&log_path);
}
