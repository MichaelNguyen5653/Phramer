//! The settings window (`app/settings.html`): every option 15.0.0's
//! configuration window had, regrouped, each saved the moment it changes.
//!
//! Opened from the tray's Configuration entry and `phramer config`. One
//! window at most; asking again brings it forward. The webview exists only
//! while the window is open. Every change goes through the typed setters of
//! `phramer-core`, so the file stays readable by 14.x and 15.x, unknown keys
//! are carried through, and no key is added that 15.0.0 did not know. After
//! each write the daemon re-reads the file at once (tray, hotkeys); the
//! capture overlay and the editor read it whenever they open.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use phramer_core::config::{ButtonType, CaptureRegionMode, Color, ConfigError, Settings};
use phramer_overlay::{TOOLBAR_ORDER, button_info, is_tool};
use serde::Serialize;
use serde_json::{Value, json};
use tauri::{
    AppHandle, Emitter, Manager, Runtime, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    WindowEvent,
};

use crate::daemon::{self, DaemonState};
use crate::{hotkeys, printscreen, settings, snipping, startup};

/// The window's label; `capabilities/settings.json` names it.
pub(crate) const LABEL: &str = "settings";
const TITLE: &str = "Phramer Settings";
/// Sent to the window when the file changed; the page reads it again.
pub(crate) const CHANGED_EVENT: &str = "settings-changed";
const WIDTH: f64 = 880.0;
const HEIGHT: f64 = 620.0;
const MIN_WIDTH: f64 = 680.0;
const MIN_HEIGHT: f64 = 480.0;
/// The formats a capture can be saved as (`ImageFormat::from_extension`).
/// 15.0.0 listed every format Qt could write; this build writes these.
const EXTENSIONS: [&str; 4] = ["png", "jpg", "jpeg", "bmp"];
/// An imported file larger than this is not a settings file.
const IMPORT_LIMIT: usize = 1 << 20;

/// Generates reading and writing for the on/off settings.
macro_rules! bool_keys {
    ($($key:literal => $get:ident / $set:ident,)*) => {
        const BOOL_KEYS: &[&str] = &[$($key),*];

        fn read_bool(settings: &Settings, key: &str) -> Option<bool> {
            match key {
                $($key => Some(settings.$get()),)*
                _ => None,
            }
        }

        fn write_bool(settings: &mut Settings, key: &str, value: bool) {
            match key {
                $($key => settings.$set(value),)*
                _ => {}
            }
        }
    };
}

bool_keys! {
    "startupLaunch" => startup_launch / set_startup_launch,
    "autoOpenInEditor" => auto_open_in_editor / set_auto_open_in_editor,
    "copyOnDoubleClick" => copy_on_double_click / set_copy_on_double_click,
    "saveAfterCopy" => save_after_copy / set_save_after_copy,
    "savePathFixed" => save_path_fixed / set_save_path_fixed,
    "showQuitPrompt" => show_quit_prompt / set_show_quit_prompt,
    "checkForUpdates" => check_for_updates / set_check_for_updates,
    "disabledTrayIcon" => disabled_tray_icon / set_disabled_tray_icon,
    "showHelp" => show_help / set_show_help,
    "showToolLabels" => show_tool_labels / set_show_tool_labels,
    "showEditorHint" => show_editor_hint / set_show_editor_hint,
    "autoCloseIdleDaemon" => auto_close_idle_daemon / set_auto_close_idle_daemon,
    "showDesktopNotification" => show_desktop_notification / set_show_desktop_notification,
    "showAbortNotification" => show_abort_notification / set_show_abort_notification,
    "showStartupLaunchMessage" => show_startup_launch_message / set_show_startup_launch_message,
    "allowMultipleGuiInstances" => allow_multiple_gui_instances / set_allow_multiple_gui_instances,
    "saveLastRegion" => save_last_region / set_save_last_region,
    "showSidePanelButton" => show_side_panel_button / set_show_side_panel_button,
    "copyPathAfterSave" => copy_path_after_save / set_copy_path_after_save,
    "antialiasingPinZoom" => antialiasing_pin_zoom / set_antialiasing_pin_zoom,
    "insecurePixelate" => insecure_pixelate / set_insecure_pixelate,
    "reverseArrow" => reverse_arrow / set_reverse_arrow,
    "showMagnifier" => show_magnifier / set_show_magnifier,
    "squareMagnifier" => square_magnifier / set_square_magnifier,
    "predefinedColorPaletteLarge" => predefined_color_palette_large / set_predefined_color_palette_large,
    "showWelcomeMessage" => show_welcome_message / set_show_welcome_message,
    "videoCaptureEnabled" => video_capture_enabled / set_video_capture_enabled,
    "ignorePrntScrForcesSnipping" => ignore_prnt_scr_forces_snipping / set_ignore_prnt_scr_forces_snipping,
}

/// Generates reading and writing for the whole-number settings.
macro_rules! int_keys {
    ($($key:literal => $get:ident / $set:ident,)*) => {
        const INT_KEYS: &[&str] = &[$($key),*];

        fn read_int(settings: &Settings, key: &str) -> Option<i32> {
            match key {
                $($key => Some(settings.$get()),)*
                _ => None,
            }
        }

        fn write_int(settings: &mut Settings, key: &str, value: i32) -> Result<(), ConfigError> {
            match key {
                $($key => settings.$set(value),)*
                _ => Ok(()),
            }
        }
    };
}

int_keys! {
    "undoLimit" => undo_limit / set_undo_limit,
    "contrastOpacity" => contrast_opacity / set_contrast_opacity,
    "showSelectionGeometry" => show_selection_geometry / set_show_selection_geometry,
    "showSelectionGeometryHideTime" => show_selection_geometry_hide_time / set_show_selection_geometry_hide_time,
    "jpegQuality" => jpeg_quality / set_jpeg_quality,
}

