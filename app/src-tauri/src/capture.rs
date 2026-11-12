//! Captures: the overlay (`gui`, the tray, the hotkey), one monitor
//! (`screen`) and the whole desktop (`full`). Each runs on a thread of its
//! own, so the tray and the hotkeys stay responsive; what an export leaves
//! behind (the last saved file, notifications) is handed back to the main
//! thread.
//!
//! As in 15.0.0 the overlay exports exactly once, after its windows are
//! gone: [`phramer_overlay::run_overlay`] returns the one outcome, and this
//! module is its only caller.

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use phramer_capture::{
    CaptureError, CaptureTarget, LocalRect, Monitor, PhysicalPoint, RegionMode, choose_target,
    cursor_position, enumerate_monitors, grab_monitor, grab_virtual_desktop, monitor_at_or_primary,
};
use phramer_core::config::{CaptureRegionMode, Settings};
use phramer_overlay::{
    BUNDLED_FONT, CaptureTasks, CapturedImage, ExportAction, ExportContext, ExportReport,
    LocalTime, OverlayConfig, OverlayError, OverlaySession, SaveTarget, SystemClipboard, Theme,
    export, run_overlay,
};
use tauri::{AppHandle, Manager, Runtime};

use crate::cli::{InitialSelection, Rect, SaveTo, Tasks};
use crate::daemon::DaemonState;
use crate::{notify, settings, tray};

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
enum Delivery {
    Exported(ExportReport),
    /// Cancelled, or refused (`captureFailed`).
    Aborted,
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

pub(crate) fn last_saved() -> Option<PathBuf> {
    LAST_SAVED.lock().ok().and_then(|saved| saved.clone())
}

/// Opens the overlay.
pub(crate) fn overlay<R: Runtime>(app: &AppHandle<R>, plan: OverlayPlan) {
    let requested_at = Instant::now();
    if OVERLAY_OPEN.swap(true, Ordering::SeqCst) {
        log::warn!("a capture is already open");
        deliver(app, Delivery::Aborted);
        return;
    }
    let delay = plan.delay_ms;
    let started = spawn(app, "capture-overlay", delay, move |settings, requested| {
        // Reopened for the next capture however this one ends
        let _open = OverlayOpen;
        run_gui(settings, plan, requested.unwrap_or(requested_at))
    });
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

/// `Flameshot::screen` without `--edit`: one monitor, straight to export.
pub(crate) fn screen<R: Runtime>(
    app: &AppHandle<R>,
    delay_ms: u32,
    monitor: Option<u32>,
    region: Option<Rect>,
    tasks: Tasks,
) {
    spawn(app, "capture-screen", delay_ms, move |settings, _| {
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
        Ok(run_export(
            &settings,
            &image,
            target.rect_to_physical(region),
            ExportAction::Accept,
            &tasks,
        ))
    });
}

/// `Flameshot::full`: every monitor in one image the size of the virtual
/// desktop.
pub(crate) fn full<R: Runtime>(app: &AppHandle<R>, delay_ms: u32, tasks: Tasks) {
    spawn(app, "capture-full", delay_ms, move |settings, _| {
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

/// Runs `work` on a new thread after `delay_ms`, with the current settings
/// and, after a delay, the time the delay ended. Returns whether the thread
/// started.
fn spawn<R: Runtime>(
    app: &AppHandle<R>,
    name: &str,
    delay_ms: u32,
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
            let main = target.clone();
            if let Err(error) = target.run_on_main_thread(move || deliver(&main, delivery)) {
                log::error!("cannot report the capture: {error}");
            }
        });
    match spawned {
        Ok(_) => true,
        Err(error) => {
            log::error!("cannot start the capture: {error}");
            false
        }
    }
}

fn under_cursor(monitors: &[Monitor]) -> usize {
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

/// The monitor to open on and whether to follow the cursor.
fn placement(
    monitors: &[Monitor],
    placement: Placement,
    settings: &Settings,
) -> Result<(usize, bool), CaptureFailure> {
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
            (index, false)
        }
        Placement::UnderCursor => (under_cursor(monitors), false),
        Placement::Settings => {
            let cursor = cursor_position().unwrap_or(PhysicalPoint::new(0, 0));
            match choose_target(
                monitors,
                cursor,
                region_mode(settings.capture_region_mode()),
            ) {
                Some(CaptureTarget::Single(monitor)) => (index_of(monitor), false),
                Some(CaptureTarget::Follow { active, .. }) => (index_of(active), true),
                Some(CaptureTarget::Pick {
                    candidates,
                    highlighted,
                }) => {
                    // The monitor picker is not built yet: the highlighted
                    // one (under the cursor) is the pick
                    log::info!(
                        "monitor picker: not implemented yet; using the monitor under the cursor"
                    );
                    let pick = candidates.get(highlighted).copied();
                    (pick.map_or(0, index_of), false)
                }
                None => return Err(CaptureFailure::Capture(CaptureError::NoMonitors)),
            }
        }
    })
}

fn has_tasks(tasks: &Tasks) -> bool {
    // `initButtons`: with no task (or only --print-geometry) the overlay
    // offers copy and save; otherwise Enter accepts the request
    tasks.copy || tasks.save.is_some() || tasks.print_raw || tasks.pin || tasks.accept_on_select
}

fn run_gui(
    settings: Settings,
    plan: OverlayPlan,
    requested_at: Instant,
) -> Result<Delivery, CaptureFailure> {
    let monitors = enumerate_monitors()?;
    let (active, follow) = placement(&monitors, plan.placement, &settings)?;

    let last_region = || LAST_REGION.lock().ok().and_then(|region| *region);
    let initial = match plan.initial_selection {
        InitialSelection::Region(rect) => {
            Some(LocalRect::new(rect.x, rect.y, rect.width, rect.height))
        }
        InitialSelection::LastRegion => last_region(),
        InitialSelection::None if settings.save_last_region() => last_region(),
        InitialSelection::None => None,
    };
    let mut config = OverlayConfig::from_settings(&settings);
    config.has_tasks = has_tasks(&plan.tasks);
    config.accept_on_select = plan.tasks.accept_on_select;
    config.follow_cursor = follow;
    config.initial_selection = initial;

    let finished = run_overlay(OverlaySession {
        monitors,
        active,
        config,
        theme: Theme::from_settings(&settings),
        font: BUNDLED_FONT,
        snap_windows: initial.is_none(),
        requested_at,
    })?;
    let (phramer_overlay::Outcome::Export(done), Some((image, geometry))) =
        (finished.outcome, finished.capture)
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
        pin: tasks.pin,
    }
}

fn run_export(
    settings: &Settings,
    image: &CapturedImage,
    geometry: phramer_capture::PhysicalRect,
    action: ExportAction,
    tasks: &Tasks,
) -> Delivery {
    let time = LocalTime::now();
    let mut clipboard = SystemClipboard;
    let mut context = ExportContext {
        settings,
        time: &time,
        handoff: &mut clipboard,
    };
    Delivery::Exported(export(
        image,
        geometry,
        action,
        &export_tasks(tasks),
        &mut context,
    ))
}

/// On the main thread: remembers the saved file, updates the tray menu and
/// sends the notifications.
fn deliver<R: Runtime>(app: &AppHandle<R>, delivery: Delivery) {
    let current = settings::load(&app.state::<DaemonState>().settings_path);
    match delivery {
        Delivery::Aborted => {
            log::info!("Screenshot aborted.");
            if current.show_abort_notification() {
                notify::capture_message(app, &current, "Screenshot aborted.", None, false);
            }
        }
        Delivery::Exported(report) => {
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
