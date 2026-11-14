//! The image editor, opened on [`EDITOR_OPEN_EVENT`] with the PNG the
//! capture left in the temporary folder. As in 15.0.0, one window holds the
//! session: an image opened while it is open joins its filmstrip
//! ([`IMAGE_ADDED_EVENT`]), and the tray's Open Editor opens it empty
//! ([`open_empty`]). The window's page (`app/editor.html`) draws with the
//! annotation engine compiled to WebAssembly; what it saves, copies or pins
//! comes back here and goes the way a capture's export goes
//! (`capture::run_export`, `capture::pin`).
//!
//! While a capture overlay is open, one hidden window is built ahead
//! ([`prewarm`]) with its page loaded and waiting for an image, so Open in
//! Editor only has to hand the image over; the capture ending any other way
//! destroys it ([`release_warm`]). None is built while an editor is open:
//! the image goes to that one. Otherwise no webview exists until an image
//! is opened, and it goes when its window closes, so an idle daemon holds
//! no browser process.

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, mpsc};
use std::time::{Duration, Instant};

use phramer_annotate::{ArrowHead, RedactStyle, ShapeStyle};
use phramer_capture::PhysicalRect;
use phramer_core::config::{ButtonType, Settings};
use phramer_overlay::{
    CapturedImage, ExportAction, ExportReport, SettingChange, ShapeKind, ToolSettings,
};
use serde::{Deserialize, Serialize};
use tauri::ipc::{InvokeBody, Request, Response};
use tauri::{
    AppHandle, Emitter, Listener, Manager, Runtime, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};

use crate::capture;
use crate::cli::{SaveTo, Tasks};
use crate::daemon::DaemonState;
use crate::handoff::{self, EDITOR_OPEN_EVENT};
use crate::{printscreen, settings, settings_ui};

/// Window labels start with this; `capabilities/editor.json` matches it.
const LABEL_PREFIX: &str = "editor-";
/// Tells a window's page that another image was given to it; the payload
/// is its index for `editor_image`.
const IMAGE_ADDED_EVENT: &str = "editor:image-added";
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

/// How long a pre-warmed window's page has, once given an image, to show
/// it is alive (by asking for the image) before a new window replaces it.
const PAGE_TIMEOUT: Duration = Duration::from_secs(5);

/// One editor window.
struct Opened {
    /// Its place in the order windows were built: the newest takes images.
    number: u64,
    /// The images given to it, in the order they came; empty while
    /// pre-warmed or opened empty.
    images: Vec<PathBuf>,
    /// Opened with no image on purpose (the tray's Open Editor).
    empty: bool,
    /// When the last image was asked for (or the pre-warm started).
    requested: Instant,
    /// The page's requests for a first image that has not come yet.
    waiting: Vec<mpsc::Sender<PathBuf>>,
    /// The page has loaded and asked for its image.
    asked: bool,
    /// Save All under way.
    batch: Option<Batch>,
}

/// Save All under way: the folder made for it, the name every file shares
/// (15.0.0's one pattern for the whole set) and how it is going.
#[derive(Debug, Default)]
struct Batch {
    folder: PathBuf,
    name: String,
    saved: u32,
    failed: u32,
    last: Option<PathBuf>,
}

/// The editor windows, by label.
#[derive(Default)]
pub(crate) struct Editors {
    windows: Mutex<HashMap<String, Opened>>,
    next: AtomicU64,
    /// The pre-warmed window that has no image yet.
    warm: Mutex<Option<String>>,
}

impl Editors {
    fn take_warm(&self) -> Option<String> {
        self.warm.lock().ok()?.take()
    }

