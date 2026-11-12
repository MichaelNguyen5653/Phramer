//! Windows.Media.Ocr (`windowsocrengine.cpp`). Calls run in the process's
//! multithreaded apartment, kept alive for good on first use, so they may
//! come from any thread that has not entered a single-threaded apartment
//! (blocking on a WinRT operation there is not allowed).

use std::sync::Once;

use windows::Globalization::Language;
use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::OcrEngine;
use windows::Storage::Streams::DataWriter;
use windows::Win32::System::Com::CoIncrementMTAUsage;
use windows::core::HSTRING;

use crate::layout::{OcrTextLayout, assemble_text, order_lines};
use crate::pipeline::run_pipeline;
use crate::{OcrLine, OcrResult, OcrStatus, OcrWord, Rect, RgbaImage};

/// The documented limit when the engine cannot be asked.
const DEFAULT_MAX_DIMENSION: u32 = 2600;

/// Keeps the multithreaded apartment up for the life of the process. Not
/// one apartment per call: `windows` caches activation factories process
/// wide, and a factory from an apartment that was torn down crashes the
/// next call that uses it.
fn ensure_apartment() {
    static MTA: Once = Once::new();
    MTA.call_once(|| {
        // SAFETY: no arguments; the usage it takes is never released.
        if let Err(error) = unsafe { CoIncrementMTAUsage() } {
            log::warn!("ocr: no multithreaded apartment: {error}");
        }
    });
}

/// BCP-47 tags of the languages Windows can read, empty when none.
pub fn available_languages() -> Vec<String> {
    ensure_apartment();
    let tags = || -> windows::core::Result<Vec<String>> {
        let languages = OcrEngine::AvailableRecognizerLanguages()?;
        (0..languages.Size()?)
            .map(|index| Ok(languages.GetAt(index)?.LanguageTag()?.to_string()))
            .collect()
    };
    tags().unwrap_or_default()
}

fn max_dimension() -> u32 {
    OcrEngine::MaxImageDimension().unwrap_or(DEFAULT_MAX_DIMENSION)
}

/// The engine for `language` (empty: the user's languages), falling back
/// to the user's languages and then to any installed one, as 15.0.0 did.
fn engine(language: &str) -> Option<OcrEngine> {
    if !language.is_empty()
        && let Ok(engine) = Language::CreateLanguage(&HSTRING::from(language))
            .and_then(|language| OcrEngine::TryCreateFromLanguage(&language))
    {
        return Some(engine);
    }
    if let Ok(engine) = OcrEngine::TryCreateFromUserProfileLanguages() {
        return Some(engine);
    }
    // The profile's languages may lack OCR while another pack has it
    let languages = OcrEngine::AvailableRecognizerLanguages().ok()?;
    if languages.Size().ok()? == 0 {
        return None;
    }
    OcrEngine::TryCreateFromLanguage(&languages.GetAt(0).ok()?).ok()
}

/// One recognition of `image` as it is.
pub fn recognize(image: &RgbaImage, language: &str) -> OcrResult {
    if image.is_empty() {
        return OcrResult::with_status(OcrStatus::NoTextFound);
    }
    ensure_apartment();
    let Some(engine) = engine(language) else {
        return OcrResult::with_status(OcrStatus::NoLanguageInstalled);
    };
    match recognize_with(&engine, image) {
        Ok(lines) if lines.is_empty() => OcrResult::with_status(OcrStatus::NoTextFound),
        Ok(lines) => OcrResult {
            lines,
            status: OcrStatus::Ok,
        },
        Err(error) => OcrResult::with_status(OcrStatus::EngineError(error.message())),
    }
}

fn recognize_with(engine: &OcrEngine, image: &RgbaImage) -> windows::core::Result<Vec<OcrLine>> {
    let mut bgra = image.rgba.clone();
    for pixel in bgra.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    let writer = DataWriter::new()?;
    writer.WriteBytes(&bgra)?;
    let buffer = writer.DetachBuffer()?;
    let width = i32::try_from(image.width).unwrap_or(i32::MAX);
    let height = i32::try_from(image.height).unwrap_or(i32::MAX);
    let bitmap =
        SoftwareBitmap::CreateCopyFromBuffer(&buffer, BitmapPixelFormat::Bgra8, width, height)?;
    // Blocking is fine: this runs on a worker thread
    let result = engine.RecognizeAsync(&bitmap)?.join()?;

    let mut lines = Vec::new();
    let found = result.Lines()?;
    for index in 0..found.Size()? {
        let line = found.GetAt(index)?;
        let words = line.Words()?;
        let mut collected = Vec::new();
        for word_index in 0..words.Size()? {
            let word = words.GetAt(word_index)?;
            let rect = word.BoundingRect()?;
            collected.push(OcrWord {
                text: word.Text()?.to_string(),
                bounding_box: Rect::new(
                    f64::from(rect.X),
                    f64::from(rect.Y),
                    f64::from(rect.Width),
                    f64::from(rect.Height),
                ),
            });
        }
        let mut ocr_line = OcrLine::from_words(collected);
        ocr_line.text = line.Text()?.to_string();
        lines.push(ocr_line);
    }
    Ok(lines)
}

/// The text in `capture`, laid out as `layout` says, or why there is none.
/// `language` is a BCP-47 tag (`ocrLanguage`), empty for the system's.
pub fn read_text(
    capture: &RgbaImage,
    language: &str,
    layout: OcrTextLayout,
) -> Result<String, OcrStatus> {
    ensure_apartment();
    let result = run_pipeline(capture, language, max_dimension(), recognize);
    if result.status != OcrStatus::Ok {
        return Err(result.status);
    }
    let text = assemble_text(&order_lines(&result.lines), layout);
    if text.trim().is_empty() {
        return Err(OcrStatus::NoTextFound);
    }
    Ok(text)
}
