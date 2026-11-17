//! Live smoke test of the overlay's Screenshot/Video mode bar through the
//! daemon. Ignored by default: it starts the release daemon with video
//! capture turned on, opens the overlay with `phramer gui`, presses 2
//! (`CAPTURE_MODE_VIDEO`) with a message posted to the overlay's own
//! window, and checks the overlay closes and the hand-off to Snipping Tool
//! happens: Snipping Tool starts, or, where it is not installed (Windows
//! Sandbox), the daemon says so.
//!
//! ```text
//! cargo test -p phramer --test video_smoke -- --ignored --nocapture
//! ```
//!
//! Needs `PHRAMER_SMOKE_EXE` (the release build, as a backslash Windows
//! path) and an unlocked session. The settings file and the launch-at-sign-in
//! entries are snapshotted first and put back at the end.

#![cfg(windows)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{CloseHandle, ERROR_SUCCESS, HWND, LPARAM};
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
    OpenMutexW, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
    SYNCHRONIZATION_SYNCHRONIZE,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    ENDSESSION_CLOSEAPP, EnumWindows, GetClassNameW, GetWindowThreadProcessId, IsWindowVisible,
    PostMessageW, WM_ENDSESSION, WM_KEYDOWN,
};

const LOCK: &str = "app.phramer.desktop-sim";
const OVERLAY_CLASS: &str = "PhramerCaptureOverlay";
const RUN_KEY: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run";
const APP_PATHS_KEY: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\App Paths\\phramer.exe";
/// The daemon's warning when Snipping Tool is missing (`actions::record_video`).
const NOT_INSTALLED: &str = "Video capture needs the Windows Snipping Tool, which is not installed";

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

/// Whether the interactive desktop is the user's: only then can the
/// overlay show.
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

/// The image names of every running process, lower case.
fn process_images() -> Vec<(u32, String)> {
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
        let pid = entry.th32ProcessID;
        // SAFETY: plain query; the handle is closed below.
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if !process.is_null() {
            let mut buffer = [0u16; 1024];
            let mut length = buffer.len() as u32;
            // SAFETY: `buffer` holds `length` characters.
            let ok =
                unsafe { QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut length) };
            // SAFETY: opened above, closed once.
            unsafe { CloseHandle(process) };
            if ok != 0 {
                found.push((
                    pid,
                    String::from_utf16_lossy(&buffer[..length as usize]).to_lowercase(),
                ));
            }
        }
        // SAFETY: as above.
        more = unsafe { Process32NextW(snapshot, &mut entry) };
    }
    // SAFETY: the snapshot handle is closed once.
    unsafe { CloseHandle(snapshot) };
    found
}

fn processes_of(exe: &Path) -> Vec<u32> {
    let wanted = exe.to_string_lossy().to_lowercase();
    process_images()
        .into_iter()
        .filter(|(_, image)| *image == wanted)
        .map(|(pid, _)| pid)
        .collect()
}

unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> i32 {
    // SAFETY: `lparam` is the `&mut Vec` passed to EnumWindows below, alive
    // for the whole enumeration.
    let found = unsafe { &mut *(lparam as *mut Vec<isize>) };
    found.push(hwnd as isize);
    1
}

/// The top-level windows of `pid`: handle, class and whether visible.
fn windows_of(pid: u32) -> Vec<(isize, String, bool)> {
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
            // SAFETY: a read-only query on a window handle.
            let visible = unsafe { IsWindowVisible(hwnd) } != 0;
            Some((
                raw,
                String::from_utf16_lossy(&class[..length.max(0) as usize]),
                visible,
            ))
        })
        .collect()
}

fn overlay_of(pid: u32) -> Option<isize> {
    windows_of(pid)
        .into_iter()
        .find(|(_, class, visible)| class == OVERLAY_CLASS && *visible)
        .map(|(hwnd, _, _)| hwnd)
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

/// The settings file with `videoCaptureEnabled=true`, its other lines kept.
fn with_video_enabled(text: &str) -> String {
    let mut lines: Vec<String> = text
        .lines()
        .filter(|line| !line.starts_with("videoCaptureEnabled="))
        .map(str::to_owned)
        .collect();
    match lines.iter().position(|line| line.trim() == "[General]") {
        Some(index) => lines.insert(index + 1, "videoCaptureEnabled=true".to_owned()),
        None => {
            lines.insert(0, "videoCaptureEnabled=true".to_owned());
            lines.insert(0, "[General]".to_owned());
        }
    }
    lines.join("\r\n") + "\r\n"
}

/// Kills what the test started and puts the settings file and the startup
/// registry entries back.
struct Cleanup {
    exe: PathBuf,
    pids: Vec<u32>,
    settings: Option<Vec<u8>>,
    run_value: Option<String>,
    app_paths: Option<String>,
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        for pid in &self.pids {
            if processes_of(&self.exe).contains(pid) {
                let _ = Command::new("taskkill")
                    .args(["/F", "/PID", &pid.to_string()])
                    .output();
            }
        }
        match &self.settings {
            Some(bytes) => {
                let _ = fs::write(settings_file(), bytes);
            }
            None => {
                let _ = fs::remove_file(settings_file());
            }
        }
        restore_registry(RUN_KEY, "Phramer", &self.run_value);
        restore_registry(APP_PATHS_KEY, "Path", &self.app_paths);
    }
}

