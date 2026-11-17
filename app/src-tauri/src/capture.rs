//! Captures: the overlay (`gui`, the tray, the hotkey), one monitor
//! (`screen`) and the whole desktop (`full`). Each runs on a thread of its
//! own, so the tray and the hotkeys stay responsive; what an export leaves
//! behind (the last saved file, notifications) is handed back to the main
//! thread.
//!
//! As in 15.0.0 the overlay exports exactly once, after its windows are
//! gone: [`phramer_overlay::run_overlay`] returns the one outcome, and this
//! module is its only caller. Its other outcomes (OCR, Open in Editor, App
//! Launcher) are carried out here too.
//!
//! A capture asked for by `phramer-cli` carries a [`Reply`]: what 15.0.0
//! printed to the console goes back through it, then the exit code.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use phramer_capture::{
    CaptureError, CaptureTarget, LocalRect, Monitor, PhysicalPoint, PhysicalRect, RegionMode,
    choose_target, cursor_position, enumerate_monitors, grab_monitor, grab_virtual_desktop,
    monitor_at_or_primary,
};
use phramer_core::config::{CaptureRegionMode, Settings};
use phramer_overlay::{
    BUNDLED_FONT, CaptureTasks, CapturedImage, DeferredAction, ExportAction, ExportContext,
    ExportReport, Handoff, ImageFormat, LocalTime, OverlayConfig, OverlayError, OverlaySession,
    SaveTarget, SystemClipboard, Theme, export, run_overlay,
};
use tauri::{AppHandle, Manager, Runtime};

use crate::cli::{InitialSelection, Rect, SaveTo, Tasks};
use crate::daemon::DaemonState;
use crate::reply::{EXIT_OK, Reply};
use crate::{actions, editor, handoff, notify, settings, tray};

/// Set while an overlay is open: like 15.0.0's single `CaptureWidget`, a
/// second one is refused.
static OVERLAY_OPEN: AtomicBool = AtomicBool::new(false);
/// The last file saved this session, for Show in Folder. In memory only.
static LAST_SAVED: Mutex<Option<PathBuf>> = Mutex::new(None);
/// The last region exported from the overlay (`--last-region`,
/// `saveLastRegion`), physical, relative to its monitor.
static LAST_REGION: Mutex<Option<LocalRect>> = Mutex::new(None);

