//! What happens to a capture once the overlay has closed (or for `full` and
//! `screen`, once it is grabbed): `Flameshot::exportCapture` and the savers
//! of `screenshotsaver.cpp`.
//!
//! The export runs once per capture, after every window is gone. It never
//! fails as a whole: each task reports how it went as a [`Notice`], which the
//! daemon turns into notifications.

mod encode;
mod filename;

use std::path::{Path, PathBuf};

use phramer_capture::PhysicalRect;
use phramer_core::config::Settings;

use crate::error::OverlayError;
use crate::state::ExportAction;
pub use encode::{CapturedImage, ImageFormat, dib, dib_v5, encode};
#[cfg(windows)]
pub use filename::LocalTime;
pub use filename::{TimeFormat, expand_pattern, screenshot_path};

/// Where a save task writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveTarget {
    /// The configured folder (`saveToFilesystemGUI`).
    Default,
    /// An existing folder or a new file (`-p`).
    Path(PathBuf),
}

/// What the request asked for (`CaptureRequest::ExportTask`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CaptureTasks {
    pub copy: bool,
    pub save: Option<SaveTarget>,
    pub print_geometry: bool,
    pub print_raw: bool,
    pub pin: bool,
}

/// Where captures go besides files: the clipboard.
pub trait Handoff {
    /// The image as a bitmap (`CF_DIB`, `CF_DIBV5`) plus PNG, or plus JPEG
    /// at this quality when `jpeg` is set (`useJpgForClipboard`).
    fn copy_image(&mut self, image: &CapturedImage, jpeg: Option<u8>) -> Result<(), OverlayError>;
    fn copy_text(&mut self, text: &str) -> Result<(), OverlayError>;
    /// The file itself, as Explorer copies one (`CF_HDROP`).
    fn copy_file(&mut self, path: &Path) -> Result<(), OverlayError>;
}

/// One line of feedback; `path` makes the notification open its folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub text: String,
    pub path: Option<PathBuf>,
    pub error: bool,
}

/// How the export went.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExportReport {
    /// The last file written, for Show in Folder.
    pub saved: Option<PathBuf>,
    pub notices: Vec<Notice>,
    /// What 15.0.0 wrote to its standard output: `--print-geometry`'s line,
    /// then `--raw`'s PNG. The caller relays it to whoever asked.
    pub stdout: Vec<u8>,
}

impl ExportReport {
    fn info(&mut self, text: String, path: Option<PathBuf>) {
        log::info!("{text}");
        self.notices.push(Notice {
            text,
            path,
            error: false,
        });
    }

    fn error(&mut self, text: String) {
        log::error!("{text}");
        self.notices.push(Notice {
            text,
            path: None,
            error: true,
        });
    }
}

/// Everything an export needs besides the pixels.
pub struct ExportContext<'a> {
    pub settings: &'a Settings,
    pub time: &'a dyn TimeFormat,
    pub handoff: &'a mut dyn Handoff,
}

/// Carries out `action` on `image`. `geometry` is where the image was on
/// the desktop, for `--print-geometry`. `Accept` runs the request's tasks;
/// the other actions are the overlay's own keys.
pub fn export(
    image: &CapturedImage,
    geometry: PhysicalRect,
    action: ExportAction,
    tasks: &CaptureTasks,
    context: &mut ExportContext<'_>,
) -> ExportReport {
    let mut report = ExportReport::default();
    match action {
        ExportAction::Copy => copy_image(image, context, &mut report),
        ExportAction::Save => save_to_default(image, context, &mut report),
        ExportAction::CopyFile => copy_as_file(image, context, &mut report),
        ExportAction::Accept => run_tasks(image, geometry, tasks, context, &mut report),
    }
    report
}

fn run_tasks(
    image: &CapturedImage,
    geometry: PhysicalRect,
    tasks: &CaptureTasks,
    context: &mut ExportContext<'_>,
    report: &mut ExportReport,
) {
    if tasks.print_geometry {
        let line = format!(
            "{}x{}+{}+{}
",
            geometry.width, geometry.height, geometry.x, geometry.y
        );
        log::info!("{}", line.trim_end());
        report.stdout.extend_from_slice(line.as_bytes());
    }
    if tasks.print_raw {
        match encode(image, ImageFormat::Png, 0) {
            Ok(png) => report.stdout.extend_from_slice(&png),
            Err(error) => report.error(format!("--raw: {error}")),
        }
    }
    match &tasks.save {
        Some(SaveTarget::Default) => save_to_default(image, context, report),
        Some(SaveTarget::Path(path)) => {
            let _ = save_to(image, path, "", context, report);
        }
        None => {}
    }
    if tasks.copy {
        copy_image(image, context, report);
    }
    // `pin` is the caller's: a pin outlives the export
}

