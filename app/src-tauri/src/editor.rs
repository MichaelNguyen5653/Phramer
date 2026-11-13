//! The image editor: one window per image, opened on [`EDITOR_OPEN_EVENT`]
//! with the PNG the capture left in the temporary folder. The window's page
//! (`app/editor.html`) draws with the annotation engine compiled to
//! WebAssembly; what it saves, copies or pins comes back here and goes the
//! way a capture's export goes (`capture::run_export`, `capture::pin`).
//!
//! No webview exists until an image is opened, and each one goes when its
//! window closes, so an idle daemon holds no browser process.

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use phramer_annotate::{ArrowHead, RedactStyle, ShapeStyle};
use phramer_capture::PhysicalRect;
use phramer_core::config::{ButtonType, Settings};
use phramer_overlay::{
    CapturedImage, ExportAction, ExportReport, SettingChange, ShapeKind, ToolSettings,
};
use serde::{Deserialize, Serialize};
use tauri::ipc::{InvokeBody, Request, Response};
use tauri::{
    AppHandle, Listener, Manager, Runtime, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    WindowEvent,
};

use crate::capture;
use crate::cli::{SaveTo, Tasks};
use crate::daemon::DaemonState;
use crate::handoff::{self, EDITOR_OPEN_EVENT};
use crate::settings;

/// Window labels start with this; `capabilities/editor.json` matches it.
const LABEL_PREFIX: &str = "editor-";
pub(crate) const TITLE: &str = "Phramer Editor";
/// Room around the image, the options panel and the bars, in logical
/// pixels (15.0.0's `resizeToFit`: 48 around the image).
const MARGIN: f64 = 48.0;
const PANEL_WIDTH: f64 = 240.0;
const BARS_HEIGHT: f64 = 36.0 + 44.0 + 28.0;
const MIN_WIDTH: f64 = 720.0;
const MIN_HEIGHT: f64 = 520.0;
/// The share of the monitor's work area the window may take.
const MAX_SHARE: f64 = 0.92;

/// One open editor window.
struct Opened {
    path: PathBuf,
    requested: Instant,
}

/// The open editor windows, by label.
#[derive(Default)]
pub(crate) struct Editors {
    windows: Mutex<HashMap<String, Opened>>,
    next: AtomicU64,
}

impl Editors {
    fn path_of(&self, label: &str) -> Option<PathBuf> {
        let windows = self.windows.lock().ok()?;
        windows.get(label).map(|opened| opened.path.clone())
    }
}

/// Opens an editor for every `editor:open` the daemon sends itself.
pub(crate) fn listen<R: Runtime>(app: &AppHandle<R>) {
    let handle = app.clone();
    app.listen(EDITOR_OPEN_EVENT, move |event| {
        let path = match serde_json::from_str::<String>(event.payload()) {
            Ok(path) => PathBuf::from(path),
            Err(error) => {
                log::error!("{EDITOR_OPEN_EVENT}: unreadable payload: {error}");
                return;
            }
        };
        let target = handle.clone();
        if let Err(error) = handle.run_on_main_thread(move || open(&target, path)) {
            log::error!("cannot open the editor: {error}");
        }
    });
}

/// The window's logical inner size for an image of `image` physical
/// pixels on a work area of `work` physical pixels at `scale`.
fn initial_size(image: (u32, u32), work: (u32, u32), scale: f64) -> (f64, f64) {
    let scale = if scale > 0.0 { scale } else { 1.0 };
    let fit = |image: u32, chrome: f64, work: u32, min: f64| {
        let wanted = f64::from(image) / scale + chrome;
        let most = (f64::from(work) / scale * MAX_SHARE).max(min);
        wanted.clamp(min, most).round()
    };
    (
        fit(image.0, 2.0 * MARGIN + PANEL_WIDTH, work.0, MIN_WIDTH),
        fit(image.1, 2.0 * MARGIN + BARS_HEIGHT, work.1, MIN_HEIGHT),
    )
}

/// Where a window of physical size `outer` sits centred in the work area at
/// `origin` of size `work`, never past its top-left corner.
fn centered_in(origin: (i32, i32), work: (u32, u32), outer: (u32, u32)) -> (i32, i32) {
    let offset = |work: u32, outer: u32| i32::try_from(work.saturating_sub(outer) / 2).unwrap_or(0);
    (
        origin.0.saturating_add(offset(work.0, outer.0)),
        origin.1.saturating_add(offset(work.1, outer.1)),
    )
}

