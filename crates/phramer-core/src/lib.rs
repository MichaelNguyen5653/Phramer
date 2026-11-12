//! Phramer core: capture, configuration and export logic shared by the
//! daemon, the overlay and the UI.

pub mod config;

/// Version of the running build, taken from the workspace manifest.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::VERSION;

    #[test]
    fn version_continues_the_series() {
        // The MSI refuses downgrades, so the major version must stay above 15.
        let major: u32 = VERSION.split('.').next().and_then(|s| s.parse().ok()).unwrap_or(0);
        assert!(major >= 16, "version {VERSION} would not upgrade a 15.x install");
    }
}