/// The rest, written by hand in [`read`] and [`write`].
const OTHER_KEYS: [&str; 9] = [
    "captureRegionMode",
    "savePath",
    "saveAsFileExtension",
    "filenamePattern",
    "uiLanguage",
    "uiColor",
    "contrastUiColor",
    "userColors",
    "buttons",
];

/// Every `[General]` key the window shows.
fn keys() -> impl Iterator<Item = &'static str> {
    BOOL_KEYS
        .iter()
        .chain(INT_KEYS)
        .chain(OTHER_KEYS.iter())
        .copied()
}

fn hex(color: Color) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b)
}

/// `#rrggbb`, as the colour fields send it.
fn parse_hex(text: &str) -> Option<Color> {
    let digits = text.strip_prefix('#')?;
    if digits.len() != 6 || !digits.is_ascii() {
        return None;
    }
    let channel = |at: usize| u8::from_str_radix(digits.get(at..at + 2)?, 16).ok();
    Some(Color::rgb(channel(0)?, channel(2)?, channel(4)?))
}

/// The value of `key` as the page shows it.
fn read(settings: &Settings, key: &str) -> Option<Value> {
    if let Some(value) = read_bool(settings, key) {
        return Some(Value::Bool(value));
    }
    if let Some(value) = read_int(settings, key) {
        return Some(json!(value));
    }
    Some(match key {
        "captureRegionMode" => json!(settings.capture_region_mode() as i32),
        "savePath" => json!(
            settings
                .save_path()
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_default()
        ),
        // None stored saves PNG, the same as `png`
        "saveAsFileExtension" => {
            let extension = settings.save_as_file_extension();
            json!(if extension.is_empty() {
                "png".to_owned()
            } else {
                extension
            })
        }
        "filenamePattern" => json!(settings.filename_pattern()),
        "uiLanguage" => json!(settings.ui_language()),
        "uiColor" => json!(hex(settings.ui_color())),
        "contrastUiColor" => json!(hex(settings.contrast_ui_color())),
        "userColors" => json!(
            settings
                .user_colors()
                .into_iter()
                .map(|entry| entry.map(hex))
                .collect::<Vec<_>>()
        ),
        "buttons" => json!(
            settings
                .buttons()
                .into_iter()
                .map(ButtonType::id)
                .collect::<Vec<_>>()
        ),
        _ => return None,
    })
}

fn config_error(error: &ConfigError) -> String {
    match error {
        ConfigError::InvalidValue { key, value } => format!("{value} is not allowed for {key}"),
        other => other.to_string(),
    }
}

/// Sets `key` from the page's `value`, through the setter that validates
/// it. Writes nothing when the value is refused.
fn write(settings: &mut Settings, key: &str, value: &Value) -> Result<(), String> {
    let wrong = || format!("{value} is not a valid value for {key}");
    if BOOL_KEYS.contains(&key) {
        write_bool(settings, key, value.as_bool().ok_or_else(wrong)?);
        return Ok(());
    }
    if INT_KEYS.contains(&key) {
        let number = value
            .as_i64()
            .and_then(|number| i32::try_from(number).ok())
            .ok_or_else(wrong)?;
        return write_int(settings, key, number).map_err(|error| config_error(&error));
    }
    let text = || value.as_str().ok_or_else(wrong);
    match key {
        "captureRegionMode" => {
            let mode = match value.as_i64() {
                Some(0) => CaptureRegionMode::SelectMonitor,
                Some(1) => CaptureRegionMode::ActiveMonitor,
                Some(2) => CaptureRegionMode::AllMonitors,
                _ => return Err(wrong()),
            };
            settings.set_capture_region_mode(mode);
        }
        "savePath" => {
            let folder = PathBuf::from(text()?);
            if !folder.is_dir() {
                return Err(format!("{} is not a folder", folder.display()));
            }
            settings.set_save_path(&folder);
        }
        "saveAsFileExtension" => {
            let extension = text()?.trim_start_matches('.').to_ascii_lowercase();
            if !EXTENSIONS.contains(&extension.as_str()) {
                return Err(format!("Phramer cannot save {extension} files"));
            }
            settings
                .set_save_as_file_extension(&extension)
                .map_err(|error| config_error(&error))?;
        }
        "filenamePattern" => {
            let pattern = text()?;
            if pattern.trim().is_empty() {
                return Err("The file name pattern cannot be empty".to_owned());
            }
            settings.set_filename_pattern(pattern);
        }
        "uiLanguage" => settings.set_ui_language(text()?),
        "uiColor" => settings.set_ui_color(parse_hex(text()?).ok_or_else(wrong)?),
        "contrastUiColor" => settings.set_contrast_ui_color(parse_hex(text()?).ok_or_else(wrong)?),
        "userColors" => {
            let entries = value.as_array().ok_or_else(wrong)?;
            let palette = entries
                .iter()
                .map(|entry| match entry {
                    Value::Null => Some(None),
                    Value::String(text) => parse_hex(text).map(Some),
                    _ => None,
                })
                .collect::<Option<Vec<_>>>()
                .ok_or_else(wrong)?;
            settings
                .set_user_colors(&palette)
                .map_err(|_| format!("The palette holds 3 to 17 entries, not {}", palette.len()))?;
        }
        "buttons" => {
            let ids = value.as_array().ok_or_else(wrong)?;
            let buttons = ids
                .iter()
                .map(|id| {
                    let id = id.as_i64()?;
                    TOOLBAR_ORDER
                        .into_iter()
                        .find(|button| i64::from(button.id()) == id)
                })
                .collect::<Option<Vec<_>>>()
                .ok_or_else(wrong)?;
            settings.set_buttons(&buttons);
        }
        _ => return Err(format!("{key} is not a setting")),
    }
    Ok(())
}

