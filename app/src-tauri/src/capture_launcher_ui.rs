//! Capture Launcher (`app/capture-launcher.html`): 15.0.0's
//! `CaptureLauncher` dialog. A mode (Rectangular Region or Full Screen), a
//! monitor when there is more than one, the region to start with (the last
//! one by default) and a delay; "Take new screenshot" hides the window for
//! the capture. A capture that is aborted or fails brings it back; any
//! other end closes it.

use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use tauri::{
    AppHandle, Emitter, Manager, Runtime, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

use crate::cli::{FullRequest, GuiRequest, InitialSelection, Rect, SaveTo, Tasks};
use crate::{actions, capture};

/// The window's label; `capabilities/capture-launcher.json` names it.
pub(crate) const LABEL: &str = "capture-launcher";
const TITLE: &str = "Capture Launcher";
const WIDTH: f64 = 440.0;
const HEIGHT: f64 = 330.0;
/// The page is told its capture came back (aborted or failed).
const BACK_EVENT: &str = "capture-launcher:back";

/// `additionalDelayToHideUI`: time for the window to be gone from the
/// screen before the capture.
const HIDE_MS: u32 = 600;
/// The delay box's maximum, in seconds.
const MAX_DELAY_S: u32 = 999;

/// What the page asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Mode {
    Region,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
pub(crate) struct Choice {
    pub(crate) mode: Mode,
    pub(crate) monitor: u32,
    /// Physical pixels relative to the monitor; empty for none.
    pub(crate) region: Rect,
    pub(crate) delay_s: u32,
}

/// The capture a choice starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Launch {
    Gui(GuiRequest),
    Full(FullRequest),
}

/// A capture started here is under way.
static WAITING: AtomicBool = AtomicBool::new(false);

/// `CaptureLauncher::startCapture`'s request.
pub(crate) fn launch_for(choice: Choice) -> Launch {
    let delay_ms = choice.delay_s.min(MAX_DELAY_S) * 1000 + HIDE_MS;
    match choice.mode {
        Mode::Region => {
            let area = choice.region;
            let initial_selection = if area.width > 0 && area.height > 0 {
                InitialSelection::Region(area)
            } else {
                InitialSelection::None
            };
            Launch::Gui(GuiRequest {
                delay_ms,
                initial_selection,
                monitor: Some(choice.monitor),
                tasks: Tasks::default(),
            })
        }
        // 15.0.0 took every monitor and saved the result (to the folder or
        // through the save dialog, `saveToFilesystemGUI`)
        Mode::Full => Launch::Full(FullRequest {
            delay_ms,
            tasks: Tasks {
                save: Some(SaveTo::Default),
                ..Tasks::default()
            },
        }),
    }
}

/// Opens the launcher, or brings the open one forward.
pub(crate) fn open<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    let page = WebviewUrl::App("capture-launcher.html".into());
    let built = WebviewWindowBuilder::new(app, LABEL, page)
        .title(TITLE)
        .inner_size(WIDTH, HEIGHT)
        .resizable(false)
        .maximizable(false)
        .decorations(false)
        .center()
        .visible(false)
        .build();
    match built {
        Ok(_) => log::info!("{LABEL}: window created"),
        Err(error) => log::error!("cannot create the capture launcher: {error}"),
    }
}

/// One entry of the Monitor list (`Monitor %1: %2 (%3x%4)`).
#[derive(Debug, Serialize)]
pub(crate) struct MonitorEntry {
    name: String,
    width: i32,
    height: i32,
}

/// What the page starts from.
#[derive(Debug, Serialize)]
pub(crate) struct LauncherInit {
    monitors: Vec<MonitorEntry>,
    /// The monitor under the cursor, selected first.
    current: u32,
    /// `getLastRegion`; absent before the first capture.
    last_region: Option<Rect>,
}