/// Width and height from a PNG's IHDR.
fn png_size(path: &Path) -> Option<(u32, u32)> {
    use std::io::Read;
    let mut head = [0u8; 24];
    File::open(path).ok()?.read_exact(&mut head).ok()?;
    if &head[..8] != b"\x89PNG\r\n\x1a\n" || &head[12..16] != b"IHDR" {
        return None;
    }
    let field = |at: usize| {
        head.get(at..at + 4)
            .and_then(|bytes| bytes.try_into().ok())
            .map(u32::from_be_bytes)
    };
    Some((field(16)?, field(20)?))
}

/// A new editor window for the PNG at `path`. Hidden until its page has
/// drawn the image (`editor_ready`), so it never flashes empty.
pub(crate) fn open<R: Runtime>(app: &AppHandle<R>, path: PathBuf) {
    let requested = Instant::now();
    let editors = app.state::<Editors>();
    let number = editors.next.fetch_add(1, Ordering::Relaxed) + 1;
    let label = format!("{LABEL_PREFIX}{number}");

    let image = png_size(&path).unwrap_or((800, 600));
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|at| app.monitor_from_point(at.x, at.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    let (origin, work, scale) = monitor.map_or(((0, 0), (1920, 1080), 1.0), |monitor| {
        let area = monitor.work_area();
        (
            (area.position.x, area.position.y),
            (area.size.width, area.size.height),
            monitor.scale_factor(),
        )
    });
    let (width, height) = initial_size(image, work, scale);

    if let Ok(mut windows) = editors.windows.lock() {
        windows.insert(
            label.clone(),
            Opened {
                path: path.clone(),
                requested,
            },
        );
    }
    let built = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("editor.html".into()))
        .title(TITLE)
        .inner_size(width, height)
        .min_inner_size(MIN_WIDTH, MIN_HEIGHT)
        .decorations(false)
        .visible(false)
        .build();
    let window = match built {
        Ok(window) => window,
        Err(error) => {
            log::error!("cannot create the editor window: {error}");
            forget(app, &label);
            return;
        }
    };
    // Centred on the monitor it was sized for (the one under the cursor);
    // the builder's `center` uses whichever monitor the window starts on
    if let Ok(outer) = window.outer_size() {
        let at = centered_in(origin, work, (outer.width, outer.height));
        if let Err(error) = window.set_position(tauri::PhysicalPosition::new(at.0, at.1)) {
            log::warn!("cannot place the editor: {error}");
        }
    }
    let handle = app.clone();
    let closing = label.clone();
    window.on_window_event(move |event| {
        if matches!(event, WindowEvent::Destroyed) {
            forget(&handle, &closing);
        }
    });
    log::info!(
        "editor: {label} for {} ({}x{}), window {width}x{height} created in {} ms",
        path.display(),
        image.0,
        image.1,
        requested.elapsed().as_millis()
    );
}

/// The window is gone: drops its entry and the temporary PNG it was given.
fn forget<R: Runtime>(app: &AppHandle<R>, label: &str) {
    let removed = app
        .state::<Editors>()
        .windows
        .lock()
        .ok()
        .and_then(|mut windows| windows.remove(label));
    let Some(opened) = removed else {
        return;
    };
    // Only the editor's own copies in %TEMP%\phramer
    let ours = opened.path.parent() == Some(std::env::temp_dir().join("phramer").as_path())
        && opened
            .path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with("editor-"));
    if ours && let Err(error) = std::fs::remove_file(&opened.path) {
        log::warn!("cannot remove {}: {error}", opened.path.display());
    }
    log::info!("editor: {label} closed");
}

#[derive(Debug, thiserror::Error)]
enum ImageError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Png(#[from] png::DecodingError),
    #[error("unsupported PNG layout {0:?}")]
    Layout(png::ColorType),
    #[error("the image is too large")]
    TooLarge,
}

/// Straight RGBA8 pixels of a PNG.
fn decode_png(path: &Path) -> Result<(u32, u32, Vec<u8>), ImageError> {
    let mut decoder = png::Decoder::new(BufReader::new(File::open(path)?));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info()?;
    let mut buffer = vec![0; reader.output_buffer_size().ok_or(ImageError::TooLarge)?];
    let info = reader.next_frame(&mut buffer)?;
    buffer.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => buffer,
        png::ColorType::Rgb => buffer
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::Grayscale => buffer.iter().flat_map(|&v| [v, v, v, 255]).collect(),
        png::ColorType::GrayscaleAlpha => buffer
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        other => return Err(ImageError::Layout(other)),
    };
    Ok((info.width, info.height, rgba))
}