    /// The newest window that holds images or was opened empty: where
    /// another image goes. Not the pre-warmed one, and not one being
    /// replaced (`rescue`), whose images were taken.
    fn session(&self) -> Option<String> {
        let windows = self.windows.lock().ok()?;
        windows
            .iter()
            .filter(|(_, opened)| !opened.images.is_empty() || opened.empty)
            .max_by_key(|(_, opened)| opened.number)
            .map(|(label, _)| label.clone())
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

/// Where a window for an image goes: the work area of the monitor under the
/// cursor, and the logical inner size the image wants there.
struct Fit {
    origin: (i32, i32),
    work: (u32, u32),
    size: (f64, f64),
}

fn fit_for<R: Runtime>(app: &AppHandle<R>, image: (u32, u32)) -> Fit {
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
    Fit {
        origin,
        work,
        size: initial_size(image, work, scale),
    }
}

/// Centres `window` in the work area it was sized for (the one under the
/// cursor); the builder's `center` uses whichever monitor the window starts
/// on.
fn center<R: Runtime>(window: &WebviewWindow<R>, fit: &Fit) {
    if let Ok(outer) = window.outer_size() {
        let at = centered_in(fit.origin, fit.work, (outer.width, outer.height));
        if let Err(error) = window.set_position(tauri::PhysicalPosition::new(at.0, at.1)) {
            log::warn!("cannot place the editor: {error}");
        }
    }
}

/// What a new window is built for.
enum Purpose {
    Image(PathBuf),
    /// Pre-warmed, waiting for an image.
    Warm,
    /// The tray's Open Editor: no image until one is opened.
    Empty,
}

/// A new hidden editor window of logical inner `size`. Blocks the main
/// thread while WebView2 starts.
fn build<R: Runtime>(
    app: &AppHandle<R>,
    purpose: Purpose,
    requested: Instant,
    size: (f64, f64),
) -> Option<WebviewWindow<R>> {
    let editors = app.state::<Editors>();
    let number = editors.next.fetch_add(1, Ordering::Relaxed) + 1;
    let label = format!("{LABEL_PREFIX}{number}");
    if let Ok(mut windows) = editors.windows.lock() {
        windows.insert(
            label.clone(),
            Opened {
                number,
                empty: matches!(purpose, Purpose::Empty),
                images: match purpose {
                    Purpose::Image(path) => vec![path],
                    Purpose::Warm | Purpose::Empty => Vec::new(),
                },
                requested,
                waiting: Vec::new(),
                asked: false,
                batch: None,
            },
        );
    }
    let held = session_end::hold();
    let built = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("editor.html".into()))
        .title(TITLE)
        .inner_size(size.0, size.1)
        .min_inner_size(MIN_WIDTH, MIN_HEIGHT)
        .decorations(false)
        .visible(false)
        .build();
    drop(held);
    let window = match built {
        Ok(window) => window,
        Err(error) => {
            log::error!("cannot create the editor window: {error}");
            forget(app, &label);
            return None;
        }
    };
    let handle = app.clone();
    window.on_window_event(move |event| {
        if matches!(event, WindowEvent::Destroyed) {
            forget(&handle, &label);
        }
    });
    Some(window)
}

/// An editor window for the PNG at `path`: the open one, where it joins
/// the filmstrip; else the pre-warmed one when there is one, otherwise a
/// new one. Hidden until its page has drawn the image (`editor_ready`), so
/// it never flashes empty.
pub(crate) fn open<R: Runtime>(app: &AppHandle<R>, path: PathBuf) {
    if let Some(window) = session_window(app) {
        add_image(app, &window, path);
        return;
    }
    if let Some(label) = app.state::<Editors>().take_warm() {
        match app.get_webview_window(&label) {
            Some(window) => {
                hand_over(app, &window, path);
                return;
            }
            None => forget(app, &label),
        }
    }
    let requested = Instant::now();
    let image = png_size(&path).unwrap_or((800, 600));
    let fit = fit_for(app, image);
    let Some(window) = build(app, Purpose::Image(path.clone()), requested, fit.size) else {
        return;
    };
    center(&window, &fit);
    log::info!(
        "editor: {} for {} ({}x{}), window {}x{} created in {} ms",
        window.label(),
        path.display(),
        image.0,
        image.1,
        fit.size.0,
        fit.size.1,
        requested.elapsed().as_millis()
    );
}

/// The window another image goes to, if one is open.
fn session_window<R: Runtime>(app: &AppHandle<R>) -> Option<WebviewWindow<R>> {
    let label = app.state::<Editors>().session()?;
    app.get_webview_window(&label)
}

/// Another image for the open window: its page is told, fetches it, adds
/// it to the filmstrip and says when it is drawn (`editor_ready`).
fn add_image<R: Runtime>(app: &AppHandle<R>, window: &WebviewWindow<R>, path: PathBuf) {
    let label = window.label().to_owned();
    let index = app
        .state::<Editors>()
        .windows
        .lock()
        .ok()
        .and_then(|mut windows| {
            let opened = windows.get_mut(&label)?;
            opened.images.push(path.clone());
            opened.requested = Instant::now();
            Some(opened.images.len() - 1)
        });
    let Some(index) = index else {
        return;
    };
    log::info!(
        "editor: {label} gets image {} ({})",
        index + 1,
        path.display()
    );
    if let Err(error) = app.emit_to(label.as_str(), IMAGE_ADDED_EVENT, index) {
        log::error!("cannot tell {label} about its image: {error}");
    }
}

/// The tray's Open Editor (15.0.0's `openEmpty`): the open editor comes
/// forward; with none, a window opens that says how to capture into it.
pub(crate) fn open_empty<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = session_window(app) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    let requested = Instant::now();
    let fit = fit_for(app, (0, 0));
    let Some(window) = build(app, Purpose::Empty, requested, fit.size) else {
        return;
    };
    center(&window, &fit);
    log::info!(
        "editor: {} opened empty, window created in {} ms",
        window.label(),
        requested.elapsed().as_millis()
    );
}