/// What each setting reads as with nothing stored. The palette follows the
/// large-palette choice, as it does when `userColors` is absent.
fn defaults(settings: &Settings) -> BTreeMap<&'static str, Value> {
    let mut base = Settings::default();
    base.set_predefined_color_palette_large(settings.predefined_color_palette_large());
    keys()
        .filter_map(|key| Some((key, read(&base, key)?)))
        .collect()
}

/// Sets `key` back to what it reads as with nothing stored.
fn reset(settings: &mut Settings, key: &str) -> Result<(), String> {
    let value = defaults(settings)
        .remove(key)
        .ok_or_else(|| format!("{key} is not a setting"))?;
    write(settings, key, &value)
}

/// 15.0.0's Reset: every setting but the shortcuts back to its default.
/// Launch at sign-in also needs its registry entry, so the caller sets it.
fn reset_all(settings: &mut Settings) -> Result<(), String> {
    let all: Vec<&str> = keys().filter(|key| *key != "startupLaunch").collect();
    // The palette's default depends on the large-palette choice: that first
    reset(settings, "predefinedColorPaletteLarge")?;
    for key in all {
        // A default the build cannot represent (no Pictures folder) stays
        if let Err(error) = reset(settings, key) {
            log::warn!("settings: cannot reset {key}: {error}");
        }
    }
    Ok(())
}

/// One row of the Shortcuts section.
#[derive(Debug, Serialize)]
pub(crate) struct ShortcutRow {
    /// The `[Shortcuts]` entry; empty for a fixed key.
    name: &'static str,
    description: &'static str,
    group: &'static str,
    value: String,
    default: String,
    /// A system-wide hotkey rather than a key of the capture screen.
    global: bool,
}

/// Shortcuts that have no toolbar button, in 15.0.0's order
/// (`ShortcutsWidget::loadShortcuts`).
const EXTRA_SHORTCUTS: [(&str, &str, &str); 19] = [
    ("TYPE_TOGGLE_PANEL", "Toggle tool settings", "Capture"),
    ("CAPTURE_MODE_SCREENSHOT", "Screenshot mode", "Capture"),
    (
        "CAPTURE_MODE_VIDEO",
        "Video mode (hands off to Snipping Tool)",
        "Capture",
    ),
    ("TYPE_GRAB_COLOR", "Grab a color from the screen", "Capture"),
    ("TYPE_RESIZE_LEFT", "Resize selection left 1px", "Selection"),
    (
        "TYPE_RESIZE_RIGHT",
        "Resize selection right 1px",
        "Selection",
    ),
    ("TYPE_RESIZE_UP", "Resize selection up 1px", "Selection"),
    ("TYPE_RESIZE_DOWN", "Resize selection down 1px", "Selection"),
    (
        "TYPE_SYM_RESIZE_LEFT",
        "Symmetrically decrease width by 2px",
        "Selection",
    ),
    (
        "TYPE_SYM_RESIZE_RIGHT",
        "Symmetrically increase width by 2px",
        "Selection",
    ),
    (
        "TYPE_SYM_RESIZE_UP",
        "Symmetrically increase height by 2px",
        "Selection",
    ),
    (
        "TYPE_SYM_RESIZE_DOWN",
        "Symmetrically decrease height by 2px",
        "Selection",
    ),
    ("TYPE_SELECT_ALL", "Select entire screen", "Selection"),
    ("TYPE_MOVE_LEFT", "Move selection left 1px", "Selection"),
    ("TYPE_MOVE_RIGHT", "Move selection right 1px", "Selection"),
    ("TYPE_MOVE_UP", "Move selection up 1px", "Selection"),
    ("TYPE_MOVE_DOWN", "Move selection down 1px", "Selection"),
    (
        "TYPE_COMMIT_CURRENT_TOOL",
        "Commit text in text area",
        "Capture",
    ),
    (
        "TYPE_DELETE_CURRENT_TOOL",
        "Delete selected drawn object",
        "Capture",
    ),
];

/// The system-wide hotkeys (Print Screen is shown on its own).
const GLOBAL_SHORTCUTS: [(&str, &str); 2] = [
    ("TAKE_SCREENSHOT", "Capture screen"),
    ("RECORD_VIDEO", "Record video (Snipping Tool)"),
];

/// Keys that cannot be changed, listed for reference as 15.0.0 did.
const FIXED_SHORTCUTS: [(&str, &str, &str); 3] = [
    ("Quit capture", "Esc", "Capture"),
    ("Show color picker", "Right Click", "Capture"),
    ("Change the tool's size", "Mouse Wheel", "Capture"),
];

/// Every shortcut row, global hotkeys first.
fn shortcut_rows(settings: &Settings) -> Vec<ShortcutRow> {
    let base = Settings::default();
    let row =
        |name: &'static str, description: &'static str, group: &'static str, global| ShortcutRow {
            name,
            description,
            group,
            value: settings.shortcut(name).unwrap_or_default(),
            default: base.shortcut(name).unwrap_or_default(),
            global,
        };
    let mut rows: Vec<ShortcutRow> = GLOBAL_SHORTCUTS
        .iter()
        .map(|&(name, description)| row(name, description, "Global", true))
        .collect();
    rows.extend(TOOLBAR_ORDER.into_iter().map(|button| {
        let info = button_info(button);
        let group = if is_tool(button) { "Tools" } else { "Actions" };
        row(info.shortcut, info.description, group, false)
    }));
    // TYPE_CANCEL closes the selection list in 15.0.0
    rows.extend(
        EXTRA_SHORTCUTS
            .iter()
            .chain(&[("TYPE_CANCEL", "Cancel current selection", "Selection")])
            .map(|&(name, description, group)| row(name, description, group, false)),
    );
    if settings.copy_on_double_click()
        && let Some(copy) = rows.iter().find(|row| row.name == "TYPE_COPY")
    {
        let description = copy.description;
        rows.push(ShortcutRow {
            name: "",
            description,
            group: "Actions",
            value: "Left Double-click".to_owned(),
            default: String::new(),
            global: false,
        });
    }
    rows.extend(
        FIXED_SHORTCUTS
            .iter()
            .map(|&(description, key, group)| ShortcutRow {
                name: "",
                description,
                group,
                value: key.to_owned(),
                default: String::new(),
                global: false,
            }),
    );
    rows
}

