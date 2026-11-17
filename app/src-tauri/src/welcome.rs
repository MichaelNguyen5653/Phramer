//! The welcome (`app/welcome.html`): a few short steps on first run that
//! name the capture hotkey and offer Print Screen and launch at sign-in,
//! each applied the moment it is switched.
//!
//! When it is due follows 15.0.0, which showed two things at start-up: the
//! welcome tour once per version (`welcomeTourShownFor`, silenced within a
//! major version by `welcomeTourDisabled`; a new major version always shows
//! and counts as seen only once its last step is reached), and the Print
//! Screen question while `showWelcomeMessage` was on (switched off before it
//! was answered). Both are this one window now. The tray's What's New opens
//! it at any time. Unlike 15.0.0, only a first run (nothing recorded in
//! `welcomeTourShownFor`) must be seen to the end; after an upgrade from an
//! older major version it shows but can be skipped.

use std::sync::atomic::{AtomicBool, Ordering};

use phramer_core::config::Settings;
use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};

use crate::daemon::DaemonState;
use crate::{printscreen, settings, snipping};

/// The window's label; `capabilities/settings.json` names it.
pub(crate) const LABEL: &str = "welcome";
const TITLE: &str = "Welcome to Phramer";
const WIDTH: f64 = 600.0;
const HEIGHT: f64 = 440.0;
/// Set (to anything) to start without the welcome: unattended runs such
/// as the live smoke tests and scripted deployments.
const SKIP_VARIABLE: &str = "PHRAMER_NO_WELCOME";

/// Whether the open welcome must be seen to its end.
#[derive(Default)]
pub(crate) struct Welcome {
    mandatory: AtomicBool,
}

/// This version as 15.0.0 recorded its own (`APP_VERSION`, `v` first).
fn current_version() -> String {
    format!("v{}", phramer_core::VERSION)
}

/// `majorOf`: the major version of `v15.0.0` or `15.0.0`.
fn major_of(version: &str) -> Option<u32> {
    version
        .trim()
        .trim_start_matches('v')
        .split('.')
        .next()?
        .parse()
        .ok()
}

/// `None` when the welcome is not due; otherwise whether it is mandatory
/// (a first run only).
fn due(settings: &Settings) -> Option<bool> {
    let current = current_version();
    let shown = settings.welcome_tour_shown_for();
    let tour = if shown.trim_start_matches('v') == current.trim_start_matches('v') {
        None
    } else {
        let first_run = shown.trim().is_empty();
        let new_major = major_of(&shown) != major_of(&current);
        (new_major || !settings.welcome_tour_disabled()).then_some(first_run)
    };
    tour.or_else(|| settings.show_welcome_message().then_some(false))
}

/// What is recorded as the welcome opens: an optional one counts as seen
/// however it is closed, and the Print Screen question is not asked again.
fn mark_opened(settings: &mut Settings, mandatory: bool) {
    if !mandatory {
        settings.set_welcome_tour_shown_for(&current_version());
    }
    settings.set_show_welcome_message(false);
}

/// At start-up: opens the welcome if it is due.
pub(crate) fn show_if_due<R: Runtime>(app: &AppHandle<R>) {
    if std::env::var_os(SKIP_VARIABLE).is_some() {
        log::info!("welcome: skipped ({SKIP_VARIABLE} is set)");
        return;
    }
    let path = app.state::<DaemonState>().settings_path.clone();
    let mut current = settings::load(&path);
    let Some(mandatory) = due(&current) else {
        return;
    };
    mark_opened(&mut current, mandatory);
    if let Err(error) = current.save(&path) {
        log::error!("welcome: cannot record it was shown: {error}");
    }
    open(app, mandatory);
}

/// Opens the welcome, or brings the open one forward.
pub(crate) fn open<R: Runtime>(app: &AppHandle<R>, mandatory: bool) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    app.state::<Welcome>()
        .mandatory
        .store(mandatory, Ordering::Relaxed);
    let built = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("welcome.html".into()))
        .title(TITLE)
        .inner_size(WIDTH, HEIGHT)
        .resizable(false)
        .maximizable(false)
        .decorations(false)
        .center()
        .visible(false)
        .build();
    match built {
        Ok(_) => log::info!("welcome: window created (mandatory: {mandatory})"),
        Err(error) => log::error!("cannot create the welcome window: {error}"),
    }
}

/// What the welcome shows.
#[derive(Debug, Serialize)]
pub(crate) struct WelcomeState {
    /// `TAKE_SCREENSHOT` as the settings file spells it.
    capture_hotkey: String,
    /// Phramer, not Windows' snipping tool, answers Print Screen.
    print_screen: bool,
    startup_launch: bool,
    mandatory: bool,
    version: &'static str,
    store_build: bool,
    snipping_tool_installed: bool,
}

