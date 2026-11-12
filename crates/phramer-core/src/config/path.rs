//! Where the settings file lives, and the folders its defaults refer to.

use std::path::{Path, PathBuf};

use super::error::ConfigError;

/// `QCoreApplication::organizationName()` of the C++ builds.
const ORGANIZATION: &str = "phramer";
/// `QCoreApplication::applicationName()` of the C++ builds.
const APPLICATION: &str = "phramer";

/// How the application is deployed (the C++ `USE_PORTABLE_CONFIG` switch).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// Installed build: settings live in the user's roaming profile.
    Installed,
    /// Portable build: settings live beside the executable.
    Portable,
}

/// Path of the settings file for this user and layout.
///
/// Installed builds use what `QSettings(IniFormat, UserScope, "phramer",
/// "phramer")` resolves to on Windows, `%APPDATA%\phramer\phramer.ini`;
/// portable builds use `phramer.ini` in the executable's folder.
pub fn settings_path(layout: Layout) -> Result<PathBuf, ConfigError> {
    match layout {
        Layout::Installed => {
            let roaming = roaming_app_data()
                .ok_or(ConfigError::FolderUnavailable("roaming application data"))?;
            Ok(installed_path(&roaming))
        }
        Layout::Portable => {
            let exe = std::env::current_exe().map_err(ConfigError::Executable)?;
            Ok(portable_path(&exe))
        }
    }
}

fn installed_path(roaming_app_data: &Path) -> PathBuf {
    roaming_app_data
        .join(ORGANIZATION)
        .join(format!("{APPLICATION}.ini"))
}

fn portable_path(executable: &Path) -> PathBuf {
    executable.with_file_name(format!("{APPLICATION}.ini"))
}

/// `ExistingDir::fallback`: the first of the Pictures, home and temporary
/// folders that exists.
pub(super) fn default_save_dir() -> Option<PathBuf> {
    [pictures_dir(), home_dir(), Some(std::env::temp_dir())]
        .into_iter()
        .flatten()
        .find(|dir| dir.is_dir())
}

fn home_dir() -> Option<PathBuf> {
    let variable = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(variable)
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
}

#[cfg(windows)]
fn roaming_app_data() -> Option<PathBuf> {
    known_folder(&windows_sys::Win32::UI::Shell::FOLDERID_RoamingAppData)
}

#[cfg(windows)]
fn pictures_dir() -> Option<PathBuf> {
    known_folder(&windows_sys::Win32::UI::Shell::FOLDERID_Pictures)
}

/// Resolves a known folder the way Qt does, so redirected folders (OneDrive
/// backup, roaming profiles) are honoured.
#[cfg(windows)]
fn known_folder(id: &windows_sys::core::GUID) -> Option<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::SHGetKnownFolderPath;

    let mut buffer: windows_sys::core::PWSTR = std::ptr::null_mut();
    // SAFETY: `id` points to a valid GUID and `buffer` to a writable pointer;
    // a null token means the current user.
    let status = unsafe { SHGetKnownFolderPath(id, 0, std::ptr::null_mut(), &mut buffer) };
    let path = if status >= 0 && !buffer.is_null() {
        // SAFETY: on success the buffer holds a NUL-terminated UTF-16 string,
        // so every offset up to and including the terminator is readable.
        let wide = unsafe {
            let len = (0..).take_while(|&i| *buffer.add(i) != 0).count();
            std::slice::from_raw_parts(buffer, len)
        };
        Some(PathBuf::from(std::ffi::OsString::from_wide(wide)))
    } else {
        None
    };
    // SAFETY: the buffer was allocated by the shell (or is null, which
    // `CoTaskMemFree` accepts) and is not used after this point.
    unsafe { CoTaskMemFree(buffer.cast()) };
    path
}

// Only Windows ships; these keep the crate building and testable elsewhere.
#[cfg(not(windows))]
fn roaming_app_data() -> Option<PathBuf> {
    home_dir().map(|home| home.join(".config"))
}

#[cfg(not(windows))]
fn pictures_dir() -> Option<PathBuf> {
    home_dir().map(|home| home.join("Pictures"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_layout_uses_organization_and_application_names() {
        let path = installed_path(Path::new("C:/profile/AppData/Roaming"));
        assert_eq!(
            path,
            Path::new("C:/profile/AppData/Roaming/phramer/phramer.ini")
        );
    }

    #[test]
    fn portable_layout_sits_beside_the_executable() {
        let path = portable_path(Path::new("D:/tools/Phramer/phramer.exe"));
        assert_eq!(path, Path::new("D:/tools/Phramer/phramer.ini"));
    }

    #[test]
    fn both_layouts_resolve_on_this_machine() {
        let installed = settings_path(Layout::Installed).expect("roaming folder resolves");
        assert!(
            installed.ends_with("phramer/phramer.ini"),
            "{}",
            installed.display()
        );
        assert!(installed.is_absolute());

        let portable = settings_path(Layout::Portable).expect("executable path resolves");
        let exe = std::env::current_exe().expect("test executable has a path");
        assert_eq!(portable.parent(), exe.parent());
        assert_eq!(
            portable.file_name().and_then(|name| name.to_str()),
            Some("phramer.ini")
        );
    }

    #[test]
    fn a_default_save_folder_exists() {
        let dir = default_save_dir().expect("the temporary folder always exists");
        assert!(dir.is_dir());
    }
}
