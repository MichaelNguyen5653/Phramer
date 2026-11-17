//! Launch at sign-in (`startupLaunch`), as 15.0.0's `ConfigHandler` keeps it.
//!
//! The GitHub build writes the HKCU `Run` value `Phramer` (the executable
//! path, unquoted) and an `App Paths\phramer.exe` entry naming its folder.
//! The Store build (`store` feature) declares a startup task in its package
//! manifest instead and never touches the registry.

use std::path::Path;

use phramer_core::config::{ConfigError, Settings};

#[derive(Debug, thiserror::Error)]
pub(crate) enum StartupError {
    #[error(transparent)]
    Settings(#[from] ConfigError),
    #[cfg(not(feature = "store"))]
    #[error("cannot locate the running executable")]
    Executable(#[source] std::io::Error),
    #[cfg(not(feature = "store"))]
    #[error("cannot update registry key {key}")]
    Registry {
        key: &'static str,
        #[source]
        source: std::io::Error,
    },
}

/// `ConfigHandler::setStartupLaunch`: stores the choice and brings the launch
/// entry in line. The entry is checked as well as the setting: with the key
/// absent the stored value already reads as the default, and choosing that
/// default must still create the entry.
pub(crate) fn set_startup_launch(
    settings: &mut Settings,
    settings_path: &Path,
    enable: bool,
) -> Result<(), StartupError> {
    #[cfg(feature = "store")]
    {
        settings.set_startup_launch(enable);
        settings.save(settings_path)?;
        Ok(())
    }
    #[cfg(not(feature = "store"))]
    {
        let exe = current_exe()?;
        if enable == settings.startup_launch() && enable == registry::launch_entry_matches(&exe) {
            return Ok(());
        }
        // Written explicitly: the setter skips a value equal to the default
        settings.set_startup_launch(!enable);
        settings.set_startup_launch(enable);
        settings.save(settings_path)?;
        registry::write_launch_entry(&exe, enable)
    }
}

/// `ConfigHandler::applyDefaultStartupLaunch`, run once per daemon start:
/// for a user who never chose, store the default and make the launch entry
/// match it. Storing it makes this a one-time step, so turning autostart off
/// later is not undone on the next start.
pub(crate) fn apply_default_startup_launch(
    settings: &mut Settings,
    settings_path: &Path,
) -> Result<(), StartupError> {
    #[cfg(feature = "store")]
    {
        let _ = (settings, settings_path);
        Ok(())
    }
    #[cfg(not(feature = "store"))]
    {
        if is_startup_launch_stored(settings) {
            return Ok(());
        }
        let enable = settings.startup_launch();
        settings.set_startup_launch(!enable);
        settings.set_startup_launch(enable);
        settings.save(settings_path)?;
        let exe = current_exe()?;
        if enable != registry::launch_entry_matches(&exe) {
            registry::write_launch_entry(&exe, enable)?;
        }
        Ok(())
    }
}

/// `QSettings::contains("startupLaunch")` for a readable value: rewriting a
/// stored `true`/`false` with itself leaves the file bytes unchanged, while
/// an absent key gets added.
#[cfg(not(feature = "store"))]
fn is_startup_launch_stored(settings: &Settings) -> bool {
    let mut probe = settings.clone();
    let value = probe.startup_launch();
    probe.set_startup_launch(!value);
    probe.set_startup_launch(value);
    probe.to_bytes() == settings.to_bytes()
}

#[cfg(not(feature = "store"))]
fn current_exe() -> Result<std::path::PathBuf, StartupError> {
    std::env::current_exe().map_err(StartupError::Executable)
}

/// Data of the `Run` value: the executable's native path, unquoted
/// (`QDir::toNativeSeparators(applicationFilePath())`).
#[cfg(not(feature = "store"))]
fn run_command_line(exe: &Path) -> String {
    exe.to_string_lossy().replace('/', "\\")
}

/// Data of `App Paths\phramer.exe\Path`: the executable's folder with
/// forward slashes (`QCoreApplication::applicationDirPath()`).
#[cfg(not(feature = "store"))]
fn app_paths_folder(exe: &Path) -> String {
    exe.parent()
        .map(|dir| dir.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default()
}

#[cfg(not(feature = "store"))]
mod registry {
    use std::io;
    use std::path::Path;

    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR};
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ, RRF_RT_REG_SZ,
        RegCloseKey, RegCreateKeyExW, RegDeleteKeyValueW, RegDeleteTreeW, RegGetValueW,
        RegSetValueExW,
    };

    use super::{StartupError, app_paths_folder, run_command_line};

    const RUN_KEY: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run";
    const RUN_VALUE: &str = "Phramer";
    const APP_PATHS_KEY: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\App Paths";
    const APP_PATHS_ENTRY: &str =
        "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\App Paths\\phramer.exe";
    const APP_PATHS_VALUE: &str = "Path";

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(Some(0)).collect()
    }

