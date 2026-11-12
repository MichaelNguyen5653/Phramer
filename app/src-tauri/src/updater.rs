//! The update check, 15.0.0's `FlameshotDaemon::getLatestAvailableVersion`
//! and `checkForUpdates`: the latest GitHub release at startup and every
//! 24 hours while `checkForUpdates` is on, or when asked from the tray.
//!
//! Check only. A newer version badges the tray icon and adds "Download
//! version X" to its menu, which opens the release page; nothing is ever
//! downloaded or installed. The Store build has no updater at all (the
//! Store updates it), as 15.0.0's Store edition had none.

use std::cmp::Ordering;
use std::sync::Mutex;
use std::time::Duration;

use tauri::{AppHandle, Manager, Runtime};

use crate::daemon::DaemonState;
use crate::{notify, settings, tray};

/// `GIT_API_URL` of 15.0.0, split for WinHTTP.
const RELEASES_HOST: &str = "api.github.com";
const RELEASES_PATH: &str = "/repos/MichaelNguyen5653/Phramer/releases/latest";
/// Only release pages of the project are opened from the menu.
const RELEASE_PAGE_PREFIX: &str = "https://github.com/MichaelNguyen5653/Phramer/";
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
/// A release document is a few kilobytes; anything far larger is not one.
const MAX_RESPONSE: usize = 4 * 1024 * 1024;

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
}

/// Reads `tag_name` and `html_url` from the releases API's answer.
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
    Ok(Release {
        version,
        html_url: field("html_url")?,
    })
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

/// The tray's "Check for updates" or "Download version X".
pub(crate) fn check_now<R: Runtime>(app: &AppHandle<R>) {
    if cfg!(feature = "store") {
        return;
    }
    let known = AVAILABLE
        .lock()
        .ok()
        .and_then(|available| available.clone());
    if let Some(release) = known {
        open_release_page(&release.html_url);
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
    let found = fetch(RELEASES_HOST, RELEASES_PATH).and_then(|json| parse_release(&json));
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
                        if manual {
                            open_release_page(&release.html_url);
                        }
                        if let Ok(mut available) = AVAILABLE.lock() {
                            *available = Some(release);
                        }
                        if let Err(error) = tray::refresh(&main, &current) {
                            log::error!("cannot update the tray icon: {error}");
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

/// An HTTPS GET with WinHTTP (system proxy and certificates); the body.
fn fetch(host: &str, path: &str) -> Result<String, String> {
    use windows_sys::Win32::Networking::WinHttp::{
        INTERNET_DEFAULT_HTTPS_PORT, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE,
        WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE, WinHttpCloseHandle, WinHttpConnect,
        WinHttpOpen, WinHttpOpenRequest, WinHttpQueryDataAvailable, WinHttpQueryHeaders,
        WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetTimeouts,
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
    let failed = |call: &str| format!("{call} failed: {}", std::io::Error::last_os_error());

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
    let headers = wide("Accept: application/vnd.github+json");
    // SAFETY: an open request; `headers` is NUL-terminated (length -1 says
    // so); no body.
    let sent = unsafe {
        WinHttpSendRequest(
            request.0,
            headers.as_ptr(),
            u32::MAX,
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
        return Err(format!("HTTP status {status}"));
    }
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
        if body.len() > MAX_RESPONSE {
            return Err("the answer is too large".to_owned());
        }
    }
    String::from_utf8(body).map_err(|_| "the answer is not text".to_owned())
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
            {"name": "Phramer-16.0.1-win64.msi",
             "browser_download_url": "https://github.com/x/y/releases/download/v16.0.1/Phramer-16.0.1-win64.msi"},
            {"name": "Phramer-16.0.1-win64.msi.sha256sum",
             "browser_download_url": "https://github.com/x/y/releases/download/v16.0.1/Phramer-16.0.1-win64.msi.sha256sum"}
        ]
    }"#;

    #[test]
    fn the_release_document_is_read() {
        assert_eq!(
            parse_release(RELEASE),
            Ok(Release {
                version: "16.0.1".into(),
                html_url: "https://github.com/MichaelNguyen5653/Phramer/releases/tag/v16.0.1"
                    .into(),
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
        let json = fetch(RELEASES_HOST, RELEASES_PATH).unwrap();
        let release = parse_release(&json).unwrap();
        println!("latest release: {release:?}");
        assert!(Version::parse(&release.version).is_some());
        assert!(release.html_url.starts_with(RELEASE_PAGE_PREFIX));
    }
}