fn state_of(settings: &Settings, mandatory: bool, print_screen: bool) -> WelcomeState {
    WelcomeState {
        capture_hotkey: settings.shortcut("TAKE_SCREENSHOT").unwrap_or_default(),
        print_screen,
        startup_launch: settings.startup_launch(),
        mandatory,
        version: phramer_core::VERSION,
        store_build: cfg!(feature = "store"),
        snipping_tool_installed: snipping::is_available(),
    }
}

#[tauri::command]
pub(crate) fn welcome_state<R: Runtime>(app: AppHandle<R>) -> WelcomeState {
    let current = settings::load(&app.state::<DaemonState>().settings_path);
    let mandatory = app.state::<Welcome>().mandatory.load(Ordering::Relaxed);
    state_of(&current, mandatory, !printscreen::snipping_claims_key())
}

/// The last step is on screen: a mandatory welcome now counts as seen.
#[tauri::command]
pub(crate) fn welcome_reached_end<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let welcome = app.state::<Welcome>();
    if !welcome.mandatory.swap(false, Ordering::Relaxed) {
        return Ok(());
    }
    let path = app.state::<DaemonState>().settings_path.clone();
    let mut current = Settings::load(&path).map_err(|error| error.to_string())?;
    current.set_welcome_tour_shown_for(&current_version());
    current.save(&path).map_err(|error| error.to_string())?;
    log::info!("welcome: seen to the end");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(text: &str) -> Settings {
        Settings::parse(text.as_bytes())
    }

    #[test]
    fn only_a_first_run_is_mandatory() {
        assert_eq!(due(&Settings::default()), Some(true));
        // 15.0.0 recorded itself with a v; an upgrade shows it, skippable
        assert_eq!(
            due(&settings(
                "welcomeTourShownFor=v15.0.0\nshowWelcomeMessage=false\n"
            )),
            Some(false)
        );
        // "Don't show again" does not hold across a major version
        assert_eq!(
            due(&settings(
                "welcomeTourShownFor=v15.0.0\nwelcomeTourDisabled=true\nshowWelcomeMessage=false\n"
            )),
            Some(false)
        );
    }

    #[test]
    fn this_version_seen_means_not_due_unless_asked_for_again() {
        let seen = format!(
            "welcomeTourShownFor={}\nshowWelcomeMessage=false\n",
            current_version()
        );
        assert_eq!(due(&settings(&seen)), None);
        let bare = format!(
            "welcomeTourShownFor={}\nshowWelcomeMessage=false\n",
            phramer_core::VERSION
        );
        assert_eq!(due(&settings(&bare)), None);
        // "Show the first-run welcome again" in Settings
        let again = format!(
            "welcomeTourShownFor={}\nshowWelcomeMessage=true\n",
            current_version()
        );
        assert_eq!(due(&settings(&again)), Some(false));
    }

    #[test]
    fn a_new_minor_version_is_optional_and_can_be_silenced() {
        let major = major_of(phramer_core::VERSION).unwrap();
        let older = format!("welcomeTourShownFor=v{major}.0.0-old\nshowWelcomeMessage=false\n");
        assert_eq!(due(&settings(&older)), Some(false));
        let silenced = format!("{older}welcomeTourDisabled=true\n");
        assert_eq!(due(&settings(&silenced)), None);
    }

    #[test]
    fn opening_records_only_existing_keys() {
        let mut optional = settings("[General]\r\nunknownKey=kept\r\n");
        mark_opened(&mut optional, false);
        let text = String::from_utf8(optional.to_bytes()).unwrap();
        assert_eq!(
            text,
            format!(
                "[General]\r\nunknownKey=kept\r\nwelcomeTourShownFor={}\r\nshowWelcomeMessage=false\r\n",
                current_version()
            )
        );
        // A mandatory one is recorded at its end instead
        let mut mandatory = Settings::default();
        mark_opened(&mut mandatory, true);
        assert_eq!(mandatory.welcome_tour_shown_for(), "");
        assert!(!mandatory.show_welcome_message());
        assert_eq!(due(&mandatory), Some(true));
    }

    #[test]
    fn the_state_names_the_capture_hotkey() {
        let state = state_of(&Settings::default(), true, false);
        assert_eq!(state.capture_hotkey, "Meta+Shift+x");
        assert!(state.startup_launch);
        assert!(state.mandatory);
        assert!(!state.print_screen);
    }

    #[test]
    fn major_versions_parse_with_or_without_v() {
        assert_eq!(major_of("v15.0.0"), Some(15));
        assert_eq!(major_of("16.0.0"), Some(16));
        assert_eq!(major_of(""), None);
    }
}