/// The window's image: width and height (u32, little-endian), then
/// straight RGBA8. Only the image this window was opened with is read.
#[tauri::command]
pub(crate) async fn editor_image<R: Runtime>(
    window: WebviewWindow<R>,
    editors: tauri::State<'_, Editors>,
) -> Result<Response, String> {
    let path = editors
        .path_of(window.label())
        .ok_or_else(|| "this window has no image".to_owned())?;
    let (width, height, rgba) = decode_png(&path).map_err(|error| error.to_string())?;
    let mut bytes = Vec::with_capacity(8 + rgba.len());
    bytes.extend_from_slice(&width.to_le_bytes());
    bytes.extend_from_slice(&height.to_le_bytes());
    bytes.extend_from_slice(&rgba);
    Ok(Response::new(bytes))
}

/// How long the page took, as it measured itself.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct PageTiming {
    /// From the page starting to load to the image on screen.
    page_ms: f64,
    /// From asking for the image to the image on screen: what an already
    /// loaded page would take.
    image_ms: f64,
}

/// The page has drawn the image: the window shows and takes the focus.
#[tauri::command]
pub(crate) fn editor_ready<R: Runtime>(
    window: WebviewWindow<R>,
    editors: tauri::State<'_, Editors>,
    timing: Option<PageTiming>,
) {
    let requested = editors
        .windows
        .lock()
        .ok()
        .and_then(|windows| windows.get(window.label()).map(|opened| opened.requested));
    if let Err(error) = window.show() {
        log::error!("cannot show the editor: {error}");
    }
    let _ = window.set_focus();
    if let Some(requested) = requested {
        let timing = timing.unwrap_or_default();
        log::info!(
            "editor: {} visible {} ms after open (page {:.0} ms, image {:.0} ms)",
            window.label(),
            requested.elapsed().as_millis(),
            timing.page_ms,
            timing.image_ms
        );
    }
}

/// The tools a toolbar button stands for, by the editor's names.
const TOOLS: [(&str, ButtonType); 13] = [
    ("pencil", ButtonType::Pencil),
    ("line", ButtonType::Line),
    ("arrow", ButtonType::Arrow),
    ("shape", ButtonType::Shape),
    ("selection", ButtonType::Selection),
    ("rectangle", ButtonType::Rectangle),
    ("circle", ButtonType::Circle),
    ("marker", ButtonType::Marker),
    ("text", ButtonType::Text),
    ("circle_count", ButtonType::CircleCount),
    ("pixelate", ButtonType::Pixelate),
    ("invert", ButtonType::Invert),
    ("move_object", ButtonType::MoveObject),
];

/// The `[Shortcuts]` entries the editor reacts to.
const SHORTCUTS: [&str; 26] = [
    "TYPE_PENCIL",
    "TYPE_DRAWER",
    "TYPE_ARROW",
    "TYPE_SHAPE",
    "TYPE_SELECTION",
    "TYPE_RECTANGLE",
    "TYPE_CIRCLE",
    "TYPE_MARKER",
    "TYPE_TEXT",
    "TYPE_CIRCLECOUNT",
    "TYPE_PIXELATE",
    "TYPE_INVERT",
    "TYPE_MOVE_OBJECT",
    "TYPE_UNDO",
    "TYPE_REDO",
    "TYPE_COPY",
    "TYPE_COPY_FILE",
    "TYPE_SAVE",
    "TYPE_EXIT",
    "TYPE_OPEN_APP",
    "TYPE_OCR",
    "TYPE_PIN",
    "TYPE_SIZEINCREASE",
    "TYPE_SIZEDECREASE",
    "TYPE_DELETE_CURRENT_TOOL",
    "TYPE_COMMIT_CURRENT_TOOL",
];

type Rgb = [u8; 3];

