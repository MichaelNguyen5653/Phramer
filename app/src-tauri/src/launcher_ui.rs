//! App Launcher (`app/launcher.html`): 15.0.0's `AppLauncherWidget`, as a
//! searchable, keyboard-first list. The capture is already saved (see
//! `handoff::open_with`); choosing an app starts it on that file, then the
//! window closes unless "Keep open after selection" (`keepOpenAppLauncher`)
//! is on. Windows' own "Open with" dialog stays as the last entry.
//!
//! The app list and the icons are read on worker threads and cached for
//! the daemon's life: the window shows at once, the list arrives when read
//! (from the cache after the first time), and each opening refreshes the
//! cache in the background so new installs appear next time.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{
    AppHandle, Manager, Runtime, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};

use crate::daemon::DaemonState;
use crate::launcher_apps::{self, AppEntry};
use crate::{handoff, settings};

/// Labels are `launcher-<n>`; `capabilities/launcher.json` names them.
const LABEL_PREFIX: &str = "launcher-";
/// 15.0.0's title.
const TITLE: &str = "Open With";
const WIDTH: f64 = 560.0;
const HEIGHT: f64 = 440.0;
const MIN_WIDTH: f64 = 380.0;
const MIN_HEIGHT: f64 = 300.0;

/// The launcher windows' files, the app list and the icons.
#[derive(Default)]
pub(crate) struct Launchers {
    files: Mutex<HashMap<String, PathBuf>>,
    next: AtomicU64,
    apps: Mutex<Option<Arc<Vec<AppEntry>>>>,
    refreshing: AtomicBool,
    icons: Mutex<HashMap<String, Option<String>>>,
}

/// Opens a launcher for the capture saved at `file`. From any thread.
pub(crate) fn open<R: Runtime>(app: &AppHandle<R>, file: PathBuf) {
    let launchers = app.state::<Launchers>();
    let label = format!(
        "{LABEL_PREFIX}{}",
        launchers.next.fetch_add(1, Ordering::Relaxed) + 1
    );
    if let Ok(mut files) = launchers.files.lock() {
        files.insert(label.clone(), file);
    }
    let target = app.clone();
    if let Err(error) = app.run_on_main_thread(move || build(&target, label)) {
        log::error!("launcher: cannot open the window: {error}");
    }
}

fn build<R: Runtime>(app: &AppHandle<R>, label: String) {
    let built = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("launcher.html".into()))
        .title(TITLE)
        .inner_size(WIDTH, HEIGHT)
        .min_inner_size(MIN_WIDTH, MIN_HEIGHT)
        .decorations(false)
        .center()
        .visible(false)
        .build();
    match built {
        Ok(window) => {
            let handle = app.clone();
            let closed = label.clone();
            window.on_window_event(move |event| {
                if matches!(event, WindowEvent::Destroyed) {
                    // The file stays: an app started on it may still be
                    // reading it (15.0.0 kept it too)
                    if let Ok(mut files) = handle.state::<Launchers>().files.lock() {
                        files.remove(&closed);
                    }
                    log::info!("{closed}: window closed");
                }
            });
            log::info!("{label}: window created");
        }
        Err(error) => log::error!("cannot create the launcher window: {error}"),
    }
}

fn file_of<R: Runtime>(app: &AppHandle<R>, window: &WebviewWindow<R>) -> Result<PathBuf, String> {
    app.state::<Launchers>()
        .files
        .lock()
        .map_err(|error| error.to_string())?
        .get(window.label())
        .cloned()
        .ok_or_else(|| format!("{} has no capture", window.label()))
}

/// What the page starts from.
#[derive(Debug, Serialize)]
pub(crate) struct LauncherInit {
    keep_open: bool,
    /// The capture's file name, shown in the title bar.
    file_name: String,
}

#[tauri::command]
pub(crate) fn launcher_init<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
) -> Result<LauncherInit, String> {
    let file = file_of(&app, &window)?;
    let current = settings::load(&app.state::<DaemonState>().settings_path);
    Ok(LauncherInit {
        keep_open: current.keep_open_app_launcher(),
        file_name: file
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
    })
}

