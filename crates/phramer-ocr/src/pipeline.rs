//! Recognition passes (`ocrpipeline.cpp`): a first pass to measure the
//! glyphs, a second at the scale that suits them, and a binarized one when
//! both read little. The best-scoring pass wins.
//!
//! 15.0.0 split an image over the engine's size limit into tiles; here it
//! is scaled down to fit instead.

use crate::preprocess::{binarize, ideal_scale, normalize, scale};
use crate::{OcrResult, OcrStatus, RgbaImage};

/// A first pass that found nothing usually means glyphs too small to
/// resolve, so the retry is well magnified.
const EMPTY_RESULT_RETRY_SCALE: f64 = 4.0;
/// About a handful of words: below it another attempt is worth the wait.
const MIN_ACCEPTABLE_SCORE: i64 = 40;
const LINE_BONUS: i64 = 2;
const JUNK_PENALTY: i64 = 2;

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

/// Runs the passes over `capture` with `recognize(image, language)`.
/// `max_dimension` is the engine's limit on a side (0: none). Lines come
/// back in engine order.
pub fn run_pipeline(
    capture: &RgbaImage,
    language: &str,
    max_dimension: u32,
    mut recognize: impl FnMut(&RgbaImage, &str) -> OcrResult,
) -> OcrResult {
    let normalized = normalize(capture);

    // Pass A measures: nothing says how tall the glyphs are until the
    // engine has put boxes around some
    let mut best = recognize(&scale(&normalized, 1.0, max_dimension), language);
    let mut best_score = score(&best);
    log::debug!("ocr pass A: scale 1.00, score {best_score}");
    if matches!(
        best.status,
        OcrStatus::NoLanguageInstalled | OcrStatus::Unsupported
    ) {
        return best;
    }

    let factor = if best.lines.is_empty() {
        EMPTY_RESULT_RETRY_SCALE
    } else {
        ideal_scale(&best.lines)
    };
    let mut scaled = None;
    if (factor - 1.0).abs() > 0.05 {
        let image = scale(&normalized, factor, max_dimension);
        let pass = recognize(&image, language);
        let pass_score = score(&pass);
        log::debug!("ocr pass B: scale {factor:.2}, score {pass_score}");
        if pass_score > best_score {
            best = pass;
            best_score = pass_score;
        }
        scaled = Some(image);
    }

    if best_score < MIN_ACCEPTABLE_SCORE {
        let image = scaled.unwrap_or_else(|| scale(&normalized, factor, max_dimension));
        let pass = recognize(&binarize(&image), language);
        let pass_score = score(&pass);
        log::debug!("ocr pass C (binarized): scale {factor:.2}, score {pass_score}");
        if pass_score > best_score {
            best = pass;
            best_score = pass_score;
        }
    }
    log::debug!("ocr kept score {best_score}");
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
        OcrResult {
            lines: vec![OcrLine::from_words(words)],
            status: OcrStatus::Ok,
        }
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
        let found = run_pipeline(&page(), "en-US", 0, |image, language| {
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
        let found = run_pipeline(&page(), "", 0, |_, _| {
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
        let found = run_pipeline(&page(), "", 0, |image, _| {
            sizes.push(image.width);
            OcrResult::with_status(OcrStatus::NoTextFound)
        });
        assert_eq!(sizes, [200, 800, 800]);
        assert_eq!(found.status, OcrStatus::NoTextFound);
    }

    #[test]
    fn no_language_stops_at_once() {
        let mut passes = 0;
        let found = run_pipeline(&page(), "", 0, |_, _| {
            passes += 1;
            OcrResult::with_status(OcrStatus::NoLanguageInstalled)
        });
        assert_eq!(passes, 1);
        assert_eq!(found.status, OcrStatus::NoLanguageInstalled);
    }
}