/// On the main thread, while a capture overlay is open: builds the hidden
/// window the capture's Open in Editor will use, unless one is waiting
/// already or an editor is open to take the image.
pub(crate) fn prewarm<R: Runtime>(app: &AppHandle<R>) {
    if let Some(label) = app.state::<Editors>().session() {
        log::info!("editor: {label} is open; nothing to pre-warm");
        return;
    }
    let editors = app.state::<Editors>();
    if let Ok(warm) = editors.warm.lock()
        && let Some(label) = warm.as_ref()
    {
        log::info!("editor: {label} is pre-warmed already");
        return;
    }
    let started = Instant::now();
    let Some(window) = build(app, Purpose::Warm, started, (MIN_WIDTH, MIN_HEIGHT)) else {
        return;
    };
    let label = window.label().to_owned();
    if let Ok(mut warm) = editors.warm.lock() {
        *warm = Some(label.clone());
    }
    log::info!(
        "editor: {label} pre-warmed, window created in {} ms",
        started.elapsed().as_millis()
    );
}

/// On the main thread, once no capture is open: the pre-warmed window that
/// got no image goes, and with it the webview.
pub(crate) fn release_warm<R: Runtime>(app: &AppHandle<R>) {
    let Some(label) = app.state::<Editors>().take_warm() else {
        return;
    };
    log::info!("editor: {label} released unused");
    match app.get_webview_window(&label) {
        Some(window) => {
            if let Err(error) = window.destroy() {
                log::error!("cannot close the pre-warmed editor: {error}");
                forget(app, &label);
            }
        }
        None => forget(app, &label),
    }
}

/// Gives the pre-warmed `window` its image: sized and placed for it, then
/// the page, which has asked or will ask for it, draws it and shows.
fn hand_over<R: Runtime>(app: &AppHandle<R>, window: &WebviewWindow<R>, path: PathBuf) {
    let label = window.label().to_owned();
    let image = png_size(&path).unwrap_or((800, 600));
    let fit = fit_for(app, image);
    // On the target monitor first, so the logical size uses its scale
    if let Err(error) =
        window.set_position(tauri::PhysicalPosition::new(fit.origin.0, fit.origin.1))
    {
        log::warn!("cannot place the editor: {error}");
    }
    if let Err(error) = window.set_size(tauri::LogicalSize::new(fit.size.0, fit.size.1)) {
        log::warn!("cannot size the editor: {error}");
    }
    center(window, &fit);
    let waiting = app
        .state::<Editors>()
        .windows
        .lock()
        .ok()
        .and_then(|mut windows| {
            let opened = windows.get_mut(&label)?;
            opened.images.push(path.clone());
            opened.requested = Instant::now();
            Some(std::mem::take(&mut opened.waiting))
        })
        .unwrap_or_default();
    let asked = !waiting.is_empty();
    for page in waiting {
        let _ = page.send(path.clone());
    }
    log::info!(
        "editor: {label} (pre-warmed) for {} ({}x{}), window {}x{}; page {}",
        path.display(),
        image.0,
        image.1,
        fit.size.0,
        fit.size.1,
        if asked { "waiting" } else { "still loading" }
    );
    // A page that never loads would keep the image from showing: a new
    // window takes over
    let handle = app.clone();
    let watched = std::thread::Builder::new()
        .name("editor-watch".to_owned())
        .spawn(move || {
            std::thread::sleep(PAGE_TIMEOUT);
            let target = handle.clone();
            if let Err(error) = handle.run_on_main_thread(move || rescue(&target, &label)) {
                log::error!("cannot check the editor: {error}");
            }
        });
    if let Err(error) = watched {
        log::warn!("cannot watch the pre-warmed editor: {error}");
    }
}