/// The tool settings the editor starts with: the overlay's, read from the
/// same keys.
#[derive(Debug, Serialize)]
pub(crate) struct EditorSettings {
    color: Rgb,
    /// `userColors`; `None` is the custom-colour entry.
    palette: Vec<Option<Rgb>>,
    sizes: HashMap<&'static str, u32>,
    thickness: u32,
    arrow_head: ArrowHead,
    reverse_arrow: bool,
    shape_kind: &'static str,
    shape_style: ShapeStyle,
    rectangle_style: ShapeStyle,
    highlight_outline: u32,
    redact: RedactStyle,
    undo_limit: usize,
    shortcuts: HashMap<&'static str, String>,
}

fn editor_settings_of(settings: &Settings) -> EditorSettings {
    let tools = ToolSettings::from_settings(settings);
    let rgb = |color: phramer_annotate::Color| [color.r, color.g, color.b];
    EditorSettings {
        color: rgb(tools.color),
        palette: tools.palette.iter().map(|entry| entry.map(rgb)).collect(),
        sizes: TOOLS
            .iter()
            .map(|(name, button)| (*name, tools.size_of(Some(*button))))
            .collect(),
        thickness: tools.size_of(None),
        arrow_head: tools.arrow_head,
        reverse_arrow: tools.reverse_arrow,
        shape_kind: match tools.shape_kind {
            ShapeKind::Square => "square",
            ShapeKind::Circle => "circle",
        },
        shape_style: tools.shape_style,
        rectangle_style: tools.rectangle_style,
        highlight_outline: tools.highlight_outline,
        redact: tools.redact,
        undo_limit: tools.undo_limit,
        shortcuts: SHORTCUTS
            .iter()
            .map(|name| (*name, settings.shortcut(name).unwrap_or_default()))
            .collect(),
    }
}

#[tauri::command]
pub(crate) fn editor_settings(state: tauri::State<'_, DaemonState>) -> EditorSettings {
    editor_settings_of(&settings::load(&state.settings_path))
}

/// A choice made in the editor, saved to the existing key the overlay
/// uses for it. The editor has no keys of its own.
#[derive(Debug, Deserialize)]
#[serde(tag = "key", rename_all = "snake_case")]
pub(crate) enum EditorChange {
    DrawColor { color: Rgb },
    ToolSize { tool: String, size: u32 },
    Shape { kind: String, style: ShapeStyle },
    Arrow { head: ArrowHead, reversed: bool },
    RectangleStyle { style: ShapeStyle },
    HighlightOutline { width: u32 },
    Redact { style: RedactStyle },
}

fn apply_change(settings: &mut Settings, change: EditorChange) -> Result<(), String> {
    let error = |error: phramer_core::config::ConfigError| error.to_string();
    let overlay =
        |change: SettingChange, settings: &mut Settings| change.apply(settings).map_err(error);
    match change {
        EditorChange::DrawColor { color: [r, g, b] } => overlay(
            SettingChange::DrawColor(phramer_annotate::Color::rgb(r, g, b)),
            settings,
        ),
        EditorChange::ToolSize { tool, size } => {
            let button = TOOLS
                .iter()
                .find(|(name, _)| *name == tool)
                .map(|(_, button)| *button)
                .ok_or_else(|| format!("unknown tool {tool}"))?;
            overlay(SettingChange::ToolSize { button, size }, settings)
        }
        EditorChange::Shape { kind, style } => {
            let kind = match kind.as_str() {
                "circle" => ShapeKind::Circle,
                _ => ShapeKind::Square,
            };
            overlay(SettingChange::Shape { kind, style }, settings)
        }
        EditorChange::Arrow { head, reversed } => {
            settings.set_reverse_arrow(reversed);
            settings
                .set_arrow_style(i32::from(head == ArrowHead::Curved))
                .map_err(error)
        }
        EditorChange::RectangleStyle { style } => settings
            .set_rectangle_fill_mode(i32::from(style == ShapeStyle::Highlight))
            .map_err(error),
        EditorChange::HighlightOutline { width } => settings
            .set_highlight_outline_width(i32::try_from(width).unwrap_or(i32::MAX))
            .map_err(error),
        EditorChange::Redact { style } => {
            settings.set_insecure_pixelate(style == RedactStyle::Insecure);
            Ok(())
        }
    }
}

/// Saves one change to the settings file at once.
#[tauri::command]
pub(crate) fn editor_set(
    change: EditorChange,
    state: tauri::State<'_, DaemonState>,
) -> Result<(), String> {
    let mut current = Settings::load(&state.settings_path).map_err(|error| error.to_string())?;
    apply_change(&mut current, change)?;
    current
        .save(&state.settings_path)
        .map_err(|error| error.to_string())
}

