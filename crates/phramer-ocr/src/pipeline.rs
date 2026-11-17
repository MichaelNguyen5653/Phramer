//! Recognition passes (`ocrpipeline.cpp`): a first pass to measure the
//! glyphs, a second at the scale that suits them, and a binarized one when
//! both read little (or always, at high effort). The best-scoring pass
//! wins. A pass whose image is over the engine's size limit is split into
//! tiles rather than shrunk.

use crate::preprocess::{binarize, ideal_scale, normalize, scale};
use crate::tiling::{crop, merge_tiled_lines, plan_tiles, remap_lines, scale_within_tile_budget};
use crate::{OcrResult, OcrStatus, RgbaImage};

/// A first pass that found nothing usually means glyphs too small to
/// resolve, so the retry is well magnified.
const EMPTY_RESULT_RETRY_SCALE: f64 = 4.0;
/// About a handful of words: below it another attempt is worth the wait.
const MIN_ACCEPTABLE_SCORE: i64 = 40;
/// High effort never magnifies less than this, whatever the first pass
/// measured: the user asking again says the measurement was not to be
/// trusted.
const HIGH_EFFORT_MIN_SCALE: f64 = 2.0;
const LINE_BONUS: i64 = 2;
const JUNK_PENALTY: i64 = 2;

/// How hard to try (`OcrEffort`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OcrEffort {
    /// Stop as soon as a result looks good enough.
    #[default]
    Normal,
    /// Every pass, scaled up more: "Try harder" in the results window.
    High,
}

/// How much a result read (`ocrScoreResult`): characters, a bonus per line,
/// a penalty per lone punctuation mark.
pub fn score(result: &OcrResult) -> i64 {
    if result.status != OcrStatus::Ok {
        return 0;
    }
    let mut total = 0;
    for line in &result.lines {
        total += LINE_BONUS;
        if line.words.is_empty() {
            total += line.text.trim().chars().count() as i64;
            continue;
        }
        for word in &line.words {
            let text = word.text.trim();
            let mut chars = text.chars();
            total += text.chars().count() as i64;
            if let (Some(only), None) = (chars.next(), chars.next())
                && !only.is_alphanumeric()
            {
                total -= JUNK_PENALTY;
            }
        }
    }
    total
}

/// One already-scaled image, split when the engine will not take it whole
/// and what the pieces found merged (`recognizeTiled`). Also returns the
/// number of tiles.
fn recognize_tiled(
    image: &RgbaImage,
    language: &str,
    max_dimension: u32,
    recognize: &mut impl FnMut(&RgbaImage, &str) -> OcrResult,
) -> (OcrResult, usize) {
    let tiles = plan_tiles(image.width, image.height, max_dimension);
    if tiles.len() <= 1 {
        return (recognize(image, language), tiles.len());
    }
    let mut lines = Vec::new();
    let mut first_error = None;
    let mut failures = 0;
    for tile in &tiles {
        let part = recognize(&crop(image, *tile), language);
        match part.status {
            // No other tile can do better; stop paying for them
            OcrStatus::NoLanguageInstalled | OcrStatus::Unsupported => return (part, tiles.len()),
            OcrStatus::EngineError(message) => {
                failures += 1;
                first_error.get_or_insert(message);
            }
            _ => lines.extend(remap_lines(
                &part.lines,
                f64::from(tile.x),
                f64::from(tile.y),
            )),
        }
    }
    if failures == tiles.len() {
        return (
            OcrResult::with_status(OcrStatus::EngineError(first_error.unwrap_or_default())),
            tiles.len(),
        );
    }
    let lines = merge_tiled_lines(&lines);
    let result = if lines.is_empty() {
        OcrResult::with_status(OcrStatus::NoTextFound)
    } else {
        OcrResult::ok(lines)
    };
    (result, tiles.len())
}

fn describe_pass(name: &str, scale: f64, tiles: usize, score: i64) -> String {
    format!("pass {name}: scale {scale:.2}, {tiles} tile(s), score {score}")
}

