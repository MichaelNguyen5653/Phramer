//! The update check, 15.0.0's `getLatestAvailableVersion`
//! and `checkForUpdates`: the latest GitHub release at startup and every
//! 24 hours while `checkForUpdates` is on, or when asked from the tray.
//!
//! A newer version badges the tray icon and adds "Update to version X and
//! restart" to its menu. That, as 15.0.0's `startUpdateAndRestart`, asks
//! first (Update now, Later, Skip this version), then downloads the
//! release's checksum and MSI, verifies it, and hands it to msiexec through
//! a script that starts Phramer again afterwards. A release without both
//! assets only opens its page. The Store build has no updater at all (the
//! Store updates it), as 15.0.0's Store edition had none.

use std::cmp::Ordering;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::{AppHandle, Manager, Runtime};

use crate::daemon::DaemonState;
use crate::{notify, settings, tray};

/// `GIT_API_URL` of 15.0.0.
const RELEASES_URL: &str = "https://api.github.com/repos/MichaelNguyen5653/Phramer/releases/latest";
const GITHUB_JSON: &str = "application/vnd.github+json";
/// Only release pages of the project are opened from the menu.
const RELEASE_PAGE_PREFIX: &str = "https://github.com/MichaelNguyen5653/Phramer/";
/// The installer runs elevated: only the project's own release assets.
const DOWNLOAD_PREFIX: &str = "https://github.com/MichaelNguyen5653/Phramer/releases/download/";
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
/// A release document or checksum is a few kilobytes; anything far larger
/// is not one.
const MAX_RESPONSE: usize = 4 * 1024 * 1024;
/// The MSI is tens of megabytes.
const MAX_INSTALLER: usize = 512 * 1024 * 1024;

const LATEST: &str = "You have the latest version";
const FAILED: &str = "Failed to get information about the latest version.";

/// A version as release tags spell it: numbers, then an optional
/// prerelease (`16.0.0-beta.2`). A leading `v` is dropped, as 15.0.0 did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Version {
    numbers: Vec<u64>,
    prerelease: Vec<String>,
}

impl Version {
    pub(crate) fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let text = text
            .strip_prefix('v')
            .or_else(|| text.strip_prefix('V'))
            .unwrap_or(text);
        // Build metadata never orders versions
        let text = text.split('+').next().unwrap_or(text);
        let (core, prerelease) = match text.split_once('-') {
            Some((core, prerelease)) => (core, Some(prerelease)),
            None => (text, None),
        };
        let numbers = core
            .split('.')
            .map(|part| part.parse::<u64>().ok())
            .collect::<Option<Vec<_>>>()?;
        if numbers.is_empty() {
            return None;
        }
        let prerelease = match prerelease {
            Some("") => return None,
            Some(text) => text.split('.').map(str::to_owned).collect(),
            None => Vec::new(),
        };
        Some(Self {
            numbers,
            prerelease,
        })
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        let length = self.numbers.len().max(other.numbers.len());
        // Missing parts are 0: 16.0 is 16.0.0
        let part = |numbers: &[u64], index: usize| numbers.get(index).copied().unwrap_or(0);
        (0..length)
            .map(|index| part(&self.numbers, index).cmp(&part(&other.numbers, index)))
            .find(|order| order.is_ne())
            .unwrap_or_else(|| compare_prerelease(&self.prerelease, &other.prerelease))
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Semantic versioning's order: a release is above its prereleases;
/// numeric identifiers compare as numbers, below alphanumeric ones.
fn compare_prerelease(a: &[String], b: &[String]) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        (false, false) => {}
    }
    for (x, y) in a.iter().zip(b) {
        let order = match (x.parse::<u64>(), y.parse::<u64>()) {
            (Ok(x), Ok(y)) => x.cmp(&y),
            (Ok(_), Err(_)) => Ordering::Less,
            (Err(_), Ok(_)) => Ordering::Greater,
            (Err(_), Err(_)) => x.cmp(y),
        };
        if order.is_ne() {
            return order;
        }
    }
    a.len().cmp(&b.len())
}

/// The parts of a release the check reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Release {
    /// `tag_name` without its `v`.
    pub(crate) version: String,
    pub(crate) html_url: String,
    /// The `.msi` asset and its `.msi.sha256sum`, when both are the
    /// project's own downloads.
    pub(crate) msi_url: Option<String>,
    pub(crate) sha_url: Option<String>,
}

/// Reads `tag_name`, `html_url` and the installer assets from the releases
/// API's answer.
pub(crate) fn parse_release(json: &str) -> Result<Release, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|error| format!("not a release: {error}"))?;
    let field = |name: &str| {
        value
            .get(name)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| format!("the release has no {name}"))
    };
    let tag = field("tag_name")?;
    let version = tag
        .trim()
        .strip_prefix('v')
        .or_else(|| tag.trim().strip_prefix('V'))
        .unwrap_or(tag.trim())
        .to_owned();
    let (mut msi_url, mut sha_url) = (None, None);
    let assets = value.get("assets").and_then(serde_json::Value::as_array);
    for asset in assets.into_iter().flatten() {
        let text = |name: &str| asset.get(name).and_then(serde_json::Value::as_str);
        let (Some(name), Some(url)) = (text("name"), text("browser_download_url")) else {
            continue;
        };
        if !url.starts_with(DOWNLOAD_PREFIX) {
            continue;
        }
        let name = name.to_ascii_lowercase();
        if name.ends_with(".msi") {
            msi_url = Some(url.to_owned());
        } else if name.ends_with(".msi.sha256sum") {
            sha_url = Some(url.to_owned());
        }
    }
    Ok(Release {
        version,
        html_url: field("html_url")?,
        msi_url,
        sha_url,
    })
}

