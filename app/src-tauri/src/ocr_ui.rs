//! The OCR results window (`app/ocr.html`): 15.0.0's `OcrResultsWindow`.
//! One window per OCR action, on top of other windows, closed by the user;
//! the text is copied only when they ask (15.0.0 never copied on its own).
//!
//! Recognition runs on a worker thread per request, so the window stays
//! responsive and can be closed mid-recognition: the result then has no one
//! to go to and is dropped. Each request carries its own effort and
//! language, and the page discards answers to requests it has superseded.
//! All three layouts come back at once, so switching layout is instant and
//! never recognises again.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use phramer_core::config::Settings;
use phramer_ocr::{
    OcrEffort, OcrResult, OcrSource, OcrStatus, OcrTextLayout, Rect, RgbaImage, assemble_text,
    status_message,
};
use phramer_overlay::{Handoff, SystemClipboard};
use serde::Serialize;
use tauri::{
    AppHandle, Manager, Runtime, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};

use crate::daemon::DaemonState;
use crate::settings;

/// Labels are `ocr-<n>`; `capabilities/ocr.json` names them.
const LABEL_PREFIX: &str = "ocr-";
const TITLE: &str = "Extracted text";
const WIDTH: f64 = 520.0;
const HEIGHT: f64 = 420.0;
const MIN_WIDTH: f64 = 360.0;
const MIN_HEIGHT: f64 = 280.0;

/// What one window recognises.
struct Job {
    image: Arc<RgbaImage>,
    /// Where the capture sat, physical screen pixels; `None` (the editor)
    /// skips the exact-text path.
    screen_rect: Option<Rect>,
}

/// The open results windows' captures, by label.
#[derive(Default)]
pub(crate) struct OcrWindows {
    jobs: Mutex<HashMap<String, Job>>,
    next: AtomicU64,
}

/// Opens a results window for `image`. From any thread.
pub(crate) fn open<R: Runtime>(app: &AppHandle<R>, image: RgbaImage, screen_rect: Option<Rect>) {
    let windows = app.state::<OcrWindows>();
    let label = format!(
        "{LABEL_PREFIX}{}",
        windows.next.fetch_add(1, Ordering::Relaxed) + 1
    );
    if let Ok(mut jobs) = windows.jobs.lock() {
        jobs.insert(
            label.clone(),
            Job {
                image: Arc::new(image),
                screen_rect,
            },
        );
    }
    let target = app.clone();
    let built = app.run_on_main_thread(move || build(&target, label));
    if let Err(error) = built {
        log::error!("ocr: cannot open the results window: {error}");
    }
}

fn build<R: Runtime>(app: &AppHandle<R>, label: String) {
    let built = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("ocr.html".into()))
        .title(TITLE)
        .inner_size(WIDTH, HEIGHT)
        .min_inner_size(MIN_WIDTH, MIN_HEIGHT)
        .decorations(false)
        // `Qt::WindowStaysOnTopHint`
        .always_on_top(true)
        .center()
        .visible(false)
        .build();
    match built {
        Ok(window) => {
            let handle = app.clone();
            let closed = label.clone();
            window.on_window_event(move |event| {
                if matches!(event, WindowEvent::Destroyed) {
                    if let Ok(mut jobs) = handle.state::<OcrWindows>().jobs.lock() {
                        jobs.remove(&closed);
                    }
                    log::info!("{closed}: window closed");
                }
            });
            log::info!("{label}: window created");
        }
        Err(error) => {
            log::error!("cannot create the OCR window: {error}");
            if let Ok(mut jobs) = app.state::<OcrWindows>().jobs.lock() {
                jobs.remove(&label);
            }
        }
    }
}

fn settings_path<R: Runtime>(app: &AppHandle<R>) -> std::path::PathBuf {
    app.state::<DaemonState>().settings_path.clone()
}

/// What the page starts from.
#[derive(Debug, Serialize, PartialEq)]
pub(crate) struct OcrInit {
    /// `ocrTextLayout`: 0 Preserve, 1 Plain, 2 Join.
    layout: i32,
    /// `ocrLanguage`; empty for the system's.
    language: String,
}

#[tauri::command]
pub(crate) fn ocr_init<R: Runtime>(app: AppHandle<R>) -> OcrInit {
    let current = settings::load(&settings_path(&app));
    OcrInit {
        layout: current.ocr_text_layout(),
        language: current.ocr_language(),
    }
}

/// An installed recognition language.
#[derive(Debug, Serialize, PartialEq)]
pub(crate) struct Language {
    tag: String,
    name: String,
}

/// The installed languages; slow (WinRT), so off the main thread.
#[tauri::command]
pub(crate) async fn ocr_languages() -> Vec<Language> {
    let probe = tauri::async_runtime::spawn_blocking(|| {
        phramer_ocr::available_languages()
            .into_iter()
            .map(|language| Language {
                tag: language.tag,
                name: language.name,
            })
            .collect()
    });
    probe.await.unwrap_or_default()
}

