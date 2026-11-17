//! User settings, stored in the same `QSettings` INI file as the C++ builds
//! (14.1.x and 15.0.0) so an upgrade keeps them.
//!
//! [`settings_path`] locates the file, [`Settings`] reads and writes it.

mod buttons;
mod check;
mod color;
mod error;
mod escape;
mod ini;
mod keyseq;
mod path;
mod settings;
mod variant;

pub use buttons::ButtonType;
pub use color::Color;
pub use error::ConfigError;
pub use path::{Layout, settings_path};
pub use settings::{CaptureRegionMode, Settings};