    fn check(status: WIN32_ERROR, key: &'static str) -> Result<(), StartupError> {
        if status == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(StartupError::Registry {
                key,
                source: io::Error::from_raw_os_error(status as i32),
            })
        }
    }

    /// `verifyLaunchFile`: the `Run` value names exactly this executable.
    pub(super) fn launch_entry_matches(exe: &Path) -> bool {
        read_string(RUN_KEY, RUN_VALUE).is_some_and(|data| data == run_command_line(exe))
    }

    /// `ConfigHandler::writeLaunchEntry` on Windows.
    pub(super) fn write_launch_entry(exe: &Path, enable: bool) -> Result<(), StartupError> {
        if enable {
            set_string(RUN_KEY, RUN_VALUE, &run_command_line(exe))?;
            set_string(APP_PATHS_ENTRY, APP_PATHS_VALUE, &app_paths_folder(exe))
        } else {
            delete_value(RUN_KEY, RUN_VALUE)?;
            delete_tree(APP_PATHS_KEY, "phramer.exe")
        }
    }

    fn read_string(key: &str, value: &str) -> Option<String> {
        let (key, value) = (wide(key), wide(value));
        let mut size: u32 = 0;
        // SAFETY: both strings are NUL-terminated and outlive the call; a
        // null buffer with a size pointer asks for the size only.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut size,
            )
        };
        if status != ERROR_SUCCESS {
            return None;
        }
        let mut buffer = vec![0u16; (size as usize).div_ceil(2)];
        // SAFETY: `buffer` holds `size` bytes, as `size` tells the call.
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

    fn set_string(key_path: &'static str, value: &str, data: &str) -> Result<(), StartupError> {
        let (path, value, data) = (wide(key_path), wide(value), wide(data));
        let mut key: HKEY = std::ptr::null_mut();
        // SAFETY: `path` is NUL-terminated and outlives the call; `key`
        // receives the opened handle.
        let status = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                path.as_ptr(),
                0,
                std::ptr::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE,
                std::ptr::null(),
                &mut key,
                std::ptr::null_mut(),
            )
        };
        check(status, key_path)?;
        let bytes = u32::try_from(data.len() * 2).unwrap_or(u32::MAX);
        // SAFETY: `key` is open with KEY_SET_VALUE; `data` holds `bytes`
        // bytes including the terminating NUL, as REG_SZ requires.
        let status = unsafe {
            RegSetValueExW(key, value.as_ptr(), 0, REG_SZ, data.as_ptr().cast(), bytes)
        };
        // SAFETY: `key` was opened above and is closed exactly once.
        unsafe { RegCloseKey(key) };
        check(status, key_path)
    }

    fn delete_value(key_path: &'static str, value: &str) -> Result<(), StartupError> {
        let (path, value) = (wide(key_path), wide(value));
        // SAFETY: both strings are NUL-terminated and outlive the call.
        let status =
            unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, path.as_ptr(), value.as_ptr()) };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(());
        }
        check(status, key_path)
    }

    fn delete_tree(parent: &'static str, name: &str) -> Result<(), StartupError> {
        let path = wide(&format!("{parent}\\{name}"));
        // SAFETY: `path` is NUL-terminated and outlives the call; with a
        // subkey given, the subkey itself is deleted along with its contents.
        let status = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, path.as_ptr()) };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(());
        }
        check(status, parent)
    }
}

#[cfg(all(test, not(feature = "store")))]
mod tests {
    use super::*;

    #[test]
    fn the_run_value_is_the_unquoted_native_path() {
        assert_eq!(
            run_command_line(Path::new("C:\\Program Files\\Phramer\\phramer.exe")),
            "C:\\Program Files\\Phramer\\phramer.exe"
        );
        assert_eq!(
            run_command_line(Path::new("C:/Program Files/Phramer/phramer.exe")),
            "C:\\Program Files\\Phramer\\phramer.exe"
        );
    }

    #[test]
    fn the_app_paths_folder_uses_forward_slashes() {
        assert_eq!(
            app_paths_folder(Path::new("C:\\Program Files\\Phramer\\phramer.exe")),
            "C:/Program Files/Phramer"
        );
    }

    #[test]
    fn a_stored_choice_is_recognised() {
        for text in ["true", "false"] {
            let settings = Settings::parse(format!("[General]\nstartupLaunch={text}\n").as_bytes());
            assert!(is_startup_launch_stored(&settings), "{text}");
        }
        assert!(!is_startup_launch_stored(&Settings::parse(
            b"[General]\nshowHelp=false\n"
        )));
        assert!(!is_startup_launch_stored(&Settings::default()));
    }
}