/// On the main thread: the pre-warmed window `label` still has not asked
/// for its image; it goes and its images open in a new window.
fn rescue<R: Runtime>(app: &AppHandle<R>, label: &str) {
    let images = app
        .state::<Editors>()
        .windows
        .lock()
        .ok()
        .and_then(|mut windows| {
            let opened = windows.get_mut(label)?;
            // Kept from `forget`, which would remove the files
            (!opened.asked).then(|| std::mem::take(&mut opened.images))
        })
        .unwrap_or_default();
    if images.is_empty() {
        return;
    }
    log::warn!("editor: {label} did not load its page; opening a new window");
    match app.get_webview_window(label) {
        Some(window) => {
            if let Err(error) = window.destroy() {
                log::error!("cannot close the editor: {error}");
            }
        }
        None => forget(app, label),
    }
    for path in images {
        open(app, path);
    }
}

/// The window is gone: drops its entry and the temporary PNG it was given.
fn forget<R: Runtime>(app: &AppHandle<R>, label: &str) {
    let editors = app.state::<Editors>();
    if let Ok(mut warm) = editors.warm.lock()
        && warm.as_deref() == Some(label)
    {
        *warm = None;
    }
    let removed = editors
        .windows
        .lock()
        .ok()
        .and_then(|mut windows| windows.remove(label));
    let Some(opened) = removed else {
        return;
    };
    log::info!("editor: {label} closed");
    let temporary = std::env::temp_dir().join("phramer");
    for path in opened.images {
        // Only the editor's own copies in %TEMP%\phramer
        let ours = path.parent() == Some(temporary.as_path())
            && path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("editor-"));
        if ours && let Err(error) = std::fs::remove_file(&path) {
            log::warn!("cannot remove {}: {error}", path.display());
        }
    }
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

/// Image `index` (default 0) of those given to the window: width and
/// height (u32, little-endian), then straight RGBA8. Only images given to
/// this window are read. A pre-warmed window's page waits here for its
/// first; an index not given yet, or any index of a window opened empty,
/// answers with no bytes.
#[tauri::command]
pub(crate) async fn editor_image<R: Runtime>(
    window: WebviewWindow<R>,
    editors: tauri::State<'_, Editors>,
    index: Option<usize>,
) -> Result<Response, String> {
    let index = index.unwrap_or(0);
    let given = {
        let mut windows = editors
            .windows
            .lock()
            .map_err(|_| "the editor state is unavailable".to_owned())?;
        let opened = windows
            .get_mut(window.label())
            .ok_or_else(|| "this window has no image".to_owned())?;
        opened.asked = true;
        match opened.images.get(index) {
            Some(path) => Ok(Some(path.clone())),
            None if index == 0 && !opened.empty => {
                let (page, image) = mpsc::channel();
                opened.waiting.push(page);
                Err(image)
            }
            None => Ok(None),
        }
    };
    let path = match given {
        Ok(Some(path)) => path,
        Ok(None) => return Ok(Response::new(Vec::new())),
        // Ends when the image comes or the window goes
        Err(image) => tauri::async_runtime::spawn_blocking(move || image.recv())
            .await
            .map_err(|error| error.to_string())?
            .map_err(|_| "the window closed before it got an image".to_owned())?,
    };
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
    /// From the image arriving to the image on screen: what an already
    /// loaded page takes.
    image_ms: f64,
}

/// The page has drawn the image (or, opened empty, its hint): the window
/// shows and takes the focus. Again for each image that joins it.
#[tauri::command]
pub(crate) fn editor_ready<R: Runtime>(
    window: WebviewWindow<R>,
    editors: tauri::State<'_, Editors>,
    timing: Option<PageTiming>,
) {
    let (requested, given) = editors
        .windows
        .lock()
        .ok()
        .and_then(|windows| {
            windows.get(window.label()).map(|opened| {
                (
                    Some(opened.requested),
                    !opened.images.is_empty() || opened.empty,
                )
            })
        })
        .unwrap_or((None, true));
    if !given {
        // The page failed before any image came (a pre-warmed window whose
        // engine did not load): never shown; the next open builds anew
        log::warn!("editor: {} failed while pre-warmed", window.label());
        if let Err(error) = window.destroy() {
            log::error!("cannot close the editor: {error}");
        }
        return;
    }
    // An image that joined a minimised window brings it back
    if window.is_minimized().unwrap_or(false)
        && let Err(error) = window.unminimize()
    {
        log::warn!("cannot restore the editor: {error}");
    }
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

/// The `[Shortcuts]` entries the editor reacts to and shows on its toolbar.
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
    /// `fontFamily`, the text tool's family; empty for the bundled font.
    font_family: String,
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
        font_family: settings.font_family(),
    }
}