#[derive(Debug, thiserror::Error)]
enum CaptureFailure {
    #[error(transparent)]
    Capture(#[from] CaptureError),
    #[error(transparent)]
    Overlay(#[from] OverlayError),
    #[error("Requested screen exceeds screen count")]
    NoSuchScreen,
}

/// How a capture ended, for the main thread.
pub(crate) enum Delivery {
    Exported(ExportReport),
    /// Cancelled, or refused (`captureFailed`).
    Aborted,
    /// Open in Editor: the capture, saved for the editor.
    Editor(PathBuf),
}

/// Which monitor the overlay opens on.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Placement {
    /// `captureRegionMode` decides.
    Settings,
    /// A monitor in capture-number order (primary first).
    Monitor(u32),
    UnderCursor,
}

/// An interactive capture.
#[derive(Debug, Clone)]
pub(crate) struct OverlayPlan {
    pub(crate) delay_ms: u32,
    pub(crate) placement: Placement,
    pub(crate) initial_selection: InitialSelection,
    pub(crate) tasks: Tasks,
}

/// The region the last overlay exported, for the Capture Launcher.
pub(crate) fn last_region() -> Option<LocalRect> {
    LAST_REGION.lock().ok().and_then(|region| *region)
}

pub(crate) fn last_saved() -> Option<PathBuf> {
    LAST_SAVED.lock().ok().and_then(|saved| saved.clone())
}

/// Opens the overlay.
pub(crate) fn overlay<R: Runtime>(app: &AppHandle<R>, plan: OverlayPlan, reply: Option<Reply>) {
    let requested_at = Instant::now();
    if OVERLAY_OPEN.swap(true, Ordering::SeqCst) {
        log::warn!("a capture is already open");
        drop(reply);
        deliver(app, Delivery::Aborted);
        return;
    }
    let delay = plan.delay_ms;
    // Colours and tool sizes picked on the overlay are saved here
    let settings_path = app.state::<DaemonState>().settings_path.clone();
    let handle = app.clone();
    let started = spawn(
        app,
        "capture-overlay",
        delay,
        reply,
        move |settings, requested| {
            // Reopened for the next capture however this one ends
            let _open = OverlayOpen;
            run_gui(
                &handle,
                settings,
                plan,
                requested.unwrap_or(requested_at),
                settings_path,
            )
        },
    );
    if !started {
        OVERLAY_OPEN.store(false, Ordering::SeqCst);
    }
}

/// Clears [`OVERLAY_OPEN`] when dropped.
struct OverlayOpen;

impl Drop for OverlayOpen {
    fn drop(&mut self) {
        OVERLAY_OPEN.store(false, Ordering::SeqCst);
    }
}

/// 15.0.0's `screen` without `--edit`: one monitor, straight to export.
pub(crate) fn screen<R: Runtime>(
    app: &AppHandle<R>,
    delay_ms: u32,
    monitor: Option<u32>,
    region: Option<Rect>,
    tasks: Tasks,
    reply: Option<Reply>,
) {
    spawn(
        app,
        "capture-screen",
        delay_ms,
        reply,
        move |settings, _| {
            let monitors = enumerate_monitors()?;
            let index = match monitor {
                Some(number) => number as usize,
                None => under_cursor(&monitors),
            };
            let target = monitors.get(index).ok_or(CaptureFailure::NoSuchScreen)?;
            let frame = grab_monitor(target)?;
            let whole = LocalRect::new(0, 0, target.bounds().width, target.bounds().height);
            let region = match region {
                Some(rect) => LocalRect::new(rect.x, rect.y, rect.width, rect.height)
                    .intersection(&whole)
                    .unwrap_or(whole),
                None => whole,
            };
            let image = crop(frame.bgra(), frame.width(), region);
            let geometry = target.rect_to_physical(region);
            Ok(run_export(
                &settings,
                &image,
                geometry,
                ExportAction::Accept,
                &tasks,
            ))
        },
    );
}

/// 15.0.0's `full`: every monitor in one image the size of the virtual
/// desktop.
pub(crate) fn full<R: Runtime>(
    app: &AppHandle<R>,
    delay_ms: u32,
    tasks: Tasks,
    reply: Option<Reply>,
) {
    spawn(app, "capture-full", delay_ms, reply, move |settings, _| {
        let monitors = enumerate_monitors()?;
        let frame = grab_virtual_desktop(&monitors)?;
        let geometry = frame.bounds();
        let image = CapturedImage {
            width: frame.width(),
            height: frame.height(),
            bgra: frame.into_bgra(),
        };
        Ok(run_export(
            &settings,
            &image,
            geometry,
            ExportAction::Accept,
            &tasks,
        ))
    });
}

/// The editor's exports: reported like a capture's (notifications, Show
/// Last Capture in Folder), on the main thread.
pub(crate) fn deliver_report<R: Runtime>(app: &AppHandle<R>, report: ExportReport) {
    hand_to_main(app, Delivery::Exported(report));
}

/// Hands `delivery` to [`deliver`] on the main thread.
fn hand_to_main<R: Runtime>(app: &AppHandle<R>, delivery: Delivery) {
    let main = app.clone();
    if let Err(error) = app.run_on_main_thread(move || deliver(&main, delivery)) {
        log::error!("cannot report the capture: {error}");
    }
}

/// Runs `work` on a new thread after `delay_ms`, with the current settings
/// and, after a delay, the time the delay ended. Returns whether the thread
/// started. `reply` hears how it went.
fn spawn<R: Runtime>(
    app: &AppHandle<R>,
    name: &str,
    delay_ms: u32,
    reply: Option<Reply>,
    work: impl FnOnce(Settings, Option<Instant>) -> Result<Delivery, CaptureFailure> + Send + 'static,
) -> bool {
    let settings_path = app.state::<DaemonState>().settings_path.clone();
    let target = app.clone();
    let spawned = std::thread::Builder::new()
        .name(name.to_owned())
        .spawn(move || {
            let requested = (delay_ms > 0).then(|| {
                std::thread::sleep(Duration::from_millis(u64::from(delay_ms)));
                Instant::now()
            });
            let settings = settings::load(&settings_path);
            let delivery = work(settings, requested).unwrap_or_else(|error| {
                log::error!("capture failed: {error}");
                Delivery::Aborted
            });
            if let Some(reply) = reply {
                answer(reply, &delivery);
            }
            hand_to_main(&target, delivery);
        });
    match spawned {
        Ok(_) => true,
        Err(error) => {
            log::error!("cannot start the capture: {error}");
            false
        }
    }
}

pub(crate) fn under_cursor(monitors: &[Monitor]) -> usize {
    let cursor = cursor_position().unwrap_or(PhysicalPoint::new(0, 0));
    monitor_at_or_primary(monitors, cursor)
        .and_then(|found| {
            monitors
                .iter()
                .position(|monitor| std::ptr::eq(monitor, found))
        })
        .unwrap_or(0)
}

fn region_mode(mode: CaptureRegionMode) -> RegionMode {
    match mode {
        CaptureRegionMode::SelectMonitor => RegionMode::SelectMonitor,
        CaptureRegionMode::ActiveMonitor => RegionMode::ActiveMonitor,
        CaptureRegionMode::AllMonitors => RegionMode::AllMonitors,
    }
}

/// How the overlay uses the monitors besides the one it opens on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Spread {
    /// That monitor only.
    One,
    /// Snip across all monitors: it follows the cursor.
    Follow,
    /// The monitor is chosen first (`captureRegionMode` 0).
    Pick,
}

