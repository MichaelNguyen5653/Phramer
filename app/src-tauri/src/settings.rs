//! The settings file: where it is, loading it, applying `phramer config`
//! changes, and noticing when something else rewrites it.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use phramer_core::config::{self, ConfigError, Layout, Settings};

use crate::cli::ConfigChanges;
use crate::startup::{self, StartupError};

/// How often the file is checked for changes. Polled rather than watched:
/// settings are saved by replacing the file, which a per-file watcher loses.
const POLL_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Debug, thiserror::Error)]
pub(crate) enum SettingsError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Startup(#[from] StartupError),
}

/// The installed build's settings file, `%APPDATA%\phramer\phramer.ini`.
pub(crate) fn path() -> Result<PathBuf, ConfigError> {
    config::settings_path(Layout::Installed)
}

/// The settings, or the defaults when the file cannot be read.
pub(crate) fn load(path: &Path) -> Settings {
    Settings::load(path).unwrap_or_else(|error| {
        log::error!("{error}; using default settings");
        Settings::default()
    })
}

/// `phramer config` with options, applied in 15.0.0's order.
pub(crate) fn apply_config_changes(changes: &ConfigChanges) -> Result<(), SettingsError> {
    let path = path()?;
    let mut settings = Settings::load(&path)?;
    if let Some(enable) = changes.autostart {
        // Saves the file itself, together with the launch entry
        startup::set_startup_launch(&mut settings, &path, enable)?;
    }
    if let Some(enable) = changes.notifications {
        settings.set_show_desktop_notification(enable);
    }
    if let Some(pattern) = &changes.filename {
        settings.set_filename_pattern(pattern);
        // 15.0.0 also printed an example of the parsed pattern; filename
        // formatting is not ported yet
        println!("The new pattern is '{pattern}'");
    }
    if let Some(show) = changes.tray_icon {
        settings.set_disabled_tray_icon(!show);
    }
    if let Some(color) = changes.main_color {
        settings.set_ui_color(color);
    }
    if let Some(color) = changes.contrast_color {
        settings.set_contrast_ui_color(color);
    }
    settings.save(&path)?;
    Ok(())
}

/// What identifies one version of the file.
fn fingerprint(path: &Path) -> Option<(SystemTime, u64)> {
    let metadata = std::fs::metadata(path).ok()?;
    Some((metadata.modified().ok()?, metadata.len()))
}

/// Calls `on_change` whenever the file is written, created or removed.
/// `on_change` returns `false` once nobody is listening any more.
pub(crate) fn watch(path: PathBuf, mut on_change: impl FnMut() -> bool + Send + 'static) {
    let spawned = std::thread::Builder::new()
        .name("settings-watch".into())
        .spawn(move || {
            let mut last = fingerprint(&path);
            loop {
                std::thread::sleep(POLL_INTERVAL);
                let current = fingerprint(&path);
                if current != last {
                    last = current;
                    if !on_change() {
                        return;
                    }
                }
            }
        });
    if let Err(error) = spawned {
        log::error!("cannot watch the settings file: {error}");
    }
}