#[tauri::command]
pub(crate) fn editor_settings(state: tauri::State<'_, DaemonState>) -> EditorSettings {
    editor_settings_of(&settings::load(&state.settings_path))
}

/// The settings file changed: open editors read their toolbar keys again.
pub(crate) fn settings_changed<R: Runtime>(app: &AppHandle<R>) {
    let labels: Vec<String> = app
        .state::<Editors>()
        .windows
        .lock()
        .map(|windows| windows.keys().cloned().collect())
        .unwrap_or_default();
    for label in labels {
        if let Err(error) = app.emit_to(label.as_str(), settings_ui::CHANGED_EVENT, ()) {
            log::warn!("editor: cannot tell {label} about the settings: {error}");
        }
    }
}

/// What the empty editor tells about capturing into it (15.0.0's
/// `updateEmptyState`).
#[derive(Debug, Serialize)]
pub(crate) struct EmptyState {
    capture_hotkey: String,
    /// Print Screen reaches Phramer, not Windows' snipping tool.
    print_screen: bool,
    auto_open_in_editor: bool,
    editor_key: String,
}

fn empty_state_of(settings: &Settings, print_screen: bool) -> EmptyState {
    EmptyState {
        capture_hotkey: settings.shortcut("TAKE_SCREENSHOT").unwrap_or_default(),
        print_screen,
        auto_open_in_editor: settings.auto_open_in_editor(),
        editor_key: settings.shortcut("TYPE_OPEN_IN_EDITOR").unwrap_or_default(),
    }
}

#[tauri::command]
pub(crate) fn editor_empty_state(state: tauri::State<'_, DaemonState>) -> EmptyState {
    empty_state_of(
        &settings::load(&state.settings_path),
        !printscreen::snipping_claims_key(),
    )
}

/// The installed font families the text tool offers.
#[tauri::command]
pub(crate) async fn editor_font_families() -> Vec<String> {
    tauri::async_runtime::spawn_blocking(phramer_overlay::system_font_families)
        .await
        .unwrap_or_default()
}

/// The faces of an installed font family as one body: a count, then for
/// each face bold, italic, collection index and length (u32, little-endian)
/// and the font file. No faces when the family is not installed.
fn faces_body(faces: &[phramer_overlay::SystemFace]) -> Vec<u8> {
    let count = u32::try_from(faces.len()).unwrap_or(u32::MAX);
    let mut body = count.to_le_bytes().to_vec();
    for face in faces {
        let length = u32::try_from(face.data.len()).unwrap_or(u32::MAX);
        for field in [
            u32::from(face.bold),
            u32::from(face.italic),
            face.index,
            length,
        ] {
            body.extend_from_slice(&field.to_le_bytes());
        }
        body.extend_from_slice(&face.data);
    }
    body
}

/// Every face of `family` installed on the system, for text set in it.
/// Reads the font files, so off the main thread.
#[tauri::command]
pub(crate) async fn editor_font_faces(family: String) -> Result<Response, String> {
    let faces =
        tauri::async_runtime::spawn_blocking(move || phramer_overlay::system_font_faces(&family))
            .await
            .map_err(|error| error.to_string())?;
    Ok(Response::new(faces_body(&faces)))
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
    FontFamily { family: String },
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
        EditorChange::FontFamily { family } => {
            settings.set_font_family(&family);
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
        "save_all" => {
            // One file of the set; the set is reported once, at the end
            // (`editor_save_all_end`)
            let report = save_one(
                &app,
                &window,
                &settings,
                &image,
                geometry,
                number("x-number"),
            )?;
            return Ok(report.notices.iter().map(Notice::from).collect());
        }
        other => return Err(format!("unknown action {other}")),
    };
    let notices = report.notices.iter().map(Notice::from).collect();
    capture::deliver_report(&app, report);
    Ok(notices)
}

impl From<&phramer_overlay::Notice> for Notice {
    fn from(notice: &phramer_overlay::Notice) -> Self {
        Self {
            text: notice.text.clone(),
            error: notice.error,
        }
    }
}