/// A key sequence's first chord as a set of modifiers and a key, both
/// lower-case, so `Shift+Meta+X` and `Meta+Shift+x` compare equal.
fn canonical(text: &str) -> Option<(Vec<String>, String)> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let (modifiers, key) = match text.strip_suffix("++") {
        Some(rest) => (rest, "+"),
        None => text.rsplit_once('+').unwrap_or(("", text)),
    };
    let mut modifiers: Vec<String> = modifiers
        .split('+')
        .filter(|name| !name.is_empty())
        .map(str::to_ascii_lowercase)
        .collect();
    modifiers.sort();
    Some((modifiers, key.to_ascii_lowercase()))
}

/// The spelling to store for `sequence`: another action's own spelling of
/// the same keys, so `Settings::set_shortcut` sees them as the same
/// sequence (an explicit one conflicts, a default one gives way).
fn respell(settings: &Settings, name: &str, sequence: &str) -> String {
    let Some(wanted) = canonical(sequence) else {
        return String::new();
    };
    shortcut_rows(settings)
        .into_iter()
        .filter(|row| !row.name.is_empty() && row.name != name)
        .find(|row| canonical(&row.value).as_ref() == Some(&wanted))
        .map_or_else(|| sequence.trim().to_owned(), |row| row.value)
}

fn shortcut_error(settings: &Settings, error: &ConfigError) -> String {
    match error {
        ConfigError::ShortcutConflict { shortcut, action } => {
            let rows = shortcut_rows(settings);
            let owner = rows
                .iter()
                .find(|row| row.name == action.as_str())
                .map_or(action.as_str(), |row| row.description);
            format!("{shortcut} is already used by \u{201c}{owner}\u{201d}")
        }
        ConfigError::ReservedShortcut(shortcut) => {
            format!("{shortcut} is reserved and cannot be a shortcut")
        }
        ConfigError::InvalidValue { value, .. } => {
            format!("{value} is not a key Phramer understands")
        }
        other => other.to_string(),
    }
}

/// Binds `sequence` to the action `name`; an empty sequence clears it.
fn set_shortcut(settings: &mut Settings, name: &str, sequence: &str) -> Result<(), String> {
    let spelled = respell(settings, name, sequence);
    let result = settings.set_shortcut(name, &spelled);
    result.map_err(|error| shortcut_error(settings, &error))
}

/// A toolbar button the page can switch on or off.
#[derive(Debug, Serialize)]
pub(crate) struct ToolbarChoice {
    id: i32,
    name: &'static str,
    description: &'static str,
    icon: &'static str,
}