#[test]
#[ignore = "live: starts the daemon, shows the capture overlay"]
fn video_mode_key_hands_over_to_snipping_tool() {
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
    let before = fs::read(settings_file()).ok();
    let mut cleanup = Cleanup {
        exe: exe.clone(),
        pids: Vec::new(),
        settings: before.clone(),
        run_value: read_registry(RUN_KEY, "Phramer"),
        app_paths: read_registry(APP_PATHS_KEY, "Path"),
    };
    let text = before
        .as_deref()
        .map(String::from_utf8_lossy)
        .unwrap_or_default();
    if let Some(folder) = settings_file().parent() {
        fs::create_dir_all(folder).expect("settings folder");
    }
    fs::write(settings_file(), with_video_enabled(&text)).expect("settings file");
    let snipping_before: Vec<u32> = process_images()
        .into_iter()
        .filter(|(_, image)| image.ends_with("\\snippingtool.exe"))
        .map(|(pid, _)| pid)
        .collect();

    // 1. The daemon
    let log_path =
        std::env::temp_dir().join(format!("phramer-video-smoke-{}.log", std::process::id()));
    let log = fs::File::create(&log_path).expect("log file");
    let daemon = Command::new(&exe)
        .env("PHRAMER_NO_WELCOME", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log)
        .spawn()
        .expect("start the daemon");
    let pid = daemon.id();
    cleanup.pids.push(pid);
    assert!(
        wait_until(Duration::from_secs(20), || lock_exists()
            && windows_of(pid)
                .iter()
                .any(|(_, class, _)| class == "tray_icon_app")),
        "the daemon did not come up"
    );
    std::thread::sleep(Duration::from_secs(2));

    // 2. `phramer gui`: the overlay, with the mode bar
    let mut cli = Command::new(&exe)
        .env("PHRAMER_NO_WELCOME", "1")
        .arg("gui")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start the CLI");
    cleanup.pids.push(cli.id());
    // It hands the command to the daemon and exits
    let forwarded = cli.wait().expect("the CLI ran");
    assert!(forwarded.success(), "phramer gui: {forwarded}");
    let mut overlay = None;
    assert!(
        wait_until(Duration::from_secs(10), || {
            overlay = overlay_of(pid);
            overlay.is_some()
        }),
        "no overlay"
    );
    let overlay = overlay.unwrap_or_default();
    std::thread::sleep(Duration::from_millis(500));

    // 3. 2 picks Video: the overlay goes, the daemon hands over
    // SAFETY: posting a message to a window handle; a stale one just fails.
    let posted = unsafe { PostMessageW(overlay as HWND, WM_KEYDOWN, usize::from(b'2'), 1) };
    assert_ne!(posted, 0, "the key could not be posted");
    assert!(
        wait_until(Duration::from_secs(5), || overlay_of(pid).is_none()),
        "the overlay stayed"
    );
    let read_log = || fs::read_to_string(&log_path).unwrap_or_default();
    assert!(
        wait_until(Duration::from_secs(5), || read_log()
            .contains("capture: video mode chosen")),
        "the daemon did not see the video mode"
    );
    let installed = wait_until(Duration::from_secs(10), || {
        read_log().contains(NOT_INSTALLED)
            || process_images().iter().any(|(pid, image)| {
                image.ends_with("\\snippingtool.exe") && !snipping_before.contains(pid)
            })
    });
    assert!(
        installed,
        "neither Snipping Tool nor the not-installed notice"
    );
    let text = read_log();
    if text.contains(NOT_INSTALLED) {
        println!("Snipping Tool is not installed: the daemon said so");
    } else {
        println!("Snipping Tool started");
    }
    assert!(!text.contains("Capture saved"), "video exported something");

    // 4. A clean stop
    let target = windows_of(pid)
        .into_iter()
        .find(|(_, class, _)| class == "Tao Thread Event Target");
    if let Some((hwnd, _, _)) = target {
        // SAFETY: posting a message to a window handle; a stale one just fails.
        unsafe {
            PostMessageW(
                hwnd as HWND,
                WM_ENDSESSION,
                1,
                ENDSESSION_CLOSEAPP as LPARAM,
            )
        };
    }
    assert!(
        wait_until(Duration::from_secs(15), || processes_of(&exe).is_empty()),
        "the daemon did not stop"
    );
    drop(daemon);
    println!("--- daemon log ---\n{text}");
    drop(cleanup);
    let _ = fs::remove_file(&log_path);
}