/// Runs the passes over `capture` with `recognize(image, language)`.
/// `max_dimension` is the engine's limit on a side (0: none). Lines come
/// back in engine order; `diagnostics` says what was tried.
pub fn run_pipeline(
    capture: &RgbaImage,
    language: &str,
    max_dimension: u32,
    effort: OcrEffort,
    mut recognize: impl FnMut(&RgbaImage, &str) -> OcrResult,
) -> OcrResult {
    let mut notes = vec![format!("capture {}x{}", capture.width, capture.height)];
    let normalized = normalize(capture);

    // Pass A measures: nothing says how tall the glyphs are until the
    // engine has put boxes around some
    let (mut best, tiles) = recognize_tiled(
        &scale(&normalized, 1.0, max_dimension),
        language,
        max_dimension,
        &mut recognize,
    );
    let mut best_score = score(&best);
    notes.push(describe_pass("A", 1.0, tiles, best_score));
    if matches!(
        best.status,
        OcrStatus::NoLanguageInstalled | OcrStatus::Unsupported
    ) {
        best.diagnostics = notes.join("\n");
        return best;
    }

    let measured = if best.lines.is_empty() {
        EMPTY_RESULT_RETRY_SCALE
    } else {
        ideal_scale(&best.lines)
    };
    let factor = match effort {
        OcrEffort::High => measured.max(HIGH_EFFORT_MIN_SCALE),
        OcrEffort::Normal => measured,
    };
    // Tiling, not clamping, keeps the engine within its limit; the budget
    // stops an upscale turning into an unbounded number of round trips
    let factor =
        scale_within_tile_budget(normalized.width, normalized.height, factor, max_dimension);

    let mut scaled = None;
    if (factor - 1.0).abs() > 0.05 {
        // From the untouched image, not pass A's input, so its
        // interpolation loss is not compounded
        let image = scale(&normalized, factor, 0);
        let (pass, tiles) = recognize_tiled(&image, language, max_dimension, &mut recognize);
        let pass_score = score(&pass);
        notes.push(describe_pass("B", factor, tiles, pass_score));
        if pass_score > best_score {
            best = pass;
            best_score = pass_score;
        }
        scaled = Some(image);
    }

    if effort == OcrEffort::High || best_score < MIN_ACCEPTABLE_SCORE {
        let image = scaled.unwrap_or_else(|| scale(&normalized, factor, 0));
        let (pass, tiles) =
            recognize_tiled(&binarize(&image), language, max_dimension, &mut recognize);
        let pass_score = score(&pass);
        notes.push(describe_pass("C (binarized)", factor, tiles, pass_score));
        if pass_score > best_score {
            best = pass;
            best_score = pass_score;
        }
    }
    notes.push(format!("kept score {best_score}"));
    log::debug!("ocr: {}", notes.join("; "));
    best.diagnostics = notes.join("\n");
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{OcrLine, OcrWord, Rect};

    fn result(words: &[&str], glyph_height: f64) -> OcrResult {
        let words = words
            .iter()
            .enumerate()
            .map(|(index, text)| OcrWord {
                text: (*text).to_owned(),
                bounding_box: Rect::new(index as f64 * 50.0, 0.0, 40.0, glyph_height),
            })
            .collect();
        OcrResult::ok(vec![OcrLine::from_words(words)])
    }

    fn page() -> RgbaImage {
        RgbaImage {
            width: 200,
            height: 100,
            rgba: [255, 255, 255, 255].repeat(200 * 100),
        }
    }

    #[test]
    fn the_score_counts_characters_lines_and_junk() {
        assert_eq!(score(&result(&["hello", "you"], 10.0)), 2 + 5 + 3);
        assert_eq!(score(&result(&["a", "|"], 10.0)), 2 + 1 + 1 - 2);
        assert_eq!(score(&OcrResult::with_status(OcrStatus::NoTextFound)), 0);
    }

    #[test]
    fn small_glyphs_get_a_magnified_second_pass() {
        let mut seen = Vec::new();
        let found = run_pipeline(&page(), "en-US", 0, OcrEffort::Normal, |image, language| {
            seen.push((image.width, language.to_owned()));
            if seen.len() == 1 {
                result(&["tiny"], 10.0)
            } else {
                result(
                    &["readable", "text", "at", "last", "and", "plenty", "more"],
                    40.0,
                )
            }
        });
        assert_eq!(seen[0], (200, "en-US".to_owned()));
        // 40 / 10: four times the size
        assert_eq!(seen[1].0, 800);
        assert_eq!(found.lines[0].words.len(), 7);
    }

    #[test]
    fn a_weak_result_is_tried_binarized_and_the_best_kept() {
        let mut passes = 0;
        let found = run_pipeline(&page(), "", 0, OcrEffort::Normal, |_, _| {
            passes += 1;
            match passes {
                1 => result(&["ab"], 40.0),
                _ => result(&["abcdef"], 40.0),
            }
        });
        // Glyphs already ideal: no pass B, but the score is low, so pass C
        assert_eq!(passes, 2);
        assert_eq!(found.lines[0].words[0].text, "abcdef");
    }

    #[test]
    fn nothing_found_retries_at_four_times() {
        let mut sizes = Vec::new();
        let found = run_pipeline(&page(), "", 0, OcrEffort::Normal, |image, _| {
            sizes.push(image.width);
            OcrResult::with_status(OcrStatus::NoTextFound)
        });
        assert_eq!(sizes, [200, 800, 800]);
        assert_eq!(found.status, OcrStatus::NoTextFound);
    }

    #[test]
    fn no_language_stops_at_once() {
        let mut passes = 0;
        let found = run_pipeline(&page(), "", 0, OcrEffort::Normal, |_, _| {
            passes += 1;
            OcrResult::with_status(OcrStatus::NoLanguageInstalled)
        });
        assert_eq!(passes, 1);
        assert_eq!(found.status, OcrStatus::NoLanguageInstalled);
    }

    fn blank(width: u32, height: u32) -> RgbaImage {
        RgbaImage {
            width,
            height,
            rgba: [255, 255, 255, 255].repeat((width * height) as usize),
        }
    }

    #[test]
    fn an_oversized_pass_is_tiled_rather_than_shrunk() {
        let mut seen = Vec::new();
        // Small glyphs: pass B wants a large upscale of an image already
        // over the limit
        run_pipeline(&blank(3000, 800), "", 1000, OcrEffort::Normal, |image, _| {
            seen.push((image.width, image.height));
            result(&["a", "b", "c"], 8.0)
        });
        assert!(seen.len() > 3, "{seen:?}");
        assert!(seen.iter().all(|(w, h)| *w <= 1000 && *h <= 1000), "{seen:?}");
    }

    #[test]
    fn tiles_are_stitched_into_whole_image_coordinates() {
        let mut calls = 0;
        // Pass A finds 10 px glyphs, so pass B is 4x: 3600x400, four tiles
        // starting at 0, 880, 1760 and 2600. Each reports a line at its own
        // x = 10.
        let found = run_pipeline(&blank(900, 100), "", 1000, OcrEffort::Normal, |image, _| {
            calls += 1;
            assert!(image.width <= 1000 && image.height <= 1000);
            if calls == 1 {
                return result(&["tiny"], 10.0);
            }
            let word = OcrWord {
                text: format!("tile{calls}"),
                bounding_box: Rect::new(10.0, 30.0, 40.0, 40.0),
            };
            OcrResult::ok(vec![OcrLine::from_words(vec![word])])
        });
        assert_eq!(found.status, OcrStatus::Ok);
        let xs: Vec<f64> = found.lines.iter().map(|line| line.bounding_box.x).collect();
        assert_eq!(xs, [10.0, 890.0, 1770.0, 2610.0]);
        assert!(found.diagnostics.contains("pass B: scale 4.00, 4 tile(s)"), "{}", found.diagnostics);
    }

    #[test]
    fn high_effort_runs_more_passes_than_normal() {
        let words = [
            "plenty", "of", "words", "here", "to", "read", "and", "score", "again", "today",
        ];
        let mut normal = 0;
        run_pipeline(&page(), "", 2600, OcrEffort::Normal, |_, _| {
            normal += 1;
            result(&words, 40.0)
        });
        let mut widths = Vec::new();
        run_pipeline(&page(), "", 2600, OcrEffort::High, |image, _| {
            widths.push(image.width);
            result(&words, 40.0)
        });
        assert_eq!(normal, 1);
        // A, B at no less than twice the size, C binarized
        assert_eq!(widths, [200, 400, 400]);
    }

    #[test]
    fn every_pass_is_described_in_the_diagnostics() {
        let found = run_pipeline(&page(), "", 2600, OcrEffort::Normal, |_, _| {
            result(&["tiny"], 40.0)
        });
        assert!(found.diagnostics.contains("capture 200x100"), "{}", found.diagnostics);
        assert!(found.diagnostics.contains("pass A: scale 1.00, 1 tile(s)"));
        assert!(found.diagnostics.contains("kept score"));
    }

    #[test]
    fn every_tile_failing_is_an_engine_error() {
        let found = run_pipeline(&blank(2000, 100), "", 1000, OcrEffort::Normal, |_, _| {
            OcrResult::with_status(OcrStatus::EngineError("0x1".into()))
        });
        assert_eq!(found.status, OcrStatus::EngineError("0x1".into()));
    }
}