/// The monitor to open on (the highlighted one when picking) and how the
/// others take part.
fn placement(
    monitors: &[Monitor],
    placement: Placement,
    settings: &Settings,
) -> Result<(usize, Spread), CaptureFailure> {
    let index_of = |found: &Monitor| {
        monitors
            .iter()
            .position(|monitor| std::ptr::eq(monitor, found))
            .unwrap_or(0)
    };
    Ok(match placement {
        Placement::Monitor(number) => {
            let index = number as usize;
            if index >= monitors.len() {
                return Err(CaptureFailure::NoSuchScreen);
            }
            (index, Spread::One)
        }
        Placement::UnderCursor => (under_cursor(monitors), Spread::One),
        Placement::Settings => {
            let cursor = cursor_position().unwrap_or(PhysicalPoint::new(0, 0));
            match choose_target(
                monitors,
                cursor,
                region_mode(settings.capture_region_mode()),
            ) {
                Some(CaptureTarget::Single(monitor)) => (index_of(monitor), Spread::One),
                Some(CaptureTarget::Follow { active, .. }) => (index_of(active), Spread::Follow),
                Some(CaptureTarget::Pick {
                    candidates,
                    highlighted,
                }) => {
                    let pick = candidates.get(highlighted).copied();
                    (pick.map_or(0, index_of), Spread::Pick)
                }
                None => return Err(CaptureFailure::Capture(CaptureError::NoMonitors)),
            }
        }
    })
}

fn has_tasks(tasks: &Tasks) -> bool {
    // `initButtons`: with no task (or only --print-geometry) the overlay
    // offers copy and save; otherwise Enter accepts the request
    tasks.copy || tasks.save.is_some() || tasks.print_raw || tasks.accept_on_select
}