/// Everything the page shows.
#[derive(Debug, Serialize)]
pub(crate) struct Snapshot {
    values: BTreeMap<&'static str, Value>,
    defaults: BTreeMap<&'static str, Value>,
    shortcuts: Vec<ShortcutRow>,
    toolbar: Vec<ToolbarChoice>,
    extensions: [&'static str; 4],
    /// Windows opens its snipping tool on Print Screen.
    snipping_claims_print_screen: bool,
    snipping_tool_installed: bool,
    /// Key sequences wanted as hotkeys that are not registered.
    hotkey_failures: Vec<String>,
    file: String,
    version: &'static str,
    /// Launch at sign-in comes from the Store package, not the registry.
    store_build: bool,
}

/// What the machine reports alongside the file.
struct Machine {
    snipping_claims_print_screen: bool,
    snipping_tool_installed: bool,
    hotkey_failures: Vec<String>,
}

fn snapshot_of(settings: &Settings, path: &Path, machine: Machine) -> Snapshot {
    Snapshot {
        values: keys()
            .filter_map(|key| Some((key, read(settings, key)?)))
            .collect(),
        defaults: defaults(settings),
        shortcuts: shortcut_rows(settings),
        toolbar: TOOLBAR_ORDER
            .into_iter()
            .map(|button| {
                let info = button_info(button);
                ToolbarChoice {
                    id: button.id(),
                    name: info.name,
                    description: info.description,
                    icon: info.icon,
                }
            })
            .collect(),
        extensions: EXTENSIONS,
        snipping_claims_print_screen: machine.snipping_claims_print_screen,
        snipping_tool_installed: machine.snipping_tool_installed,
        hotkey_failures: machine.hotkey_failures,
        file: path.to_string_lossy().into_owned(),
        version: phramer_core::VERSION,
        store_build: cfg!(feature = "store"),
    }
}

/// Loads, changes and saves the file in one step; nothing is saved when
/// `change` fails.
fn update(
    path: &Path,
    change: impl FnOnce(&mut Settings) -> Result<(), String>,
) -> Result<Settings, String> {
    let mut current = Settings::load(path).map_err(|error| error.to_string())?;
    change(&mut current)?;
    current.save(path).map_err(|error| error.to_string())?;
    Ok(current)
}

/// Writes the current file to `target` as it is.
fn export_to(path: &Path, target: &Path) -> Result<(), String> {
    let bytes = Settings::load(path)
        .map_err(|error| error.to_string())?
        .to_bytes();
    std::fs::write(target, bytes).map_err(|error| format!("{}: {error}", target.display()))
}

/// Whether `bytes` read like a settings file: a section header or a key
/// Phramer knows.
fn looks_like_settings(bytes: &[u8]) -> bool {
    let text = String::from_utf8_lossy(bytes);
    text.lines().any(|line| {
        let line = line.trim();
        line == "[General]"
            || line == "[Shortcuts]"
            || line
                .split_once('=')
                .is_some_and(|(key, _)| keys().any(|known| known == key.trim()))
    })
}

/// 15.0.0's Import: the chosen file replaces the settings file, byte for
/// byte, after a check that it is one.
fn import_from(path: &Path, source: &Path) -> Result<(), String> {
    let bytes = std::fs::read(source).map_err(|error| format!("{}: {error}", source.display()))?;
    if bytes.len() > IMPORT_LIMIT || !looks_like_settings(&bytes) {
        return Err(format!(
            "{} is not a Phramer settings file",
            source.display()
        ));
    }
    Settings::parse(&bytes)
        .save(path)
        .map_err(|error| error.to_string())
}

/// `FileNameEditor::showParsedPattern`: the name a capture saved now gets.
fn preview(pattern: &str) -> String {
    phramer_overlay::expand_pattern(pattern, &phramer_overlay::LocalTime::now())
}

/// Whether Phramer may write in `folder` (15.0.0 refused a read-only one).
fn writable(folder: &Path) -> bool {
    let probe = folder.join(format!(".phramer-write-test-{}", std::process::id()));
    let written = std::fs::write(&probe, b"").is_ok();
    if written {
        let _ = std::fs::remove_file(&probe);
    }
    written
}

// --- The window and its commands ----------------------------------------------

fn settings_path<R: Runtime>(app: &AppHandle<R>) -> PathBuf {
    app.state::<DaemonState>().settings_path.clone()
}

fn snapshot<R: Runtime>(app: &AppHandle<R>) -> Snapshot {
    let path = settings_path(app);
    let current = settings::load(&path);
    let machine = Machine {
        snipping_claims_print_screen: printscreen::snipping_claims_key(),
        snipping_tool_installed: snipping::is_available(),
        hotkey_failures: hotkeys::failures(app, &current),
    };
    snapshot_of(&current, &path, machine)
}

/// The file changed, here or elsewhere (`phramer config --...`, another
/// program): an open settings window shows it, as 15.0.0's did on
/// `fileChanged`.
pub(crate) fn changed<R: Runtime>(app: &AppHandle<R>) {
    if app.get_webview_window(LABEL).is_some()
        && let Err(error) = app.emit_to(LABEL, CHANGED_EVENT, ())
    {
        log::warn!("settings: cannot tell the window: {error}");
    }
}

/// The daemon picks the change up now rather than at its next poll.
fn apply_live<R: Runtime>(app: &AppHandle<R>) {
    daemon::settings_changed(app);
}

/// [`apply_live`] from a thread that is not the main one.
fn apply_live_later<R: Runtime>(app: &AppHandle<R>) {
    let target = app.clone();
    if let Err(error) = app.run_on_main_thread(move || apply_live(&target)) {
        log::error!("settings: cannot apply the change: {error}");
    }
}

/// Opens the settings window, or brings the open one forward.
pub(crate) fn open<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    let built = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("settings.html".into()))
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
            window.on_window_event(move |event| {
                if matches!(event, WindowEvent::Destroyed) {
                    // Closed while recording a shortcut: the hotkeys come back
                    hotkeys::apply(&handle, &settings::load(&settings_path(&handle)));
                    log::info!("settings: window closed");
                }
            });
            log::info!("settings: window created");
        }
        Err(error) => log::error!("cannot create the settings window: {error}"),
    }
}

/// The page has painted: the window shows. Shared by the settings and
/// welcome windows.
#[tauri::command]
pub(crate) fn ui_ready<R: Runtime>(window: WebviewWindow<R>) {
    if let Err(error) = window.show() {
        log::error!("cannot show {}: {error}", window.label());
    }
    let _ = window.set_focus();
    log::info!("{}: visible", window.label());
}

#[tauri::command]
pub(crate) fn settings_snapshot<R: Runtime>(app: AppHandle<R>) -> Snapshot {
    snapshot(&app)
}

/// Saves one setting and applies it.
#[tauri::command]
pub(crate) fn settings_set<R: Runtime>(
    app: AppHandle<R>,
    key: String,
    value: Value,
) -> Result<Snapshot, String> {
    let path = settings_path(&app);
    if key == "startupLaunch" {
        let enable = value.as_bool().ok_or("startupLaunch is on or off")?;
        let mut current = Settings::load(&path).map_err(|error| error.to_string())?;
        // Saves the file itself, with the launch entry
        startup::set_startup_launch(&mut current, &path, enable)
            .map_err(|error| error.to_string())?;
    } else {
        update(&path, |current| write(current, &key, &value))?;
    }
    log::info!("settings: {key} = {value}");
    apply_live(&app);
    Ok(snapshot(&app))
}

/// Puts one setting back to its default.
#[tauri::command]
pub(crate) fn settings_reset<R: Runtime>(
    app: AppHandle<R>,
    key: String,
) -> Result<Snapshot, String> {
    let value = defaults(&settings::load(&settings_path(&app)))
        .remove(key.as_str())
        .ok_or_else(|| format!("{key} is not a setting"))?;
    settings_set(app, key, value)
}

/// Every setting but the shortcuts back to its default.
#[tauri::command]
pub(crate) fn settings_reset_all<R: Runtime>(app: AppHandle<R>) -> Result<Snapshot, String> {
    let path = settings_path(&app);
    let mut current = update(&path, reset_all)?;
    let launch = Settings::default().startup_launch();
    startup::set_startup_launch(&mut current, &path, launch).map_err(|error| error.to_string())?;
    log::info!("settings: reset to defaults");
    apply_live(&app);
    Ok(snapshot(&app))
}

/// Binds a key sequence to a shortcut; an empty one clears it.
#[tauri::command]
pub(crate) fn settings_set_shortcut<R: Runtime>(
    app: AppHandle<R>,
    name: String,
    sequence: String,
) -> Result<Snapshot, String> {
    update(&settings_path(&app), |current| {
        set_shortcut(current, &name, &sequence)
    })?;
    log::info!("settings: shortcut {name} = {sequence:?}");
    apply_live(&app);
    Ok(snapshot(&app))
}