/// What became of an export, for the editor to show.
#[derive(Debug, Serialize)]
pub(crate) struct Notice {
    text: String,
    error: bool,
}

fn header<'a>(request: &'a Request<'_>, name: &str) -> Option<&'a str> {
    request.headers().get(name)?.to_str().ok()
}

/// Straight RGBA8 from the page as the export's BGRA capture.
fn captured(width: u32, height: u32, rgba: &[u8]) -> Option<CapturedImage> {
    let expected = (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(4)?;
    if rgba.len() != expected || expected == 0 {
        return None;
    }
    let bgra = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| [p[2], p[1], p[0], p[3]])
        .collect();
    Some(CapturedImage {
        width,
        height,
        bgra,
    })
}

/// The editor's image, rendered by the page with its annotations, goes
/// where the action says. The body is straight RGBA8; headers
/// `x-action`, `x-width` and `x-height` describe it.
#[tauri::command]
pub(crate) async fn editor_export<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    request: Request<'_>,
) -> Result<Vec<Notice>, String> {
    let action = header(&request, "x-action").unwrap_or_default().to_owned();
    let number = |name: &str| {
        header(&request, name)
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(0)
    };
    let (width, height) = (number("x-width"), number("x-height"));
    let InvokeBody::Raw(rgba) = request.body() else {
        return Err("the image must be sent as raw bytes".to_owned());
    };
    let image = captured(width, height, rgba)
        .ok_or_else(|| "the image does not fit its size".to_owned())?;
    let settings_path = app.state::<DaemonState>().settings_path.clone();
    let settings = settings::load(&settings_path);
    // Where the image is on the desktop: over the editor window
    let origin = window
        .outer_position()
        .map(|at| (at.x, at.y))
        .unwrap_or((0, 0));
    let geometry = PhysicalRect::new(
        origin.0,
        origin.1,
        i32::try_from(width).unwrap_or(i32::MAX),
        i32::try_from(height).unwrap_or(i32::MAX),
    );
    let report = match action.as_str() {
        "copy" => export(
            &settings,
            &image,
            geometry,
            ExportAction::Copy,
            Tasks::default(),
        ),
        "copy_file" => export(
            &settings,
            &image,
            geometry,
            ExportAction::CopyFile,
            Tasks::default(),
        ),
        "save" => export(
            &settings,
            &image,
            geometry,
            ExportAction::Save,
            Tasks::default(),
        ),
        "save_as" => {
            let Some(path) = ask_save_path(&app, &window, &settings) else {
                return Ok(Vec::new());
            };
            let tasks = Tasks {
                save: Some(SaveTo::Path(path)),
                ..Tasks::default()
            };
            export(&settings, &image, geometry, ExportAction::Accept, tasks)
        }
        "pin" => {
            capture::pin(&app, &settings, image, geometry);
            ExportReport::default()
        }
        "ocr" => handoff::read_text(&settings, &image),
        "open_app" => handoff::open_with(&settings, &image),
        other => return Err(format!("unknown action {other}")),
    };
    let notices = report
        .notices
        .iter()
        .map(|notice| Notice {
            text: notice.text.clone(),
            error: notice.error,
        })
        .collect();
    capture::deliver_report(&app, report);
    Ok(notices)
}

fn export(
    settings: &Settings,
    image: &CapturedImage,
    geometry: PhysicalRect,
    action: ExportAction,
    tasks: Tasks,
) -> ExportReport {
    match capture::run_export(settings, image, geometry, action, &tasks) {
        capture::Delivery::Exported(report) => report,
        _ => ExportReport::default(),
    }
}