/// Save All: the system's folder dialog, starting in the save folder; a
/// folder of its own is made inside the one picked (15.0.0: a session is a
/// set). `None` when cancelled.
#[tauri::command]
pub(crate) async fn editor_save_all_begin<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let settings = settings::load(&app.state::<DaemonState>().settings_path);
    let mut dialog = app
        .dialog()
        .file()
        .set_parent(&window)
        .set_title("Save all images to folder");
    if let Some(folder) = settings.save_path() {
        dialog = dialog.set_directory(folder);
    }
    let Some(parent) = dialog
        .blocking_pick_folder()
        .and_then(|picked| picked.into_path().ok())
    else {
        return Ok(None);
    };
    let time = phramer_overlay::LocalTime::now();
    let folder = set_folder(&parent, &time).map_err(|error| {
        log::error!("Save All: {}: {error}", parent.display());
        format!("Could not create a folder inside {}.", parent.display())
    })?;
    let batch = Batch {
        folder: folder.clone(),
        name: phramer_overlay::expand_pattern(&settings.filename_pattern(), &time),
        ..Batch::default()
    };
    let editors = app.state::<Editors>();
    let mut windows = editors
        .windows
        .lock()
        .map_err(|_| "the editor state is unavailable".to_owned())?;
    let opened = windows
        .get_mut(window.label())
        .ok_or_else(|| "this window is not an editor".to_owned())?;
    opened.batch = Some(batch);
    Ok(Some(folder.display().to_string()))
}

/// `createSessionFolder`: `Phramer <date time>` inside `parent`, with
/// `_1`, `_2`, ... when one of that name exists, never reusing one.
fn set_folder(parent: &Path, time: &dyn phramer_overlay::TimeFormat) -> std::io::Result<PathBuf> {
    let name = phramer_overlay::expand_pattern("Phramer %Y-%m-%d %H-%M-%S", time);
    let mut attempt = 0;
    loop {
        let candidate = if attempt == 0 {
            parent.join(&name)
        } else {
            parent.join(format!("{name}_{attempt}"))
        };
        match std::fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists && attempt < 1000 => {
                attempt += 1;
            }
            Err(error) => return Err(error),
        }
    }
}

/// One image of Save All, as `<number> - <name>` in the set's folder.
fn save_one<R: Runtime>(
    app: &AppHandle<R>,
    window: &WebviewWindow<R>,
    settings: &Settings,
    image: &CapturedImage,
    geometry: PhysicalRect,
    number: u32,
) -> Result<ExportReport, String> {
    let editors = app.state::<Editors>();
    let target = {
        let windows = editors
            .windows
            .lock()
            .map_err(|_| "the editor state is unavailable".to_owned())?;
        let batch = windows
            .get(window.label())
            .and_then(|opened| opened.batch.as_ref())
            .ok_or_else(|| "Save All has not started".to_owned())?;
        batch.folder.join(format!("{number} - {}", batch.name))
    };
    let tasks = Tasks {
        save: Some(SaveTo::Path(target)),
        ..Tasks::default()
    };
    let report = export(settings, image, geometry, ExportAction::Accept, tasks);
    if let Ok(mut windows) = editors.windows.lock()
        && let Some(batch) = windows
            .get_mut(window.label())
            .and_then(|opened| opened.batch.as_mut())
    {
        match &report.saved {
            Some(saved) => {
                batch.saved += 1;
                batch.last = Some(saved.clone());
            }
            None => batch.failed += 1,
        }
    }
    Ok(report)
}

/// What became of Save All (15.0.0's texts): every file, or how many failed.
fn save_all_report(batch: &Batch) -> ExportReport {
    let total = batch.saved + batch.failed;
    let folder = batch.folder.display();
    let notice = if batch.failed > 0 {
        phramer_overlay::Notice {
            text: format!(
                "{} of {total} images could not be saved to {folder}.",
                batch.failed
            ),
            path: None,
            error: true,
        }
    } else {
        phramer_overlay::Notice {
            text: format!("{total} images saved to {folder}"),
            path: Some(batch.folder.clone()),
            error: false,
        }
    };
    ExportReport {
        saved: batch.last.clone(),
        notices: vec![notice],
        ..ExportReport::default()
    }
}