/// `saveToFilesystem`: writes the image to `path` (a folder or a file
/// name), never replacing a file. Returns the file written.
fn save_to(
    image: &CapturedImage,
    path: &Path,
    prefix: &str,
    context: &mut ExportContext<'_>,
    report: &mut ExportReport,
) -> Option<PathBuf> {
    let settings = context.settings;
    let name = expand_pattern(&settings.filename_pattern(), context.time);
    let mut target = screenshot_path(path, &settings.save_as_file_extension(), &name);
    let extension = target
        .extension()
        .map(|extension| extension.to_string_lossy().into_owned())
        .unwrap_or_default();
    let format = match ImageFormat::from_extension(&extension) {
        Some(format) => format,
        None => {
            // 15.0.0 had every Qt image writer; this build writes PNG, JPEG
            // and BMP, so other formats are saved as PNG
            log::warn!("cannot write .{extension} files; saving as PNG");
            target = screenshot_path(&target, "png", &name);
            ImageFormat::Png
        }
    };
    let quality = u8::try_from(settings.jpeg_quality()).unwrap_or(75);
    let lead = if prefix.is_empty() {
        String::new()
    } else {
        format!("{prefix} ")
    };
    let written = encode(image, format, quality).and_then(|bytes| {
        std::fs::write(&target, bytes).map_err(|source| OverlayError::Write {
            path: target.clone(),
            source,
        })
    });
    match written {
        Ok(()) => {
            report.info(
                format!("{lead}Capture saved as {}", target.display()),
                Some(target.clone()),
            );
            report.saved = Some(target.clone());
            Some(target)
        }
        Err(error) => {
            report.error(format!(
                "{lead}Error trying to save as {}: {error}",
                target.display()
            ));
            None
        }
    }
}

/// `saveToFilesystemGUI`: the configured folder; with `copyPathAfterSave`
/// the path goes on the clipboard too.
fn save_to_default(
    image: &CapturedImage,
    context: &mut ExportContext<'_>,
    report: &mut ExportReport,
) {
    let Some(folder) = context.settings.save_path() else {
        report.error(OverlayError::NoSaveFolder.to_string());
        return;
    };
    if !context.settings.save_path_fixed() {
        log::info!(
            "save dialog: not implemented yet; saving to {}",
            folder.display()
        );
    }
    let Some(saved) = save_to(image, &folder, "", context, report) else {
        return;
    };
    if context.settings.copy_path_after_save() {
        let text = saved.display().to_string().replace('/', "\\");
        match context.handoff.copy_text(&text) {
            Ok(()) => report.info(format!("Path copied to clipboard as {text}"), None),
            Err(error) => report.error(format!("Error while saving to clipboard: {error}")),
        }
    }
}

/// `saveToClipboard`: with `saveAfterCopy` the image is also saved.
fn copy_image(image: &CapturedImage, context: &mut ExportContext<'_>, report: &mut ExportReport) {
    const COPIED: &str = "Capture saved to clipboard.";
    match context.settings.save_path() {
        Some(folder) if context.settings.save_after_copy() => {
            let _ = save_to(image, &folder, COPIED, context, report);
        }
        _ => report.info(COPIED.to_owned(), None),
    }
    let jpeg = context
        .settings
        .use_jpg_for_clipboard()
        .then(|| u8::try_from(context.settings.jpeg_quality()).unwrap_or(75));
    if let Err(error) = context.handoff.copy_image(image, jpeg) {
        report.error(format!("Error while saving to clipboard: {error}"));
    }
}

/// Copy as file: saved to the configured folder without asking, then the
/// file is put on the clipboard.
fn copy_as_file(image: &CapturedImage, context: &mut ExportContext<'_>, report: &mut ExportReport) {
    let Some(folder) = context.settings.save_path() else {
        report.error(OverlayError::NoSaveFolder.to_string());
        return;
    };
    if let Some(saved) = save_to(image, &folder, "Copied as a file.", context, report)
        && let Err(error) = context.handoff.copy_file(&saved)
    {
        report.error(format!("Error while saving to clipboard: {error}"));
    }
}