/// Reads the list again on a worker thread, unless that is already
/// happening; the cache takes the result.
fn refresh<R: Runtime>(
    app: &AppHandle<R>,
) -> Option<tauri::async_runtime::JoinHandle<Arc<Vec<AppEntry>>>> {
    let launchers = app.state::<Launchers>();
    if launchers.refreshing.swap(true, Ordering::AcqRel) {
        return None;
    }
    let handle = app.clone();
    Some(tauri::async_runtime::spawn_blocking(move || {
        let apps = Arc::new(launcher_apps::enumerate());
        let launchers = handle.state::<Launchers>();
        if let Ok(mut cached) = launchers.apps.lock() {
            *cached = Some(Arc::clone(&apps));
        }
        launchers.refreshing.store(false, Ordering::Release);
        apps
    }))
}

/// The apps: the cached list at once (refreshed meanwhile for next time),
/// or the first reading.
#[tauri::command]
pub(crate) async fn launcher_apps<R: Runtime>(app: AppHandle<R>) -> Result<Vec<AppEntry>, String> {
    let cached = app
        .state::<Launchers>()
        .apps
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    let reading = refresh(&app);
    if let Some(apps) = cached {
        return Ok(apps.as_ref().clone());
    }
    let apps = match reading {
        Some(reading) => reading.await.map_err(|error| error.to_string())?,
        // Another window is reading the first list; rare, so read it too
        None => tauri::async_runtime::spawn_blocking(|| Arc::new(launcher_apps::enumerate()))
            .await
            .map_err(|error| error.to_string())?,
    };
    Ok(apps.as_ref().clone())
}

/// The icons of `exes` as PNG data URLs; those without one are left out.
#[tauri::command]
pub(crate) async fn launcher_icons<R: Runtime>(
    app: AppHandle<R>,
    exes: Vec<String>,
) -> Result<HashMap<String, String>, String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let launchers = handle.state::<Launchers>();
        let mut found = HashMap::new();
        for exe in exes {
            let cached = launchers
                .icons
                .lock()
                .ok()
                .and_then(|icons| icons.get(&exe).cloned());
            let icon = match cached {
                Some(icon) => icon,
                None => {
                    let icon = launcher_apps::icon_data_url(&exe);
                    if let Ok(mut icons) = launchers.icons.lock() {
                        icons.insert(exe.clone(), icon.clone());
                    }
                    icon
                }
            };
            if let Some(icon) = icon {
                found.insert(exe, icon);
            }
        }
        found
    })
    .await
    .map_err(|error| error.to_string())
}

/// Starts `exe` on the capture, in the temporary folder (15.0.0's working
/// directory), as Explorer would: no handles of ours are inherited.
#[tauri::command]
pub(crate) async fn launcher_launch<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    exe: String,
) -> Result<(), String> {
    let file = file_of(&app, &window)?;
    // Only a program the list offered
    let listed = app
        .state::<Launchers>()
        .apps
        .lock()
        .map_err(|error| error.to_string())?
        .as_ref()
        .is_some_and(|apps| apps.iter().any(|entry| entry.exe == exe));
    if !listed {
        return Err(format!("{exe} is not in the list"));
    }
    start(Path::new(&exe), &file).map_err(|error| format!("Unable to launch {exe}: {error}"))?;
    log::info!("launcher: started {exe} on {}", file.display());
    Ok(())
}

/// Windows' "Open with" dialog for the capture, on its own thread (it is
/// modal for as long as the user likes).
#[tauri::command]
pub(crate) fn launcher_open_with<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
) -> Result<(), String> {
    let file = file_of(&app, &window)?;
    handoff::show_open_with(file).map_err(|error| error.to_string())
}

/// "Keep open after selection", remembered (`keepOpenAppLauncher`).
#[tauri::command]
pub(crate) fn launcher_set_keep_open<R: Runtime>(
    app: AppHandle<R>,
    keep_open: bool,
) -> Result<(), String> {
    let path = app.state::<DaemonState>().settings_path.clone();
    let mut current =
        phramer_core::config::Settings::load(&path).map_err(|error| error.to_string())?;
    current.set_keep_open_app_launcher(keep_open);
    current.save(&path).map_err(|error| error.to_string())
}

/// The parameters `exe` gets: the file, quoted.
fn parameters(file: &Path) -> String {
    format!("\"{}\"", file.display())
}