/// One recognition's answer, with the text in every layout.
#[derive(Debug, Serialize, PartialEq)]
pub(crate) struct Recognition {
    /// `true` when text was found.
    ok: bool,
    /// What the user is told when it was not, in 15.0.0's words.
    message: Option<String>,
    /// Read from the window itself rather than recognised.
    exact: bool,
    diagnostics: String,
    /// Preserve, Plain and Join, in `ocrTextLayout`'s order.
    texts: [String; 3],
}

fn recognition(result: &OcrResult) -> Recognition {
    let ok = result.status == OcrStatus::Ok;
    let texts = if ok {
        [
            OcrTextLayout::Preserve,
            OcrTextLayout::Plain,
            OcrTextLayout::Join,
        ]
        .map(|layout| assemble_text(&result.lines, layout))
    } else {
        Default::default()
    };
    Recognition {
        ok,
        message: status_message(&result.status),
        exact: result.source == OcrSource::Exact,
        diagnostics: result.diagnostics.clone(),
        texts,
    }
}

/// Recognises the window's capture; `effort` is `"high"` for Try harder.
#[tauri::command]
pub(crate) async fn ocr_recognize<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    language: String,
    effort: String,
) -> Result<Recognition, String> {
    let (image, screen_rect) = {
        let windows = app.state::<OcrWindows>();
        let jobs = windows.jobs.lock().map_err(|error| error.to_string())?;
        let job = jobs
            .get(window.label())
            .ok_or_else(|| format!("{} has no capture", window.label()))?;
        (Arc::clone(&job.image), job.screen_rect)
    };
    let effort = if effort == "high" {
        OcrEffort::High
    } else {
        OcrEffort::Normal
    };
    let label = window.label().to_owned();
    let started = std::time::Instant::now();
    // Blocks on WinRT: a worker of its own, never the main thread
    let result = tauri::async_runtime::spawn_blocking(move || {
        phramer_ocr::recognize_capture(&image, &language, effort, screen_rect)
    })
    .await
    .map_err(|error| {
        status_message(&OcrStatus::EngineError(error.to_string())).unwrap_or_default()
    })?;
    log::info!(
        "{label}: {:?} ({effort:?}, {:?}) in {} ms",
        result.status,
        result.source,
        started.elapsed().as_millis()
    );
    Ok(recognition(&result))
}

fn save(
    app_path: &std::path::Path,
    change: impl FnOnce(&mut Settings) -> Result<(), String>,
) -> Result<(), String> {
    let mut current = Settings::load(app_path).map_err(|error| error.to_string())?;
    change(&mut current)?;
    current.save(app_path).map_err(|error| error.to_string())
}

/// The layout chosen in the window, remembered (`ocrTextLayout`).
#[tauri::command]
pub(crate) fn ocr_set_layout<R: Runtime>(app: AppHandle<R>, layout: i32) -> Result<(), String> {
    save(&settings_path(&app), |current| {
        current
            .set_ocr_text_layout(layout)
            .map_err(|error| error.to_string())
    })
}

/// The language chosen in the window, remembered (`ocrLanguage`).
#[tauri::command]
pub(crate) fn ocr_set_language<R: Runtime>(
    app: AppHandle<R>,
    language: String,
) -> Result<(), String> {
    save(&settings_path(&app), |current| {
        current.set_ocr_language(&language);
        Ok(())
    })
}

/// Copy all.
#[tauri::command]
pub(crate) fn ocr_copy(text: String) -> Result<(), String> {
    SystemClipboard
        .copy_text(&text)
        .map_err(|error| format!("Error while saving to clipboard: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use phramer_ocr::OcrLine;

    fn line(text: &str, x: f64, y: f64) -> OcrLine {
        OcrLine {
            text: text.into(),
            bounding_box: Rect::new(x, y, 8.0 * text.len() as f64, 16.0),
            words: Vec::new(),
        }
    }

    #[test]
    fn every_layout_comes_back_at_once() {
        let result = OcrResult {
            diagnostics: "pass A".into(),
            ..OcrResult::ok(vec![
                line("fn main() {", 0.0, 0.0),
                line("call();", 32.0, 20.0),
                line("}", 0.0, 40.0),
            ])
        };
        let answer = recognition(&result);
        assert!(answer.ok);
        assert!(!answer.exact);
        assert_eq!(answer.message, None);
        assert_eq!(answer.texts[0], "fn main() {\n    call();\n}");
        assert_eq!(answer.texts[1], "fn main() {\ncall();\n}");
        assert_eq!(answer.diagnostics, "pass A");
    }

    #[test]
    fn a_failure_carries_15_0_0_s_message() {
        let answer = recognition(&OcrResult::with_status(OcrStatus::NoTextFound));
        assert!(!answer.ok);
        assert_eq!(
            answer.message.as_deref(),
            Some("No text was found in the selection.")
        );
        assert_eq!(answer.texts, [String::new(), String::new(), String::new()]);
    }

    #[test]
    fn exact_text_is_marked() {
        let result = OcrResult {
            source: OcrSource::Exact,
            ..OcrResult::ok(vec![line("exact", 0.0, 0.0)])
        };
        assert!(recognition(&result).exact);
    }
}