/// While the shortcut recorder listens, the hotkeys are let go so their
/// keys reach the page.
#[tauri::command]
pub(crate) fn settings_pause_hotkeys<R: Runtime>(app: AppHandle<R>, paused: bool) -> Snapshot {
    if paused {
        hotkeys::suspend(&app);
    } else {
        hotkeys::apply(&app, &settings::load(&settings_path(&app)));
    }
    snapshot(&app)
}

/// Gives Print Screen to Phramer, or back to Windows' snipping tool.
#[tauri::command]
pub(crate) fn settings_print_screen<R: Runtime>(
    app: AppHandle<R>,
    phramer: bool,
) -> Result<Snapshot, String> {
    printscreen::set_snipping_claims_key(!phramer)
        .map_err(|error| format!("The registry could not be changed: {error}"))?;
    log::info!(
        "settings: Print Screen for {}",
        if phramer { "Phramer" } else { "Windows" }
    );
    // Registered again: Windows may have refused it while it held the key
    hotkeys::suspend(&app);
    hotkeys::apply(&app, &settings::load(&settings_path(&app)));
    Ok(snapshot(&app))
}

#[tauri::command]
pub(crate) fn settings_preview_pattern(pattern: String) -> String {
    preview(&pattern)
}

/// The folder dialog for `savePath`. `None` when cancelled.
#[tauri::command]
pub(crate) async fn settings_choose_folder<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
) -> Result<Option<Snapshot>, String> {
    use tauri_plugin_dialog::DialogExt;
    let path = settings_path(&app);
    let mut dialog = app
        .dialog()
        .file()
        .set_parent(&window)
        .set_title("Choose a Folder");
    if let Some(folder) = settings::load(&path).save_path() {
        dialog = dialog.set_directory(folder);
    }
    let Some(folder) = dialog
        .blocking_pick_folder()
        .and_then(|picked| picked.into_path().ok())
    else {
        return Ok(None);
    };
    if !writable(&folder) {
        return Err(format!("Phramer cannot write to {}", folder.display()));
    }
    let value = json!(folder.to_string_lossy());
    update(&path, |current| write(current, "savePath", &value))?;
    apply_live_later(&app);
    Ok(Some(snapshot(&app)))
}

/// Saves a copy of the settings file where the user says. The path written,
/// or `None` when cancelled.
#[tauri::command]
pub(crate) async fn settings_export<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let path = settings_path(&app);
    let Some(target) = app
        .dialog()
        .file()
        .set_parent(&window)
        .set_title("Export Settings")
        .set_file_name("phramer.ini")
        .add_filter("Settings", &["ini"])
        .blocking_save_file()
        .and_then(|picked| picked.into_path().ok())
    else {
        return Ok(None);
    };
    // The file itself: nothing to do (15.0.0 did the same)
    if target == path {
        return Ok(None);
    }
    export_to(&path, &target)?;
    log::info!("settings: exported to {}", target.display());
    Ok(Some(target.to_string_lossy().into_owned()))
}

/// Replaces the settings with a file the user picks, and applies them.
#[tauri::command]
pub(crate) async fn settings_import<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
) -> Result<Option<Snapshot>, String> {
    use tauri_plugin_dialog::DialogExt;
    let path = settings_path(&app);
    let Some(source) = app
        .dialog()
        .file()
        .set_parent(&window)
        .set_title("Import Settings")
        .add_filter("Settings", &["ini"])
        .blocking_pick_file()
        .and_then(|picked| picked.into_path().ok())
    else {
        return Ok(None);
    };
    import_from(&path, &source)?;
    // The launch entry follows the imported choice
    let mut current = settings::load(&path);
    let launch = current.startup_launch();
    startup::set_startup_launch(&mut current, &path, launch).map_err(|error| error.to_string())?;
    log::info!("settings: imported {}", source.display());
    apply_live_later(&app);
    Ok(Some(snapshot(&app)))
}