fn start(exe: &Path, file: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::Com::{
        COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoUninitialize,
    };
    use windows_sys::Win32::UI::Shell::{SEE_MASK_FLAG_NO_UI, SHELLEXECUTEINFOW, ShellExecuteExW};
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let wide = |text: &std::ffi::OsStr| -> Vec<u16> { text.encode_wide().chain(Some(0)).collect() };
    let program = wide(exe.as_os_str());
    let arguments = wide(std::ffi::OsStr::new(&parameters(file)));
    let folder = wide(
        file.parent()
            .map_or_else(
                || std::env::temp_dir().into_os_string(),
                |parent| parent.as_os_str().to_owned(),
            )
            .as_os_str(),
    );
    let open = wide(std::ffi::OsStr::new("open"));
    // SAFETY: an all-zero SHELLEXECUTEINFOW is valid; every string is
    // NUL-terminated and outlives the call.
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_FLAG_NO_UI;
    info.lpVerb = open.as_ptr();
    info.lpFile = program.as_ptr();
    info.lpParameters = arguments.as_ptr();
    info.lpDirectory = folder.as_ptr();
    info.nShow = SW_SHOWNORMAL;
    // SAFETY: the shell asks for an apartment; this thread's is left below.
    unsafe {
        let initialised = CoInitializeEx(
            std::ptr::null(),
            (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32,
        ) >= 0;
        // The error is read before CoUninitialize can overwrite it
        let result = if ShellExecuteExW(&mut info) == 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        };
        if initialised {
            CoUninitialize();
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_is_passed_quoted() {
        assert_eq!(
            parameters(Path::new(r"C:\Temp\2026-11-13_09-41_ab12cd.png")),
            r#""C:\Temp\2026-11-13_09-41_ab12cd.png""#
        );
        assert_eq!(
            parameters(Path::new(r"C:\Users\A B\AppData\Local\Temp\x.png")),
            r#""C:\Users\A B\AppData\Local\Temp\x.png""#
        );
    }

    /// Starts a program on a file as the launcher does: Windows Sandbox only.
    /// Edge, which the Sandbox's Start Menu lists (it has no Notepad), or
    /// the command prompt.
    #[test]
    #[ignore = "live: starts Microsoft Edge or a command prompt"]
    fn a_program_is_started_on_the_file() {
        // A program that is not there fails with Windows' reason
        let missing = start(Path::new(r"C:\nowhere\missing.exe"), Path::new(r"C:\x.png"))
            .expect_err("a missing program");
        println!("missing program: {missing}");
        assert_ne!(missing.raw_os_error(), Some(0), "{missing}");

        let file =
            std::env::temp_dir().join(format!("phramer launcher test {}.png", std::process::id()));
        std::fs::write(&file, b"not really a png").unwrap();
        let edge = PathBuf::from(r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe");
        let program = if edge.is_file() {
            edge
        } else {
            PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into()))
                .join("System32\\cmd.exe")
        };
        let name = program.file_name().unwrap().to_string_lossy().into_owned();
        start(&program, &file).expect("started");
        let lines = || {
            let output = std::process::Command::new("powershell.exe")
                .args([
                    "-NoProfile",
                    "-Command",
                    &format!(
                        "Get-CimInstance Win32_Process -Filter \"Name='{name}'\" | ForEach-Object {{ \"$($_.ProcessId)|$($_.CommandLine)\" }}"
                    ),
                ])
                .output()
                .expect("powershell");
            String::from_utf8_lossy(&output.stdout).into_owned()
        };
        let wanted = file.display().to_string();
        let mut found = String::new();
        for _ in 0..50 {
            found = lines()
                .lines()
                .find(|line| line.contains(&wanted))
                .unwrap_or_default()
                .to_owned();
            if !found.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        println!("started: {found}");
        assert!(
            found.contains(&format!("\"{wanted}\"")),
            "{name} with the quoted file: {}",
            lines()
        );
        if let Some(pid) = found.split('|').next() {
            let _ = std::process::Command::new("taskkill")
                .args(["/F", "/T", "/PID", pid])
                .output();
        }
        let _ = std::fs::remove_file(&file);
    }
}