/// The overlay's configuration for a capture with `tasks`, spread over the
/// monitors as `spread` says, starting with `initial`.
fn overlay_config(
    settings: &Settings,
    tasks: &Tasks,
    spread: Spread,
    initial: Option<LocalRect>,
) -> OverlayConfig {
    let mut config = OverlayConfig::from_settings(settings);
    config.has_tasks = has_tasks(tasks);
    config.accept_on_select = tasks.accept_on_select;
    // A given region belongs to the monitor the overlay opens on; following
    // the cursor (snip across all monitors) or picking would drop it
    config.follow_cursor = spread == Spread::Follow && initial.is_none();
    config.pick_monitor = spread == Spread::Pick && initial.is_none();
    config.initial_selection = initial;
    config
}

/// Whether an overlay is open, or waiting out its delay.
pub(crate) fn overlay_open() -> bool {
    OVERLAY_OPEN.load(Ordering::SeqCst)
}

/// Builds the editor window ahead once the overlay's first frame is on
/// screen, so WebView2 starting up never delays it; Open in Editor then
/// only hands the image over.
fn prewarm_editor_when_shown<R: Runtime>(app: &AppHandle<R>) {
    let handle = app.clone();
    let spawned = std::thread::Builder::new()
        .name("editor-prewarm".to_owned())
        .spawn(move || {
            let start = Instant::now();
            while !overlay_showing() {
                if !overlay_open() || start.elapsed() > PREWARM_WAIT {
                    return;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            let target = handle.clone();
            let posted = handle.run_on_main_thread(move || {
                // Not for a capture that has ended meanwhile
                if overlay_open() {
                    editor::prewarm(&target);
                }
            });
            if let Err(error) = posted {
                log::error!("cannot pre-warm the editor: {error}");
            }
        });
    if let Err(error) = spawned {
        log::warn!("cannot pre-warm the editor: {error}");
    }
}

/// How long the pre-warm waits for the overlay to show.
const PREWARM_WAIT: Duration = Duration::from_secs(5);

/// Whether an overlay window of this process is showing.
fn overlay_showing() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, IsWindowVisible};
    // `phramer-overlay`'s window class
    let class: Vec<u16> = "PhramerCaptureOverlay"
        .encode_utf16()
        .chain(Some(0))
        .collect();
    // SAFETY: `class` is NUL-terminated and outlives the call; a null title
    // matches any.
    let window = unsafe { FindWindowW(class.as_ptr(), std::ptr::null()) };
    // SAFETY: a plain query; a null or stale handle just answers false.
    !window.is_null() && unsafe { IsWindowVisible(window) } != 0
}

/// Tells `phramer-cli` how the capture went: the output, the notices as
/// 15.0.0's logger printed them, then the exit code.
fn answer(mut reply: Reply, delivery: &Delivery) {
    match delivery {
        Delivery::Exported(report) => {
            reply.stdout(&report.stdout);
            for notice in &report.notices {
                let level = if notice.error { "error" } else { "info" };
                reply.log_line(level, &notice.text);
            }
            reply.finish(EXIT_OK);
        }
        Delivery::Editor(_) => reply.finish(EXIT_OK),
        // Dropping it reports the abort
        Delivery::Aborted => drop(reply),
    }
}