/// The hash in a `sha256sum` file: its first word, lowercase, when it is
/// 64 hexadecimal digits.
pub(crate) fn parse_checksum(text: &str) -> Option<String> {
    let word = text.split_whitespace().next()?.to_ascii_lowercase();
    (word.len() == 64 && word.bytes().all(|byte| byte.is_ascii_hexdigit())).then_some(word)
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// `Phramer-<version>-win64.msi`, for a version of letters, digits, dots
/// and dashes only: it came from the network and goes into a script.
pub(crate) fn installer_name(version: &str) -> Option<String> {
    let plain = !version.is_empty()
        && version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'-');
    plain.then(|| format!("Phramer-{version}-win64.msi"))
}

/// 15.0.0's `phramer-update.cmd`: cmd waits for msiexec, then starts the
/// app from its (unchanged) install path. Run with no `&&`, so a decline at
/// the UAC prompt still brings the old version back instead of leaving
/// nothing running.
pub(crate) fn update_script(msi: &Path, exe: &Path) -> String {
    format!(
        "@echo off\r\n\
         msiexec /i \"{}\" /passive /norestart\r\n\
         start \"\" \"{}\"\r\n\
         del \"%~f0\"\r\n",
        msi.display(),
        exe.display()
    )
}

/// Host and path of an `https://` address.
fn split_url(url: &str) -> Option<(&str, &str)> {
    let rest = url.strip_prefix("https://")?;
    let (host, path) = match rest.find('/') {
        Some(at) => rest.split_at(at),
        None => (rest, "/"),
    };
    (!host.is_empty()).then_some((host, path))
}

/// `release` when it is newer than `current` and than the version the user
/// skipped (`ignoreUpdateToVersion`, empty for none).
pub(crate) fn newer(current: &str, ignored: &str, release: Release) -> Option<Release> {
    let candidate = Version::parse(&release.version)?;
    let current = Version::parse(current)?;
    if candidate <= current {
        return None;
    }
    if let Some(ignored) = Version::parse(ignored)
        && candidate <= ignored
    {
        return None;
    }
    Some(release)
}

/// The newer release found this session, if any.
static AVAILABLE: Mutex<Option<Release>> = Mutex::new(None);

/// The version of the newer release found, for the tray.
pub(crate) fn available_version() -> Option<String> {
    AVAILABLE
        .lock()
        .ok()
        .and_then(|available| available.as_ref().map(|release| release.version.clone()))
}

/// Starts the checks: one now and one a day, each only while
/// `checkForUpdates` is on.
pub(crate) fn start<R: Runtime>(app: &AppHandle<R>) {
    if cfg!(feature = "store") {
        return;
    }
    let app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("update-check".to_owned())
        .spawn(move || {
            loop {
                let path = app.state::<DaemonState>().settings_path.clone();
                if settings::load(&path).check_for_updates() {
                    check(&app, false);
                }
                std::thread::sleep(CHECK_INTERVAL);
            }
        });
    if let Err(error) = spawned {
        log::error!("cannot start the update check: {error}");
    }
}

/// The tray's "Check for updates" or "Update to version X and restart".
pub(crate) fn check_now<R: Runtime>(app: &AppHandle<R>) {
    if cfg!(feature = "store") {
        return;
    }
    let known = AVAILABLE
        .lock()
        .ok()
        .and_then(|available| available.clone());
    if let Some(release) = known {
        start_update(app, release);
        return;
    }
    // No update seen yet: check, and say how it went
    let app = app.clone();
    if let Err(error) = std::thread::Builder::new()
        .name("update-check".to_owned())
        .spawn(move || check(&app, true))
    {
        log::error!("cannot check for updates: {error}");
    }
}

/// One check, off the main thread; `manual` reports every outcome.
fn check<R: Runtime>(app: &AppHandle<R>, manual: bool) {
    let found = fetch_text(RELEASES_URL, Some(GITHUB_JSON)).and_then(|json| parse_release(&json));
    let main = app.clone();
    let posted = app.run_on_main_thread(move || {
        let current = settings::load(&main.state::<DaemonState>().settings_path);
        // `handleReplyCheckUpdates`: an automatic check turned off meanwhile
        // has nothing to say
        if !manual && !current.check_for_updates() {
            return;
        }
        match found {
            Ok(release) => {
                let newer = newer(
                    phramer_core::VERSION,
                    &current.ignore_update_to_version(),
                    release,
                );
                match newer {
                    Some(release) => {
                        log::info!("version {} is available", release.version);
                        if let Ok(mut available) = AVAILABLE.lock() {
                            *available = Some(release.clone());
                        }
                        if let Err(error) = tray::refresh(&main, &current) {
                            log::error!("cannot update the tray icon: {error}");
                        }
                        if manual {
                            start_update(&main, release);
                        }
                    }
                    None if manual => notify::tray_message(&main, "Phramer", LATEST),
                    None => log::info!("no newer version"),
                }
            }
            Err(error) => {
                log::warn!("{FAILED} {error}");
                if manual {
                    notify::tray_message(&main, "Phramer", FAILED);
                }
            }
        }
    });
    if let Err(error) = posted {
        log::error!("cannot report the update check: {error}");
    }
}