/// Save As: the system's save dialog, in the save folder, with the file
/// name the filename pattern gives. `None` when it is cancelled.
fn ask_save_path<R: Runtime>(
    app: &AppHandle<R>,
    window: &WebviewWindow<R>,
    settings: &Settings,
) -> Option<PathBuf> {
    use tauri_plugin_dialog::DialogExt;
    let name = phramer_overlay::expand_pattern(
        &settings.filename_pattern(),
        &phramer_overlay::LocalTime::now(),
    );
    let extension = settings.save_as_file_extension();
    let extension = extension.trim_start_matches('.');
    let extension = if extension.is_empty() {
        "png"
    } else {
        extension
    };
    let mut dialog = app
        .dialog()
        .file()
        .set_parent(window)
        .set_title("Save As")
        .set_file_name(format!("{name}.{extension}"))
        .add_filter("PNG image", &["png"])
        .add_filter("JPEG image", &["jpg", "jpeg"])
        .add_filter("BMP image", &["bmp"]);
    if let Some(folder) = settings.save_path() {
        dialog = dialog.set_directory(folder);
    }
    dialog.blocking_save_file()?.into_path().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_is_centred_in_the_work_area() {
        assert_eq!(
            centered_in((-1920, 0), (1920, 1040), (1000, 600)),
            (-1460, 220)
        );
        // Larger than the area: its top-left corner
        assert_eq!(centered_in((0, 0), (800, 600), (1000, 700)), (0, 0));
    }

    #[test]
    fn the_window_fits_the_image_within_the_work_area() {
        // A small image: the minimum size
        assert_eq!(initial_size((100, 80), (1920, 1040), 1.0), (720.0, 520.0));
        // A medium one: image + margins + panel, image + margins + bars
        assert_eq!(initial_size((800, 500), (1920, 1040), 1.0), (1136.0, 704.0));
        // A large one is held to 92% of the work area
        assert_eq!(
            initial_size((3000, 2000), (1920, 1040), 1.0),
            (1766.0, 957.0)
        );
        // At 200% the image counts in logical pixels
        assert_eq!(
            initial_size((1600, 1000), (3840, 2080), 2.0),
            (1136.0, 704.0)
        );
    }

    #[test]
    fn exports_take_straight_rgba_as_bgra() {
        let image = captured(1, 1, &[1, 2, 3, 4]).unwrap();
        assert_eq!(image.bgra, [3, 2, 1, 4]);
        assert!(captured(2, 1, &[0; 4]).is_none());
        assert!(captured(0, 0, &[]).is_none());
    }

    #[test]
    fn settings_round_trip_through_existing_keys() {
        let mut current = Settings::default();
        apply_change(&mut current, EditorChange::DrawColor { color: [1, 2, 3] }).unwrap();
        apply_change(
            &mut current,
            EditorChange::ToolSize {
                tool: "text".into(),
                size: 14,
            },
        )
        .unwrap();
        apply_change(
            &mut current,
            EditorChange::Arrow {
                head: ArrowHead::Curved,
                reversed: true,
            },
        )
        .unwrap();
        apply_change(
            &mut current,
            EditorChange::Redact {
                style: RedactStyle::Insecure,
            },
        )
        .unwrap();
        assert!(
            apply_change(
                &mut current,
                EditorChange::ToolSize {
                    tool: "nonsense".into(),
                    size: 1
                }
            )
            .is_err()
        );
        let read = editor_settings_of(&current);
        assert_eq!(read.color, [1, 2, 3]);
        assert_eq!(read.sizes.get("text"), Some(&14));
        assert_eq!(read.arrow_head, ArrowHead::Curved);
        assert!(read.reverse_arrow);
        assert_eq!(read.redact, RedactStyle::Insecure);
        assert!(
            read.shortcuts
                .get("TYPE_PENCIL")
                .is_some_and(|key| key.eq_ignore_ascii_case("p"))
        );
        let text = String::from_utf8_lossy(&current.to_bytes()).into_owned();
        for key in [
            "drawColor",
            "drawFontSize",
            "arrowStyle",
            "reverseArrow",
            "insecurePixelate",
        ] {
            assert!(text.contains(key), "{key} missing from {text}");
        }
    }

    #[test]
    fn png_headers_give_the_size() {
        let folder =
            std::env::temp_dir().join(format!("phramer-editor-test-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("a.png");
        let image = CapturedImage {
            width: 3,
            height: 2,
            bgra: [10, 20, 30, 255].repeat(6),
        };
        std::fs::write(
            &path,
            phramer_overlay::encode(&image, phramer_overlay::ImageFormat::Png, 0).unwrap(),
        )
        .unwrap();
        assert_eq!(png_size(&path), Some((3, 2)));
        let (width, height, rgba) = decode_png(&path).unwrap();
        assert_eq!((width, height), (3, 2));
        assert_eq!(&rgba[..4], &[30, 20, 10, 255]);
        let _ = std::fs::remove_dir_all(&folder);
    }
}