fn run_gui<R: Runtime>(
    app: &AppHandle<R>,
    settings: Settings,
    plan: OverlayPlan,
    requested_at: Instant,
    settings_path: PathBuf,
) -> Result<Delivery, CaptureFailure> {
    let monitors = enumerate_monitors()?;
    let (active, spread) = placement(&monitors, plan.placement, &settings)?;

    let last_region = || LAST_REGION.lock().ok().and_then(|region| *region);
    let initial = match plan.initial_selection {
        InitialSelection::Region(rect) => {
            Some(LocalRect::new(rect.x, rect.y, rect.width, rect.height))
        }
        InitialSelection::LastRegion => last_region(),
        InitialSelection::None if settings.save_last_region() => last_region(),
        InitialSelection::None => None,
    };
    let config = overlay_config(&settings, &plan.tasks, spread, initial);
    // Open in Editor is offered only without tasks
    if !config.has_tasks {
        prewarm_editor_when_shown(app);
    }

    let finished = run_overlay(OverlaySession {
        monitors,
        active,
        config,
        theme: Theme::from_settings(&settings),
        font: BUNDLED_FONT,
        snap_windows: initial.is_none(),
        requested_at,
        settings_path: Some(settings_path),
    })?;
    let (outcome, capture) = (finished.outcome, finished.capture);
    if outcome == phramer_overlay::Outcome::RecordVideo {
        // The mode bar's Video: the overlay is gone, so Snipping Tool's own
        // overlay can come up. Nothing is exported, as after Esc
        // (`CaptureWidget::startVideoCapture`).
        log::info!("capture: video mode chosen");
        let target = app.clone();
        if let Err(error) = app.run_on_main_thread(move || actions::record_video(&target)) {
            log::error!("cannot hand over to Snipping Tool: {error}");
        }
        return Ok(Delivery::Aborted);
    }
    if let (phramer_overlay::Outcome::Deferred(deferred), Some((image, geometry))) =
        (outcome, capture.as_ref())
    {
        if let Ok(mut last) = LAST_REGION.lock() {
            *last = Some(deferred.region);
        }
        return Ok(deferred_action(
            &settings,
            deferred.action,
            image.clone(),
            *geometry,
        ));
    }
    let (phramer_overlay::Outcome::Export(done), Some((image, geometry))) = (outcome, capture)
    else {
        log::info!("capture cancelled");
        return Ok(Delivery::Aborted);
    };
    if let Ok(mut last) = LAST_REGION.lock() {
        *last = Some(done.region);
    }
    Ok(run_export(
        &settings,
        &image,
        geometry,
        done.action,
        &plan.tasks,
    ))
}

/// The overlay's OCR, Open in Editor and App Launcher buttons.
fn deferred_action(
    settings: &Settings,
    action: DeferredAction,
    image: CapturedImage,
    geometry: PhysicalRect,
) -> Delivery {
    log::info!("{action:?}: {}x{}", image.width, image.height);
    match action {
        DeferredAction::Ocr => Delivery::Exported(handoff::read_text_at(&image, Some(geometry))),
        DeferredAction::OpenInEditor => match handoff::save_for_editor(&image) {
            Ok(path) => Delivery::Editor(path),
            Err(report) => Delivery::Exported(report),
        },
        DeferredAction::OpenWith => Delivery::Exported(handoff::open_with(settings, &image)),
    }
}

fn crop(frame: &[u8], frame_width: u32, region: LocalRect) -> CapturedImage {
    let unsigned = |value: i32| u32::try_from(value).unwrap_or(0);
    CapturedImage::crop(
        frame,
        frame_width,
        (
            unsigned(region.x),
            unsigned(region.y),
            unsigned(region.width),
            unsigned(region.height),
        ),
    )
}

fn export_tasks(tasks: &Tasks) -> CaptureTasks {
    CaptureTasks {
        copy: tasks.copy,
        save: tasks.save.as_ref().map(|save| match save {
            SaveTo::Default => SaveTarget::Default,
            SaveTo::Path(path) => SaveTarget::Path(path.clone()),
        }),
        print_geometry: tasks.print_geometry,
        print_raw: tasks.print_raw,
    }
}

pub(crate) fn run_export(
    settings: &Settings,
    image: &CapturedImage,
    geometry: PhysicalRect,
    action: ExportAction,
    tasks: &Tasks,
) -> Delivery {
    let mut handoff = CaptureHandoff(SystemClipboard);
    run_export_with(
        settings,
        image,
        geometry,
        action,
        tasks,
        (None, None),
        &mut handoff,
    )
}