/// An update is being asked about, downloaded or started.
static UPDATING: AtomicBool = AtomicBool::new(false);

/// Button ids of the update prompt; the common buttons use the low ones.
const UPDATE_NOW: i32 = 1001;
const LATER: i32 = 1002;
const SKIP: i32 = 1003;

/// 15.0.0's `startUpdateAndRestart`: asks, then installs, on a thread of its
/// own so the tray stays responsive.
fn start_update<R: Runtime>(app: &AppHandle<R>, release: Release) {
    use std::sync::atomic::Ordering::AcqRel;

    // Without a verifiable installer there is nothing safe to apply
    let (Some(msi_url), Some(sha_url)) = (release.msi_url.clone(), release.sha_url.clone()) else {
        open_release_page(&release.html_url);
        return;
    };
    if UPDATING.swap(true, AcqRel) {
        return;
    }
    let app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("update".to_owned())
        .spawn(move || {
            match ask(&release) {
                UPDATE_NOW => match install(&release.version, &msi_url, &sha_url) {
                    // msiexec closes this copy anyway; the script reopens it
                    Ok(()) => app.exit(0),
                    Err(reason) => notify::tray_message(&app, "Phramer", reason),
                },
                SKIP => skip(&app, release.version),
                _ => {}
            }
            UPDATING.store(false, std::sync::atomic::Ordering::Release);
        });
    if let Err(error) = spawned {
        UPDATING.store(false, std::sync::atomic::Ordering::Release);
        log::error!("cannot start the update: {error}");
    }
}

/// Skip this version: `ignoreUpdateToVersion`, and the badge goes.
fn skip<R: Runtime>(app: &AppHandle<R>, version: String) {
    let main = app.clone();
    let posted = app.run_on_main_thread(move || {
        if let Ok(mut available) = AVAILABLE.lock() {
            *available = None;
        }
        let path = main.state::<DaemonState>().settings_path.clone();
        let mut current = settings::load(&path);
        current.set_ignore_update_to_version(&version);
        if let Err(error) = current.save(&path) {
            log::error!("cannot skip version {version}: {error}");
        }
        if let Err(error) = tray::refresh(&main, &current) {
            log::error!("cannot update the tray icon: {error}");
        }
    });
    if let Err(error) = posted {
        log::error!("cannot skip the update: {error}");
    }
}