/// Opens the settings window from another page (the welcome).
#[tauri::command]
pub(crate) async fn settings_open<R: Runtime>(app: AppHandle<R>) {
    let target = app.clone();
    if let Err(error) = app.run_on_main_thread(move || open(&target)) {
        log::error!("cannot open the settings window: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../crates/phramer-core/tests/fixtures/v15.0.0-keys.txt"
    ));

    fn known_keys() -> Vec<&'static str> {
        FIXTURE.lines().filter(|line| !line.is_empty()).collect()
    }

    /// The `section/key` names in an INI text (no section is `General`).
    fn keys_in(text: &str) -> Vec<String> {
        let mut section = "General".to_owned();
        let mut found = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if let Some(name) = line
                .strip_prefix('[')
                .and_then(|rest| rest.strip_suffix(']'))
            {
                section = name.to_owned();
            } else if let Some((key, _)) = line.split_once('=') {
                found.push(format!("{section}/{}", key.trim()));
            }
        }
        found
    }

    fn machine() -> Machine {
        Machine {
            snipping_claims_print_screen: true,
            snipping_tool_installed: true,
            hotkey_failures: Vec::new(),
        }
    }

    fn temp_file(name: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("phramer-settings-ui-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        folder.join("phramer.ini")
    }

    /// A value different from the default for each setting.
    fn changed(key: &str, settings: &Settings) -> Value {
        let current = read(settings, key).unwrap();
        match key {
            "captureRegionMode" => json!(0),
            "savePath" => json!(std::env::temp_dir().to_string_lossy()),
            "saveAsFileExtension" => json!("jpg"),
            "filenamePattern" => json!("shot_%Y"),
            "uiLanguage" => json!("en"),
            "uiColor" | "contrastUiColor" => json!("#123456"),
            "userColors" => json!([null, "#ffffff", "#000000", "#ff0000"]),
            "buttons" => json!([0, 2, 10]),
            "contrastOpacity" => json!(100),
            "showSelectionGeometry" => json!(1),
            "undoLimit" | "showSelectionGeometryHideTime" | "jpegQuality" => json!(42),
            _ => json!(!current.as_bool().unwrap()),
        }
    }

    #[test]
    fn every_key_shown_is_a_15_0_0_key() {
        let known = known_keys();
        for key in keys() {
            let qualified = format!("General/{key}");
            assert!(
                known.contains(&qualified.as_str()),
                "{key} is not in v15.0.0-keys.txt"
            );
        }
        let rows = shortcut_rows(&Settings::default());
        for row in rows.iter().filter(|row| !row.name.is_empty()) {
            let qualified = format!("Shortcuts/{}", row.name);
            assert!(
                known.contains(&qualified.as_str()),
                "{} is not a shortcut",
                row.name
            );
        }
        // And every shortcut 15.0.0 had is listed, once
        let shortcuts: Vec<&str> = known
            .iter()
            .filter_map(|key| key.strip_prefix("Shortcuts/"))
            .collect();
        for name in &shortcuts {
            let count = rows.iter().filter(|row| row.name == *name).count();
            assert_eq!(count, 1, "{name} listed {count} times");
        }
    }

    #[test]
    fn every_setting_round_trips_and_unknown_keys_stay() {
        let original =
            "[General]\r\nunknownKey=kept\r\nshowHelp=true\r\n\r\n[Mystery]\r\nthing=1\r\n";
        let mut current = Settings::parse(original.as_bytes());
        for key in keys() {
            let value = changed(key, &current);
            write(&mut current, key, &value).unwrap_or_else(|error| panic!("{key}: {error}"));
            let read_back = read(&Settings::parse(&current.to_bytes()), key).unwrap();
            assert_eq!(read_back, value, "{key}");
        }
        let text = String::from_utf8(current.to_bytes()).unwrap();
        assert!(text.contains("unknownKey=kept"), "{text}");
        assert!(text.contains("[Mystery]\r\nthing=1"), "{text}");
        // Nothing but 15.0.0's keys and the file's own were written
        let known = known_keys();
        let before = keys_in(original);
        for key in keys_in(&text) {
            assert!(
                known.contains(&key.as_str()) || before.contains(&key),
                "{key} was added"
            );
        }
        for key in before {
            assert!(keys_in(&text).contains(&key), "{key} was removed");
        }
    }

    #[test]
    fn refused_values_write_nothing() {
        let mut current = Settings::default();
        for (key, value) in [
            ("undoLimit", json!(1000)),
            ("jpegQuality", json!(-1)),
            ("showHelp", json!("yes")),
            ("captureRegionMode", json!(3)),
            ("saveAsFileExtension", json!("webp")),
            ("savePath", json!("C:\\no\\such\\folder\\anywhere")),
            ("filenamePattern", json!("  ")),
            ("uiColor", json!("red")),
            ("userColors", json!([null, "#ffffff"])),
            ("buttons", json!([0, 7])),
            ("nonsense", json!(true)),
        ] {
            assert!(write(&mut current, key, &value).is_err(), "{key} = {value}");
        }
        assert!(current.to_bytes().is_empty());
    }

    #[test]
    fn reset_writes_the_default_and_never_removes_a_key() {
        let original = b"[General]\r\nshowHelp=false\r\njpegQuality=10\r\nunknownKey=kept\r\n";
        let mut current = Settings::parse(original);
        reset(&mut current, "showHelp").unwrap();
        reset(&mut current, "jpegQuality").unwrap();
        assert!(current.show_help());
        assert_eq!(current.jpeg_quality(), 75);
        let text = String::from_utf8(current.to_bytes()).unwrap();
        assert_eq!(
            text,
            "[General]\r\nshowHelp=true\r\njpegQuality=75\r\nunknownKey=kept\r\n"
        );

        let mut all = Settings::parse(original);
        write(&mut all, "uiColor", &json!("#010203")).unwrap();
        set_shortcut(&mut all, "TYPE_PENCIL", "Q").unwrap();
        reset_all(&mut all).unwrap();
        assert!(all.show_help());
        assert_eq!(hex(all.ui_color()), "#3899c2");
        // Shortcuts are kept, as 15.0.0's Reset kept them
        assert_eq!(all.shortcut("TYPE_PENCIL").as_deref(), Some("q"));
        let text = String::from_utf8(all.to_bytes()).unwrap();
        assert!(text.contains("unknownKey=kept"));
        let snapshot = snapshot_of(&all, Path::new("x.ini"), machine());
        for (key, value) in &snapshot.values {
            if *key != "savePath" {
                assert_eq!(Some(value), snapshot.defaults.get(key), "{key}");
            }
        }
    }

    #[test]
    fn the_palette_default_follows_the_large_palette_choice() {
        let mut current = Settings::default();
        write(&mut current, "predefinedColorPaletteLarge", &json!(true)).unwrap();
        let palette = defaults(&current).remove("userColors").unwrap();
        assert_eq!(palette.as_array().map(Vec::len), Some(17));
    }

    #[test]
    fn shortcuts_conflict_with_explicit_bindings_and_take_defaults() {
        let mut current = Settings::default();
        // G is only TYPE_MOVE_OBJECT's default: it gives way
        set_shortcut(&mut current, "TYPE_PENCIL", "G").unwrap();
        assert_eq!(current.shortcut("TYPE_MOVE_OBJECT").as_deref(), Some(""));
        // Now explicit: another action cannot have it
        let error = set_shortcut(&mut current, "TYPE_ARROW", "g").unwrap_err();
        assert_eq!(
            error,
            "g is already used by \u{201c}Set the Pencil as the paint tool\u{201d}"
        );
        // The same keys in another order are the same shortcut
        set_shortcut(&mut current, "TAKE_SCREENSHOT", "Ctrl+Alt+P").unwrap();
        let error = set_shortcut(&mut current, "RECORD_VIDEO", "Alt+Ctrl+p").unwrap_err();
        assert!(error.contains("Capture screen"), "{error}");
        // Win+Shift+X recorded in another order still yields to the default
        let mut fresh = Settings::default();
        set_shortcut(&mut fresh, "RECORD_VIDEO", "Shift+Meta+X").unwrap();
        assert_eq!(
            fresh.shortcut("RECORD_VIDEO").as_deref(),
            Some("Meta+Shift+x")
        );
        assert_eq!(fresh.shortcut("TAKE_SCREENSHOT").as_deref(), Some(""));
        // Reserved keys and clearing
        assert!(set_shortcut(&mut fresh, "TYPE_UNDO", "Esc").is_err());
        set_shortcut(&mut fresh, "TYPE_UNDO", "").unwrap();
        assert_eq!(fresh.shortcut("TYPE_UNDO").as_deref(), Some(""));
    }

    #[test]
    fn canonical_sequences_ignore_order_and_case() {
        assert_eq!(canonical("Meta+Shift+x"), canonical("Shift+Meta+X"));
        assert_ne!(canonical("Ctrl+x"), canonical("Ctrl+Shift+x"));
        assert_eq!(
            canonical("Ctrl++"),
            Some((vec!["ctrl".to_owned()], "+".to_owned()))
        );
        assert_eq!(canonical(""), None);
    }

    #[test]
    fn update_saves_and_a_refused_change_leaves_the_file() {
        let path = temp_file("update");
        std::fs::write(&path, b"[General]\r\nunknownKey=kept\r\n").unwrap();
        update(&path, |current| write(current, "showHelp", &json!(false))).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text, "[General]\r\nunknownKey=kept\r\nshowHelp=false\r\n");
        assert!(update(&path, |current| write(current, "undoLimit", &json!(5000))).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn export_and_import_copy_the_file_byte_for_byte() {
        let path = temp_file("io");
        let folder = path.parent().unwrap().to_owned();
        let original = b"[General]\r\nuiColor=#740096\r\nunknownKey=\"kept, quoted\"\r\n\r\n[Shortcuts]\r\nTYPE_COPY=Ctrl+Shift+c\r\n";
        std::fs::write(&path, original).unwrap();
        let exported = folder.join("exported.ini");
        export_to(&path, &exported).unwrap();
        assert_eq!(std::fs::read(&exported).unwrap(), original);

        std::fs::write(&path, b"[General]\r\nshowHelp=false\r\n").unwrap();
        import_from(&path, &exported).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), original);

        let stranger = folder.join("notes.txt");
        std::fs::write(&stranger, b"just some text\r\n").unwrap();
        assert!(import_from(&path, &stranger).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn the_preview_expands_the_pattern() {
        let name = preview("shot_%Y-%m-%d");
        assert!(name.starts_with("shot_"), "{name}");
        assert_eq!(name.len(), "shot_2026-11-13".len(), "{name}");
        assert!(!name.contains('%'));
        // A slash and a colon cannot be in a file name
        assert_eq!(preview("a/b:c"), "a\u{2044}b-c");
    }

    #[test]
    fn the_snapshot_lists_everything_the_page_needs() {
        let current = Settings::parse(b"[General]\r\ncopyOnDoubleClick=true\r\n");
        let snapshot = snapshot_of(&current, Path::new("C:\\x\\phramer.ini"), machine());
        assert_eq!(snapshot.values.len(), keys().count());
        assert_eq!(snapshot.values.get("showHelp"), Some(&json!(true)));
        assert_eq!(
            snapshot.values.get("filenamePattern"),
            Some(&json!("%F_%H-%M"))
        );
        assert_eq!(snapshot.toolbar.len(), TOOLBAR_ORDER.len());
        assert_eq!(snapshot.shortcuts[0].name, "TAKE_SCREENSHOT");
        assert_eq!(snapshot.shortcuts[0].value, "Meta+Shift+x");
        assert!(snapshot.shortcuts[0].global);
        // The double-click copy is listed while it is on
        assert!(
            snapshot
                .shortcuts
                .iter()
                .any(|row| row.value == "Left Double-click")
        );
        let json = serde_json::to_value(&snapshot).unwrap();
        assert!(json.get("snipping_claims_print_screen").is_some());
    }

    /// The browser tests answer for the daemon with this snapshot
    /// (`app/tests/fixtures/settings-default.json`), so it must stay what
    /// the daemon sends. `PHRAMER_UPDATE_FIXTURES=1` rewrites it.
    #[test]
    fn the_page_fixture_matches_the_default_snapshot() {
        let file = "C:\\Users\\you\\AppData\\Roaming\\phramer\\phramer.ini";
        let snapshot = snapshot_of(&Settings::default(), Path::new(file), machine());
        let mut page = serde_json::to_value(snapshot).unwrap();
        // The Pictures folder differs from machine to machine
        for part in ["values", "defaults"] {
            page[part]["savePath"] = json!("C:\\Users\\you\\Pictures");
        }
        let expected = serde_json::to_string_pretty(&page).unwrap() + "\n";
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/settings-default.json");
        if std::env::var_os("PHRAMER_UPDATE_FIXTURES").is_some() {
            std::fs::write(&fixture, &expected).unwrap();
        }
        let actual = std::fs::read_to_string(&fixture)
            .unwrap_or_default()
            .replace("\r\n", "\n");
        assert!(
            actual == expected,
            "{} is out of date; run with PHRAMER_UPDATE_FIXTURES=1",
            fixture.display()
        );
    }

    #[test]
    fn a_folder_that_takes_files_is_writable() {
        let folder = temp_file("writable").parent().unwrap().to_owned();
        assert!(writable(&folder));
        assert!(!writable(&folder.join("missing")));
        let _ = std::fs::remove_dir_all(&folder);
    }
}
