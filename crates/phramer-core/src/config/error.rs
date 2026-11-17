//! Errors raised by the settings module.

use std::io;
use std::path::PathBuf;

/// Failure to locate, read, write or update the settings file.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// The settings file exists but could not be read.
    #[error("cannot read settings file {path}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// The settings file could not be written.
    #[error("cannot write settings file {path}")]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// A folder the settings location depends on could not be resolved.
    #[error("cannot locate the {0} folder")]
    FolderUnavailable(&'static str),
    /// The running executable could not be located (portable layout).
    #[error("cannot locate the application executable")]
    Executable(#[source] io::Error),
    /// A setter was given a value the old application would reject on load.
    #[error("`{value}` is not a valid value for setting `{key}`")]
    InvalidValue { key: String, value: String },
    /// The shortcut name is not one the application registers.
    #[error("unknown shortcut name `{0}`")]
    UnknownShortcut(String),
    /// The key sequence may not be bound to any action.
    #[error("shortcut `{0}` is reserved")]
    ReservedShortcut(String),
    /// Another action already has this key sequence configured.
    #[error("shortcut `{shortcut}` is already used by `{action}`")]
    ShortcutConflict { shortcut: String, action: String },
}