/// The question 15.0.0 asked; the button chosen (Esc and the close box
/// count as Later).
fn ask(release: &Release) -> i32 {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::Controls::{
        TASKDIALOG_BUTTON, TASKDIALOGCONFIG, TASKDIALOGCONFIG_0, TDF_ALLOW_DIALOG_CANCELLATION,
        TDF_USE_HICON_MAIN,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::LoadIconW;

    let Some(task_dialog) = crate::about::task_dialog_indirect() else {
        log::error!("cannot ask about the update: no TaskDialogIndirect");
        return LATER;
    };
    let wide = |text: &str| -> Vec<u16> { text.encode_utf16().chain(Some(0)).collect() };
    let title = wide("Phramer");
    let heading = wide(&format!(
        "Update to version {} and restart?",
        release.version
    ));
    let content = wide(&format!(
        "Phramer will close while the update installs and reopen when it is done. \
         Windows will ask for administrator approval; if this account has no \
         administrator rights, administrator credentials can be entered at that \
         prompt.\n\nRelease notes: {}",
        release.html_url
    ));
    let labels = [
        (UPDATE_NOW, wide("Update now")),
        (LATER, wide("Later")),
        (SKIP, wide("Skip this version")),
    ];
    let buttons = labels.each_ref().map(|(id, text)| TASKDIALOG_BUTTON {
        nButtonID: *id,
        pszButtonText: text.as_ptr(),
    });
    // SAFETY: the module of this process and a resource id; a missing icon
    // gives null, and the dialog then shows none.
    let icon = unsafe {
        LoadIconW(
            GetModuleHandleW(std::ptr::null()),
            crate::about::APP_ICON as usize as _,
        )
    };
    let config = TASKDIALOGCONFIG {
        cbSize: size_of::<TASKDIALOGCONFIG>() as u32,
        dwFlags: TDF_ALLOW_DIALOG_CANCELLATION
            | if icon.is_null() {
                0
            } else {
                TDF_USE_HICON_MAIN
            },
        pszWindowTitle: title.as_ptr(),
        Anonymous1: TASKDIALOGCONFIG_0 { hMainIcon: icon },
        pszMainInstruction: heading.as_ptr(),
        pszContent: content.as_ptr(),
        cButtons: buttons.len() as u32,
        pButtons: buttons.as_ptr(),
        nDefaultButton: UPDATE_NOW,
        ..Default::default()
    };
    let mut chosen = LATER;
    // SAFETY: `config`, its strings and buttons outlive the modal call;
    // `chosen` is a valid out-parameter, the others may be null.
    let result = unsafe {
        task_dialog(
            &config,
            &mut chosen,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if result < 0 {
        log::error!("cannot ask about the update ({result:#x})");
        return LATER;
    }
    chosen
}

/// Checksum, installer, verification, script: 15.0.0's
/// `downloadUpdateInstaller` and `applyUpdate`. The error is the tray
/// message 15.0.0 showed.
fn install(version: &str, msi_url: &str, sha_url: &str) -> Result<(), &'static str> {
    use std::os::windows::process::CommandExt;
    use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;

    // The checksum first: it is tiny, and there is no point downloading an
    // installer that could never be verified
    let text = fetch_text(sha_url, None).map_err(|error| {
        log::warn!("update checksum: {error}");
        "Could not download the update checksum."
    })?;
    let expected = parse_checksum(&text).ok_or("The update checksum file is malformed.")?;
    let name = installer_name(version).ok_or("Could not save the installer to disk.")?;
    let installer = download_with_progress(msi_url).map_err(|error| match error {
        FetchError::Canceled => "Update canceled.",
        FetchError::Failed(error) => {
            log::warn!("update installer: {error}");
            "Could not download the update installer."
        }
    })?;
    // A mismatched installer must never be offered for execution
    if sha256_hex(&installer) != expected {
        return Err("The downloaded installer failed verification and was discarded.");
    }
    let temp = std::env::temp_dir();
    let msi = temp.join(name);
    std::fs::write(&msi, &installer).map_err(|error| {
        log::error!("cannot write {}: {error}", msi.display());
        "Could not save the installer to disk."
    })?;
    let prepare = |error: std::io::Error| {
        log::error!("update script: {error}");
        "Could not prepare the update script."
    };
    let exe = std::env::current_exe().map_err(prepare)?;
    let script = temp.join("phramer-update.cmd");
    std::fs::write(&script, update_script(&msi, &exe)).map_err(prepare)?;
    std::process::Command::new("cmd.exe")
        .arg("/C")
        .arg(&script)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(prepare)?;
    log::info!("installing version {version} from {}", msi.display());
    Ok(())
}

/// How far the installer download is, shared with its progress dialog.
#[derive(Default)]
struct Progress {
    received: AtomicUsize,
    /// 0 while unknown.
    total: AtomicUsize,
    /// The download ended, either way.
    done: AtomicBool,
    /// The dialog closed itself because of `done`, not Cancel.
    finished: AtomicBool,
    canceled: AtomicBool,
}

/// The MSI, under 15.0.0's "Downloading Phramer update…" with Cancel.
fn download_with_progress(url: &str) -> Result<Vec<u8>, FetchError> {
    use std::sync::atomic::Ordering::{Acquire, Release};

    let progress = Arc::new(Progress::default());
    let shared = Arc::clone(&progress);
    let url = url.to_owned();
    let worker = std::thread::Builder::new()
        .name("update-download".to_owned())
        .spawn(move || {
            let result = fetch(&url, None, MAX_INSTALLER, &mut |received, total| {
                shared.received.store(received, Release);
                shared.total.store(total.unwrap_or(0), Release);
                !shared.canceled.load(Acquire)
            });
            shared.done.store(true, Release);
            result
        })
        .map_err(|error| FetchError::Failed(error.to_string()))?;
    show_progress(&progress);
    if !progress.finished.load(Acquire) {
        progress.canceled.store(true, Release);
    }
    let result = worker
        .join()
        .unwrap_or_else(|_| Err(FetchError::Failed("the download stopped".to_owned())));
    if progress.canceled.load(Acquire) {
        return Err(FetchError::Canceled);
    }
    result
}

/// Modal until the download ends or Cancel is pressed. Without task
/// dialogs it only waits.
fn show_progress(progress: &Progress) {
    use std::sync::atomic::Ordering::Acquire;
    use windows_sys::Win32::UI::Controls::{
        TASKDIALOGCONFIG, TDCBF_CANCEL_BUTTON, TDF_ALLOW_DIALOG_CANCELLATION, TDF_CALLBACK_TIMER,
        TDF_SHOW_PROGRESS_BAR,
    };

    let Some(task_dialog) = crate::about::task_dialog_indirect() else {
        while !progress.done.load(Acquire) {
            std::thread::sleep(Duration::from_millis(100));
        }
        progress
            .finished
            .store(true, std::sync::atomic::Ordering::Release);
        return;
    };
    let wide = |text: &str| -> Vec<u16> { text.encode_utf16().chain(Some(0)).collect() };
    let title = wide("Phramer");
    let heading = wide("Downloading Phramer update…");
    let config = TASKDIALOGCONFIG {
        cbSize: size_of::<TASKDIALOGCONFIG>() as u32,
        dwFlags: TDF_ALLOW_DIALOG_CANCELLATION | TDF_SHOW_PROGRESS_BAR | TDF_CALLBACK_TIMER,
        dwCommonButtons: TDCBF_CANCEL_BUTTON,
        pszWindowTitle: title.as_ptr(),
        pszMainInstruction: heading.as_ptr(),
        pfCallback: Some(progress_callback),
        lpCallbackData: std::ptr::from_ref(progress) as isize,
        ..Default::default()
    };
    // SAFETY: `config`, its strings and `progress` (the callback's data)
    // outlive the modal call; the out-parameters may be null.
    let result = unsafe {
        task_dialog(
            &config,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if result < 0 {
        log::error!("cannot show the download progress ({result:#x})");
        while !progress.done.load(Acquire) {
            std::thread::sleep(Duration::from_millis(100));
        }
        progress
            .finished
            .store(true, std::sync::atomic::Ordering::Release);
    }
}

/// Moves the bar on the dialog's timer and closes it once the download
/// ends.
unsafe extern "system" fn progress_callback(
    hwnd: windows_sys::Win32::Foundation::HWND,
    notification: u32,
    _wparam: windows_sys::Win32::Foundation::WPARAM,
    _lparam: windows_sys::Win32::Foundation::LPARAM,
    data: isize,
) -> windows_sys::core::HRESULT {
    use std::sync::atomic::Ordering::{Acquire, Release};
    use windows_sys::Win32::Foundation::S_OK;
    use windows_sys::Win32::UI::Controls::{
        TDM_CLICK_BUTTON, TDM_SET_PROGRESS_BAR_POS, TDN_CREATED, TDN_TIMER,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{IDCANCEL, SendMessageW};

    // TASKDIALOG_NOTIFICATIONS are i32, the callback's parameter u32
    if !matches!(notification as i32, TDN_CREATED | TDN_TIMER) {
        return S_OK;
    }
    // SAFETY: `data` is the `Progress` that `show_progress` borrows for the
    // whole dialog.
    let progress = unsafe { &*(data as *const Progress) };
    if progress.done.load(Acquire) {
        progress.finished.store(true, Release);
        // SAFETY: the dialog's own window; Cancel is its one button.
        unsafe { SendMessageW(hwnd, TDM_CLICK_BUTTON as u32, IDCANCEL as usize, 0) };
        return S_OK;
    }
    let (received, total) = (
        progress.received.load(Acquire),
        progress.total.load(Acquire),
    );
    if let Some(percent) = received.saturating_mul(100).checked_div(total) {
        // SAFETY: the dialog's own window; the range is 0 to 100.
        unsafe { SendMessageW(hwnd, TDM_SET_PROGRESS_BAR_POS as u32, percent.min(100), 0) };
    }
    S_OK
}

/// Opens a release page in the browser.
fn open_release_page(url: &str) {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    // The address came from the network: nothing but the project's pages
    if !url.starts_with(RELEASE_PAGE_PREFIX) {
        log::warn!("not opening {url}: not a Phramer release page");
        return;
    }
    let verb: Vec<u16> = "open".encode_utf16().chain(Some(0)).collect();
    let file: Vec<u16> = url.encode_utf16().chain(Some(0)).collect();
    // SAFETY: both strings are NUL-terminated and outlive the call; the
    // other pointers are null.
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    // Values up to 32 are errors
    if result as usize <= 32 {
        log::error!("cannot open {url} (code {})", result as usize);
    }
}

#[derive(Debug)]
enum FetchError {
    Canceled,
    Failed(String),
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Canceled => f.write_str("canceled"),
            Self::Failed(error) => f.write_str(error),
        }
    }
}

/// A small text answer: a release document or a checksum file.
fn fetch_text(url: &str, accept: Option<&str>) -> Result<String, String> {
    let body = fetch(url, accept, MAX_RESPONSE, &mut |_, _| true).map_err(|e| e.to_string())?;
    String::from_utf8(body).map_err(|_| "the answer is not text".to_owned())
}

/// An HTTPS GET with WinHTTP (system proxy and certificates), following
/// redirects (release assets live on a CDN host); the body, at most `limit`
/// bytes. `progress` hears bytes so far and the total, if known, and stops
/// the download by returning false.
fn fetch(
    url: &str,
    accept: Option<&str>,
    limit: usize,
    progress: &mut dyn FnMut(usize, Option<usize>) -> bool,
) -> Result<Vec<u8>, FetchError> {
    use windows_sys::Win32::Networking::WinHttp::{
        INTERNET_DEFAULT_HTTPS_PORT, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE,
        WINHTTP_QUERY_CONTENT_LENGTH, WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
        WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest,
        WinHttpQueryDataAvailable, WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse,
        WinHttpSendRequest, WinHttpSetTimeouts,
    };

    /// A WinHTTP handle, closed when dropped.
    struct Handle(*mut core::ffi::c_void);
    impl Drop for Handle {
        fn drop(&mut self) {
            if !self.0.is_null() {
                // SAFETY: an open WinHTTP handle owned by this value.
                unsafe { WinHttpCloseHandle(self.0) };
            }
        }
    }
    let wide = |text: &str| -> Vec<u16> { text.encode_utf16().chain(Some(0)).collect() };
    let failed = |call: &str| {
        FetchError::Failed(format!(
            "{call} failed: {}",
            std::io::Error::last_os_error()
        ))
    };
    let (host, path) =
        split_url(url).ok_or_else(|| FetchError::Failed(format!("not an https address: {url}")))?;

    let agent = wide(&format!("Phramer/{}", phramer_core::VERSION));
    // SAFETY: `agent` is NUL-terminated and outlives the call.
    let session = Handle(unsafe {
        WinHttpOpen(
            agent.as_ptr(),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            std::ptr::null(),
            std::ptr::null(),
            0,
        )
    });
    if session.0.is_null() {
        return Err(failed("WinHttpOpen"));
    }
    // SAFETY: an open session; timeouts in milliseconds.
    unsafe { WinHttpSetTimeouts(session.0, 15_000, 15_000, 15_000, 30_000) };
    let host = wide(host);
    // SAFETY: an open session; `host` is NUL-terminated.
    let connection =
        Handle(unsafe { WinHttpConnect(session.0, host.as_ptr(), INTERNET_DEFAULT_HTTPS_PORT, 0) });
    if connection.0.is_null() {
        return Err(failed("WinHttpConnect"));
    }
    let (verb, path) = (wide("GET"), wide(path));
    // SAFETY: an open connection; the strings are NUL-terminated, the rest
    // null (HTTP/1.1, no referrer, any type).
    let request = Handle(unsafe {
        WinHttpOpenRequest(
            connection.0,
            verb.as_ptr(),
            path.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        )
    });
    if request.0.is_null() {
        return Err(failed("WinHttpOpenRequest"));
    }
    let headers = accept.map(|accept| wide(&format!("Accept: {accept}")));
    // SAFETY: an open request; `headers` is null or NUL-terminated (length
    // -1 says so); no body.
    let sent = unsafe {
        WinHttpSendRequest(
            request.0,
            headers
                .as_ref()
                .map_or(std::ptr::null(), |headers| headers.as_ptr()),
            if headers.is_some() { u32::MAX } else { 0 },
            std::ptr::null(),
            0,
            0,
            0,
        ) != 0
            && WinHttpReceiveResponse(request.0, std::ptr::null_mut()) != 0
    };
    if !sent {
        return Err(failed("the request"));
    }
    let mut status = 0u32;
    let mut size = size_of::<u32>() as u32;
    // SAFETY: asks for the status as a number into `status`, `size` bytes.
    let queried = unsafe {
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            std::ptr::null(),
            (&raw mut status).cast(),
            &mut size,
            std::ptr::null_mut(),
        )
    };
    if queried == 0 {
        return Err(failed("WinHttpQueryHeaders"));
    }
    if status != 200 {
        return Err(FetchError::Failed(format!("HTTP status {status}")));
    }
    let mut length = 0u32;
    let mut size = size_of::<u32>() as u32;
    // SAFETY: as above; a chunked answer has no length, and the call fails.
    let total = (unsafe {
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_CONTENT_LENGTH | WINHTTP_QUERY_FLAG_NUMBER,
            std::ptr::null(),
            (&raw mut length).cast(),
            &mut size,
            std::ptr::null_mut(),
        )
    } != 0)
        .then_some(length as usize);
    let mut body = Vec::new();
    loop {
        let mut available = 0u32;
        // SAFETY: an open request; `available` is a valid out-parameter.
        if unsafe { WinHttpQueryDataAvailable(request.0, &mut available) } == 0 {
            return Err(failed("WinHttpQueryDataAvailable"));
        }
        if available == 0 {
            break;
        }
        let start = body.len();
        body.resize(start + available as usize, 0);
        let mut read = 0u32;
        // SAFETY: `body` has `available` writable bytes from `start`.
        let ok = unsafe {
            WinHttpReadData(
                request.0,
                body[start..].as_mut_ptr().cast(),
                available,
                &mut read,
            )
        };
        if ok == 0 {
            return Err(failed("WinHttpReadData"));
        }
        body.truncate(start + read as usize);
        if body.len() > limit {
            return Err(FetchError::Failed("the answer is too large".to_owned()));
        }
        if !progress(body.len(), total) {
            return Err(FetchError::Canceled);
        }
    }
    Ok(body)
}

/// The tray icon with 15.0.0's update badge: an amber dot, outlined in
/// dark grey, in the bottom-right corner. `rgba` is straight RGBA8.
pub(crate) fn badge(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    const AMBER: [f64; 3] = [255.0, 193.0, 7.0];
    const OUTLINE: [f64; 3] = [32.0, 32.0, 32.0];
    let mut out = rgba.to_vec();
    let side = f64::from(width.min(height));
    // 15.0.0 drew on 64 pixels: a 25-pixel dot 2 pixels in, a 3-pixel pen
    let unit = side / 64.0;
    let diameter = side * 2.0 / 5.0;
    let radius = diameter / 2.0;
    let center_x = f64::from(width) - 2.0 * unit - radius;
    let center_y = f64::from(height) - 2.0 * unit - radius;
    let pen = 3.0 * unit;
    for y in 0..height {
        for x in 0..width {
            let dx = f64::from(x) + 0.5 - center_x;
            let dy = f64::from(y) + 0.5 - center_y;
            let distance = (dx * dx + dy * dy).sqrt();
            // Coverage of the dot with its outline, and of the fill inside
            let outer = (radius + pen / 2.0 - distance + 0.5).clamp(0.0, 1.0);
            if outer <= 0.0 {
                continue;
            }
            let inner = (radius - pen / 2.0 - distance + 0.5).clamp(0.0, 1.0);
            let at = ((y * width + x) * 4) as usize;
            let Some(pixel) = out.get_mut(at..at + 4) else {
                continue;
            };
            for channel in 0..3 {
                let badge = OUTLINE[channel] + (AMBER[channel] - OUTLINE[channel]) * inner;
                let below = f64::from(pixel[channel]);
                pixel[channel] = (below + (badge - below) * outer).round() as u8;
            }
            let alpha = f64::from(pixel[3]) / 255.0;
            pixel[3] = ((alpha + (1.0 - alpha) * outer) * 255.0).round() as u8;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    #[test]
    fn versions_compare_by_number() {
        assert!(v("16.0.0") > v("15.0.0"));
        assert!(v("15.0.0") > v("14.1.7"));
        assert!(v("14.1.10") > v("14.1.7"));
        assert!(v("16.0.1") > v("16.0.0"));
        assert_eq!(v("v16.0.0"), v("16.0.0"));
        assert_eq!(v("16.0").cmp(&v("16.0.0")), Ordering::Equal);
        assert!(v("16") > v("15.9.9"));
        assert_eq!(v("16.0.0+build.5").cmp(&v("16.0.0")), Ordering::Equal);
    }

    #[test]
    fn prereleases_come_before_their_release() {
        assert!(v("16.0.0-beta.1") < v("16.0.0"));
        assert!(v("16.0.0-beta.1") > v("15.0.0"));
        assert!(v("16.0.0-alpha") < v("16.0.0-beta"));
        assert!(v("16.0.0-beta.2") < v("16.0.0-beta.10"));
        assert!(v("16.0.0-beta") < v("16.0.0-beta.1"));
        assert!(v("16.0.0-1") < v("16.0.0-rc"));
        assert!(v("16.0.0-rc.1") > v("16.0.0-beta.9"));
    }

    #[test]
    fn malformed_versions_are_rejected() {
        for text in ["", "v", "16.x", "16..0", "16.0.0-", "latest"] {
            assert_eq!(Version::parse(text), None, "{text}");
        }
    }

    const RELEASE: &str = r#"{
        "url": "https://api.github.com/repos/MichaelNguyen5653/Phramer/releases/1",
        "html_url": "https://github.com/MichaelNguyen5653/Phramer/releases/tag/v16.0.1",
        "tag_name": "v16.0.1",
        "name": "Phramer 16.0.1",
        "draft": false,
        "prerelease": false,
        "assets": [
            {"name": "phramer-16.0.1-win64.zip",
             "browser_download_url": "https://github.com/MichaelNguyen5653/Phramer/releases/download/v16.0.1/phramer-16.0.1-win64.zip"},
            {"name": "Phramer-16.0.1-win64.msi",
             "browser_download_url": "https://github.com/MichaelNguyen5653/Phramer/releases/download/v16.0.1/Phramer-16.0.1-win64.msi"},
            {"name": "Phramer-16.0.1-win64.msi.sha256sum",
             "browser_download_url": "https://github.com/MichaelNguyen5653/Phramer/releases/download/v16.0.1/Phramer-16.0.1-win64.msi.sha256sum"}
        ]
    }"#;

    const MSI_URL: &str = "https://github.com/MichaelNguyen5653/Phramer/releases/download/v16.0.1/Phramer-16.0.1-win64.msi";
    const SHA_URL: &str = "https://github.com/MichaelNguyen5653/Phramer/releases/download/v16.0.1/Phramer-16.0.1-win64.msi.sha256sum";

    #[test]
    fn the_release_document_is_read() {
        assert_eq!(
            parse_release(RELEASE),
            Ok(Release {
                version: "16.0.1".into(),
                html_url: "https://github.com/MichaelNguyen5653/Phramer/releases/tag/v16.0.1"
                    .into(),
                msi_url: Some(MSI_URL.into()),
                sha_url: Some(SHA_URL.into()),
            })
        );
        // The 14.1.3 mirror was tagged without a `v`
        let bare = RELEASE.replace("\"v16.0.1\"", "\"14.1.3\"");
        assert_eq!(parse_release(&bare).map(|r| r.version), Ok("14.1.3".into()));
        assert!(parse_release("{}").is_err());
        assert!(parse_release("<html>rate limited</html>").is_err());
        assert!(parse_release(r#"{"tag_name": 5, "html_url": "x"}"#).is_err());
    }

    #[test]
    fn installers_are_taken_only_from_the_projects_downloads() {
        // No assets: the release page is all there is
        let bare = parse_release(
            r#"{"tag_name": "v16.0.1", "html_url": "https://github.com/MichaelNguyen5653/Phramer/releases/tag/v16.0.1"}"#,
        )
        .unwrap();
        assert_eq!((bare.msi_url, bare.sha_url), (None, None));
        // An installer hosted anywhere else is never downloaded
        let elsewhere = RELEASE.replace(
            "https://github.com/MichaelNguyen5653/Phramer/releases/download/",
            "https://example.com/MichaelNguyen5653/Phramer/releases/download/",
        );
        let release = parse_release(&elsewhere).unwrap();
        assert_eq!((release.msi_url, release.sha_url), (None, None));
        // Matched by suffix, in any case, as 15.0.0 did
        let upper = RELEASE.replace("-win64.msi\",", "-win64.MSI\",");
        assert_eq!(
            parse_release(&upper).unwrap().msi_url.as_deref(),
            Some(MSI_URL)
        );
    }

    #[test]
    fn the_checksum_file_gives_its_first_word() {
        let hash = "BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD";
        assert_eq!(
            parse_checksum(&format!("{hash} *Phramer-16.0.1-win64.msi\n")),
            Some(hash.to_lowercase())
        );
        assert_eq!(
            parse_checksum(&format!("  {hash}\r\n")),
            Some(hash.to_lowercase())
        );
        // "The update checksum file is malformed."
        for text in ["", "abc", &hash[1..], &format!("{}g", &hash[1..])] {
            assert_eq!(parse_checksum(text), None, "{text}");
        }
    }

    #[test]
    fn the_installer_hash_is_lowercase_hex() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn the_installer_is_named_after_a_plain_version() {
        assert_eq!(
            installer_name("16.0.1").as_deref(),
            Some("Phramer-16.0.1-win64.msi")
        );
        assert_eq!(
            installer_name("16.0.0-beta.2").as_deref(),
            Some("Phramer-16.0.0-beta.2-win64.msi")
        );
        // The version came from the network and ends up in a script
        for version in ["", "16.0.1\"&calc", "..\\16", "16/0", "16 0", "16%TEMP%"] {
            assert_eq!(installer_name(version), None, "{version}");
        }
    }

    #[test]
    fn the_script_installs_then_starts_phramer_again() {
        let script = update_script(
            Path::new(r"C:\Temp\Phramer-16.0.1-win64.msi"),
            Path::new(r"C:\Program Files\Phramer\phramer.exe"),
        );
        assert_eq!(
            script,
            "@echo off\r\n\
             msiexec /i \"C:\\Temp\\Phramer-16.0.1-win64.msi\" /passive /norestart\r\n\
             start \"\" \"C:\\Program Files\\Phramer\\phramer.exe\"\r\n\
             del \"%~f0\"\r\n"
        );
    }

    #[test]
    fn only_https_addresses_are_fetched() {
        assert_eq!(
            split_url("https://github.com/a/b.msi?x=1"),
            Some(("github.com", "/a/b.msi?x=1"))
        );
        assert_eq!(
            split_url("https://api.github.com"),
            Some(("api.github.com", "/"))
        );
        assert_eq!(split_url("http://github.com/a"), None);
        assert_eq!(split_url("https:///a"), None);
        assert_eq!(split_url("github.com/a"), None);
    }

    #[test]
    fn only_newer_unskipped_releases_count() {
        let release = parse_release(RELEASE).unwrap();
        assert_eq!(newer("16.0.0", "", release.clone()), Some(release.clone()));
        assert_eq!(newer("16.0.1", "", release.clone()), None);
        assert_eq!(newer("16.1.0", "", release.clone()), None);
        // Skipped, or an older skip that does not cover it
        assert_eq!(newer("16.0.0", "16.0.1", release.clone()), None);
        assert_eq!(
            newer("16.0.0", "16.0.0", release.clone()),
            Some(release.clone())
        );
        // A 15.x or 14.x client sees 16
        assert_eq!(newer("14.1.7", "", release.clone()), Some(release.clone()));
        // A prerelease build sees its release
        assert_eq!(newer("16.0.1-beta.3", "", release.clone()), Some(release));
    }

    #[test]
    fn the_badge_sits_in_the_bottom_right_corner() {
        let (width, height) = (64u32, 64u32);
        let clear = vec![0u8; (width * height * 4) as usize];
        let badged = badge(&clear, width, height);
        let pixel = |x: u32, y: u32| {
            let i = ((y * width + x) * 4) as usize;
            [badged[i], badged[i + 1], badged[i + 2], badged[i + 3]]
        };
        // The dot's centre is amber and opaque, the opposite corner untouched
        assert_eq!(pixel(49, 49), [255, 193, 7, 255]);
        assert_eq!(pixel(5, 5), [0, 0, 0, 0]);
        // Its rim is the dark outline
        assert_eq!(pixel(49, 62)[..3], [32, 32, 32]);
    }

    #[test]
    #[ignore = "live: queries the GitHub releases API"]
    fn the_latest_release_is_fetched() {
        let json = fetch_text(RELEASES_URL, Some(GITHUB_JSON)).unwrap();
        let release = parse_release(&json).unwrap();
        println!("latest release: {release:?}");
        assert!(Version::parse(&release.version).is_some());
        assert!(release.html_url.starts_with(RELEASE_PAGE_PREFIX));
    }

    #[test]
    #[ignore = "live: downloads the latest release's checksum through its CDN redirect"]
    fn the_checksum_asset_is_fetched() {
        let json = fetch_text(RELEASES_URL, Some(GITHUB_JSON)).unwrap();
        let release = parse_release(&json).unwrap();
        let Some(url) = release.sha_url else {
            println!("the latest release has no checksum asset");
            return;
        };
        let text = fetch_text(&url, None).unwrap();
        assert!(parse_checksum(&text).is_some(), "{text}");
    }
}