#[tauri::command]
pub(crate) fn capture_launcher_init() -> LauncherInit {
    let (monitors, current) = match phramer_capture::enumerate_monitors() {
        Ok(monitors) => {
            let current = capture::under_cursor(&monitors);
            (monitors, current)
        }
        Err(error) => {
            log::error!("capture launcher: no monitors: {error}");
            (Vec::new(), 0)
        }
    };
    LauncherInit {
        monitors: monitors
            .iter()
            .map(|monitor| {
                let bounds = monitor.bounds();
                let id = monitor.id().as_str();
                MonitorEntry {
                    name: id.strip_prefix(r"\\.\").unwrap_or(id).to_owned(),
                    width: bounds.width,
                    height: bounds.height,
                }
            })
            .collect(),
        current: u32::try_from(current).unwrap_or(0),
        last_region: capture::last_region().map(|area| Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: area.height,
        }),
    }
}

/// "Take new screenshot". Without a delay the window goes at once; with
/// one the page counts down and hides itself at the end.
#[tauri::command]
pub(crate) fn capture_launcher_take<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    choice: Choice,
) {
    if choice.delay_s == 0
        && let Err(error) = window.hide()
    {
        log::error!("capture launcher: cannot hide: {error}");
    }
    WAITING.store(true, Ordering::Release);
    match launch_for(choice) {
        Launch::Gui(request) => actions::capture_gui(&app, request),
        Launch::Full(request) => actions::capture_full(&app, request, None),
    }
}

/// A capture ended. One started here that was aborted or failed brings the
/// launcher back for another try (`onCaptureFailed`); otherwise it closes.
pub(crate) fn capture_ended<R: Runtime>(app: &AppHandle<R>, aborted: bool) {
    if !WAITING.swap(false, Ordering::AcqRel) {
        return;
    }
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    if aborted {
        let _ = window.show();
        let _ = window.set_focus();
        if let Err(error) = window.emit(BACK_EVENT, ()) {
            log::error!("capture launcher: {error}");
        }
    } else if let Err(error) = window.destroy() {
        log::error!("capture launcher: cannot close: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: i32, y: i32, width: i32, height: i32) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn a_region_capture_starts_on_the_monitor_with_the_area() {
        let launch = launch_for(Choice {
            mode: Mode::Region,
            monitor: 1,
            region: rect(10, 20, 300, 200),
            delay_s: 3,
        });
        assert_eq!(
            launch,
            Launch::Gui(GuiRequest {
                delay_ms: 3600,
                initial_selection: InitialSelection::Region(rect(10, 20, 300, 200)),
                monitor: Some(1),
                tasks: Tasks::default(),
            })
        );
    }

    #[test]
    fn an_empty_area_starts_without_a_selection() {
        let Launch::Gui(request) = launch_for(Choice {
            mode: Mode::Region,
            monitor: 0,
            region: rect(0, 0, 0, 0),
            delay_s: 0,
        }) else {
            panic!("not a region capture");
        };
        assert_eq!(request.initial_selection, InitialSelection::None);
        assert_eq!(request.delay_ms, HIDE_MS);
    }

    #[test]
    fn full_screen_takes_every_monitor_and_saves_it() {
        let launch = launch_for(Choice {
            mode: Mode::Full,
            monitor: 1,
            region: rect(10, 20, 300, 200),
            delay_s: 2,
        });
        assert_eq!(
            launch,
            Launch::Full(FullRequest {
                delay_ms: 2600,
                tasks: Tasks {
                    save: Some(SaveTo::Default),
                    ..Tasks::default()
                },
            })
        );
    }

    #[test]
    fn the_delay_stops_at_999_seconds() {
        let Launch::Full(request) = launch_for(Choice {
            mode: Mode::Full,
            monitor: 0,
            region: rect(0, 0, 0, 0),
            delay_s: 5000,
        }) else {
            panic!("not full screen");
        };
        assert_eq!(request.delay_ms, MAX_DELAY_S * 1000 + HIDE_MS);
    }
}