/// A capture's handoff: the system clipboard, and `saveToFilesystemGUI`'s
/// save dialog when `savePathFixed` is off.
struct CaptureHandoff(SystemClipboard);

impl Handoff for CaptureHandoff {
    fn copy_image(&mut self, image: &CapturedImage, jpeg: Option<u8>) -> Result<(), OverlayError> {
        self.0.copy_image(image, jpeg)
    }

    fn copy_text(&mut self, text: &str) -> Result<(), OverlayError> {
        self.0.copy_text(text)
    }

    fn copy_file(&mut self, path: &Path) -> Result<(), OverlayError> {
        self.0.copy_file(path)
    }

    fn choose_save_path(&mut self, suggested: &Path) -> Option<PathBuf> {
        let format = suggested
            .extension()
            .and_then(|extension| ImageFormat::from_extension(&extension.to_string_lossy()))
            .unwrap_or(ImageFormat::Png);
        let mut dialog = rfd::FileDialog::new().set_title("Save screenshot");
        if let Some(folder) = suggested.parent() {
            dialog = dialog.set_directory(folder);
        }
        if let Some(name) = suggested.file_name() {
            dialog = dialog.set_file_name(name.to_string_lossy());
        }
        for (label, extensions) in editor::save_filters(format) {
            dialog = dialog.add_filter(label, extensions);
        }
        dialog.save_file()
    }
}

/// `run_export`, with `name` (no extension) for a file saved into a folder
/// instead of the filename pattern's, and `format` for what a save writes
/// instead of the configured one: the editor's image names and formats.
pub(crate) fn run_export_named(
    settings: &Settings,
    image: &CapturedImage,
    geometry: PhysicalRect,
    action: ExportAction,
    tasks: &Tasks,
    name: Option<&str>,
    format: Option<ImageFormat>,
) -> Delivery {
    let mut clipboard = SystemClipboard;
    run_export_with(
        settings,
        image,
        geometry,
        action,
        tasks,
        (name, format),
        &mut clipboard,
    )
}

fn run_export_with(
    settings: &Settings,
    image: &CapturedImage,
    geometry: PhysicalRect,
    action: ExportAction,
    tasks: &Tasks,
    (name, format): (Option<&str>, Option<ImageFormat>),
    handoff: &mut dyn Handoff,
) -> Delivery {
    let time = LocalTime::now();
    let mut context = ExportContext {
        settings,
        time: &time,
        handoff,
        name,
        format,
    };
    Delivery::Exported(export(
        image,
        geometry,
        action,
        &export_tasks(tasks),
        &mut context,
    ))
}

/// The save dialog's folder is the save folder from now on (`setSavePath`).
fn remember_save_folder(settings_path: &Path, folder: &Path) {
    let saved = Settings::load(settings_path).and_then(|mut current| {
        current.set_save_path(folder);
        current.save(settings_path)
    });
    if let Err(error) = saved {
        log::error!(
            "cannot remember {} as the save folder: {error}",
            folder.display()
        );
    }
}