/// Save All is over: one notification for the set, and what to show.
#[tauri::command]
pub(crate) fn editor_save_all_end<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
) -> Vec<Notice> {
    let batch = app
        .state::<Editors>()
        .windows
        .lock()
        .ok()
        .and_then(|mut windows| windows.get_mut(window.label())?.batch.take());
    let Some(batch) = batch else {
        return Vec::new();
    };
    let report = save_all_report(&batch);
    let notices = report.notices.iter().map(Notice::from).collect();
    capture::deliver_report(&app, report);
    notices
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

/// While WebView2 starts, it pumps the main thread's messages from inside
/// Tao's event handler. Tao answers `WM_ENDSESSION` (sign-out, Restart
/// Manager) by calling that handler again, which aborts the process; with a
/// window built ahead on every capture, that would hit a sign-out during a
/// capture. The message is held back for the length of a build and sent
/// again once it is over, when Tao ends the process cleanly.
mod session_end {
    use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};

    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallWindowProcW, EnumThreadWindows, GWLP_WNDPROC, GetClassNameW, PostMessageW,
        SetWindowLongPtrW, WM_ENDSESSION, WNDPROC,
    };

    /// Tao's window for the event loop's own messages.
    const TAO_TARGET: &str = "Tao Thread Event Target";

    /// The window procedure replaced while held; 0 when not held.
    static ORIGINAL: AtomicIsize = AtomicIsize::new(0);
    static ENDED: AtomicBool = AtomicBool::new(false);
    static ENDED_LPARAM: AtomicIsize = AtomicIsize::new(0);

    /// Holds `WM_ENDSESSION` back until dropped.
    pub(super) struct Held {
        window: HWND,
    }

    /// On the main thread: holds the session's end back, unless it is held
    /// already or Tao's window is not found.
    pub(super) fn hold() -> Option<Held> {
        if ORIGINAL.load(Ordering::SeqCst) != 0 {
            return None;
        }
        let window = tao_target()?;
        ENDED.store(false, Ordering::SeqCst);
        let procedure = held as *const () as isize;
        // SAFETY: `window` belongs to this thread; `held` has the window
        // procedure's signature and passes on all it does not keep.
        let original = unsafe { SetWindowLongPtrW(window, GWLP_WNDPROC, procedure) };
        if original == 0 {
            return None;
        }
        ORIGINAL.store(original, Ordering::SeqCst);
        Some(Held { window })
    }

    impl Drop for Held {
        fn drop(&mut self) {
            let original = ORIGINAL.swap(0, Ordering::SeqCst);
            // SAFETY: puts back the procedure `hold` replaced, on the same
            // window of this thread.
            unsafe { SetWindowLongPtrW(self.window, GWLP_WNDPROC, original) };
            if ENDED.swap(false, Ordering::SeqCst) {
                log::info!("session ending: passed on after the editor window was built");
                // SAFETY: posts to a window of this thread, no pointers.
                unsafe {
                    PostMessageW(
                        self.window,
                        WM_ENDSESSION,
                        1,
                        ENDED_LPARAM.load(Ordering::SeqCst),
                    )
                };
            }
        }
    }

    unsafe extern "system" fn held(
        window: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if message == WM_ENDSESSION && wparam != 0 {
            ENDED_LPARAM.store(lparam, Ordering::SeqCst);
            ENDED.store(true, Ordering::SeqCst);
            return 0;
        }
        // SAFETY: `ORIGINAL` is the procedure this one replaced (a function
        // pointer, so the same size as `isize`), set before this one runs.
        unsafe {
            let original = std::mem::transmute::<isize, WNDPROC>(ORIGINAL.load(Ordering::SeqCst));
            CallWindowProcW(original, window, message, wparam, lparam)
        }
    }

    /// Tao's event-target window on this thread.
    fn tao_target() -> Option<HWND> {
        unsafe extern "system" fn found(window: HWND, out: LPARAM) -> i32 {
            let mut class = [0u16; 64];
            // SAFETY: `class` holds 64 characters.
            let length = unsafe { GetClassNameW(window, class.as_mut_ptr(), 64) };
            let name = String::from_utf16_lossy(&class[..usize::try_from(length).unwrap_or(0)]);
            if name == TAO_TARGET {
                // SAFETY: `out` is the `Option<HWND>` `tao_target` passed.
                unsafe { *(out as *mut Option<HWND>) = Some(window) };
                return 0;
            }
            1
        }
        let mut window: Option<HWND> = None;
        // SAFETY: `found` writes only through the pointer to `window`,
        // which outlives the call.
        unsafe {
            EnumThreadWindows(
                GetCurrentThreadId(),
                Some(found),
                &mut window as *mut Option<HWND> as LPARAM,
            )
        };
        window
    }
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
        apply_change(
            &mut current,
            EditorChange::FontFamily {
                family: "Cambria".into(),
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
        assert_eq!(read.font_family, "Cambria");
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
            "fontFamily",
        ] {
            assert!(text.contains(key), "{key} missing from {text}");
        }
    }

    fn opened(number: u64, images: usize, empty: bool) -> Opened {
        Opened {
            number,
            images: (0..images)
                .map(|n| PathBuf::from(format!("{n}.png")))
                .collect(),
            empty,
            requested: Instant::now(),
            waiting: Vec::new(),
            asked: true,
            batch: None,
        }
    }

    #[test]
    fn another_image_goes_to_the_newest_window_holding_images() {
        let editors = Editors::default();
        assert_eq!(editors.session(), None);
        {
            let mut windows = editors.windows.lock().unwrap();
            // Pre-warmed: no image yet, so no session
            windows.insert("editor-1".into(), opened(1, 0, false));
        }
        assert_eq!(editors.session(), None);
        {
            let mut windows = editors.windows.lock().unwrap();
            windows.insert("editor-2".into(), opened(2, 1, false));
            windows.insert("editor-10".into(), opened(10, 2, false));
        }
        // By build order, not by label
        assert_eq!(editors.session().as_deref(), Some("editor-10"));
        {
            let mut windows = editors.windows.lock().unwrap();
            // Its images taken (`rescue`): no longer a session
            windows.get_mut("editor-10").unwrap().images.clear();
            // Opened empty from the tray: takes the next image
            windows.insert("editor-3".into(), opened(3, 0, true));
        }
        assert_eq!(editors.session().as_deref(), Some("editor-3"));
    }

    #[test]
    fn save_all_makes_a_folder_of_its_own_and_never_reuses_one() {
        let parent =
            std::env::temp_dir().join(format!("phramer-editor-save-all-{}", std::process::id()));
        std::fs::create_dir_all(&parent).unwrap();
        let time = phramer_overlay::LocalTime::at(2026, 11, 13, 9, 5, 7);
        let first = set_folder(&parent, &time).unwrap();
        let second = set_folder(&parent, &time).unwrap();
        assert_eq!(
            first.file_name().unwrap().to_string_lossy(),
            "Phramer 2026-11-13 09-05-07"
        );
        assert_eq!(
            second.file_name().unwrap().to_string_lossy(),
            "Phramer 2026-11-13 09-05-07_1"
        );
        assert!(first.is_dir() && second.is_dir());
        let _ = std::fs::remove_dir_all(&parent);
    }

    #[test]
    fn save_all_reports_the_set_once() {
        let folder = PathBuf::from(r"C:\Pictures\Phramer 2026-11-13 09-05-07");
        let done = save_all_report(&Batch {
            folder: folder.clone(),
            saved: 3,
            last: Some(folder.join("3 - x.png")),
            ..Batch::default()
        });
        assert_eq!(
            done.notices[0].text,
            r"3 images saved to C:\Pictures\Phramer 2026-11-13 09-05-07"
        );
        assert_eq!(done.notices[0].path.as_deref(), Some(folder.as_path()));
        assert!(!done.notices[0].error);
        assert_eq!(done.saved, Some(folder.join("3 - x.png")));
        let failed = save_all_report(&Batch {
            folder,
            saved: 2,
            failed: 1,
            ..Batch::default()
        });
        assert_eq!(
            failed.notices[0].text,
            r"1 of 3 images could not be saved to C:\Pictures\Phramer 2026-11-13 09-05-07."
        );
        assert!(failed.notices[0].error);
    }

    #[test]
    fn font_faces_go_to_the_page_with_their_style_and_index() {
        let body = faces_body(&[
            phramer_overlay::SystemFace {
                bold: false,
                italic: true,
                data: vec![7, 8],
                index: 3,
            },
            phramer_overlay::SystemFace {
                bold: true,
                italic: false,
                data: vec![9],
                index: 0,
            },
        ]);
        let mut expected = 2u32.to_le_bytes().to_vec();
        for field in [0u32, 1, 3, 2] {
            expected.extend_from_slice(&field.to_le_bytes());
        }
        expected.extend_from_slice(&[7, 8]);
        for field in [1u32, 0, 0, 1] {
            expected.extend_from_slice(&field.to_le_bytes());
        }
        expected.push(9);
        assert_eq!(body, expected);
        assert_eq!(faces_body(&[]), 0u32.to_le_bytes());
    }

    #[test]
    fn the_empty_editor_names_the_capture_keys() {
        let mut current = Settings::default();
        current.set_auto_open_in_editor(true);
        let state = empty_state_of(&current, true);
        assert!(state.print_screen && state.auto_open_in_editor);
        assert!(!state.capture_hotkey.is_empty());
        assert!(state.editor_key.eq_ignore_ascii_case("e"));
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