/// On the main thread: remembers the saved file, updates the tray menu and
/// sends the notifications.
fn deliver<R: Runtime>(app: &AppHandle<R>, delivery: Delivery) {
    // A capture that ended any other way than Open in Editor leaves no
    // pre-warmed editor behind
    if !matches!(delivery, Delivery::Editor(_)) && !overlay_open() {
        editor::release_warm(app);
    }
    crate::capture_launcher_ui::capture_ended(app, matches!(delivery, Delivery::Aborted));
    let current = settings::load(&app.state::<DaemonState>().settings_path);
    match delivery {
        Delivery::Aborted => {
            log::info!("Screenshot aborted.");
            if current.show_abort_notification() {
                notify::capture_message(app, &current, "Screenshot aborted.", None, false);
            }
        }
        Delivery::Editor(path) => handoff::open_in_editor(app, &path),
        Delivery::Exported(report) => {
            if let Some(folder) = &report.save_folder {
                remember_save_folder(&app.state::<DaemonState>().settings_path, folder);
            }
            if let Some(saved) = report.saved
                && let Ok(mut last) = LAST_SAVED.lock()
            {
                let first = last.is_none();
                *last = Some(saved);
                drop(last);
                // "Show Last Capture in Folder" becomes available
                if first && let Err(error) = tray::refresh(app, &current) {
                    log::error!("cannot update the tray icon: {error}");
                }
            }
            for notice in &report.notices {
                notify::capture_message(
                    app,
                    &current,
                    &notice.text,
                    notice.path.as_deref(),
                    notice.error,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use phramer_capture::MonitorId;
    use phramer_overlay::{Effect, Event, Key, Modifiers, Outcome, OverlayState};

    use super::*;

    /// The layout `gui --region 320x200+40+40` then E was reported on
    /// (opening the whole monitor): three monitors, snip across all of them.
    fn three_monitors() -> Vec<Monitor> {
        vec![
            Monitor::new(
                MonitorId::new("primary"),
                PhysicalRect::new(0, 0, 1920, 1200),
                120,
                true,
            ),
            Monitor::new(
                MonitorId::new("upper-left"),
                PhysicalRect::new(-846, -1080, 1920, 1080),
                96,
                false,
            ),
            Monitor::new(
                MonitorId::new("upper-right"),
                PhysicalRect::new(1074, -1080, 1920, 1080),
                96,
                false,
            ),
        ]
    }

    #[test]
    fn a_given_region_reaches_open_in_editor_when_snipping_across_monitors() {
        let settings = Settings::default();
        assert_eq!(
            settings.capture_region_mode(),
            CaptureRegionMode::AllMonitors
        );
        let region = LocalRect::new(40, 40, 320, 200);
        // `placement` follows the cursor in this mode
        let config = overlay_config(&settings, &Tasks::default(), Spread::Follow, Some(region));
        let mut state = OverlayState::new(three_monitors(), 1, Vec::new(), config).unwrap();
        state.start();
        let effects = state.handle(
            Event::Key {
                key: Key::Char('e'),
                modifiers: Modifiers::NONE,
            },
            0,
        );
        let deferred = effects.iter().find_map(|effect| match effect {
            Effect::Close(Outcome::Deferred(deferred)) => Some(*deferred),
            _ => None,
        });
        let deferred = deferred.expect("E opens the editor");
        assert_eq!(deferred.action, DeferredAction::OpenInEditor);
        assert_eq!((deferred.monitor, deferred.region), (1, region));
    }

    #[test]
    fn without_a_region_the_overlay_still_follows_the_cursor() {
        let config = overlay_config(
            &Settings::default(),
            &Tasks::default(),
            Spread::Follow,
            None,
        );
        assert!(config.follow_cursor);
        assert!(!config.has_tasks);
        let tasks = Tasks {
            copy: true,
            ..Tasks::default()
        };
        assert!(overlay_config(&Settings::default(), &tasks, Spread::One, None).has_tasks);
    }

    #[test]
    fn select_monitor_mode_opens_the_picker_unless_a_region_is_given() {
        let settings = Settings::default();
        let config = overlay_config(&settings, &Tasks::default(), Spread::Pick, None);
        assert!(config.pick_monitor);
        assert!(!config.follow_cursor);
        let state = OverlayState::new(three_monitors(), 0, Vec::new(), config).unwrap();
        assert!(state.is_picking());
        // A region belongs to the monitor it was given for
        let region = Some(LocalRect::new(40, 40, 320, 200));
        let config = overlay_config(&settings, &Tasks::default(), Spread::Pick, region);
        assert!(!config.pick_monitor);
    }
}
