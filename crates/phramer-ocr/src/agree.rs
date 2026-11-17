//! Whether text read straight from a window says what the capture shows
//! (`ocrTextsAgree`). Pure.

use std::collections::HashMap;

/// Share of words each side must find in the other: low enough for the odd
/// misread word, high enough that a different window or text past the
/// selection's edge fails it.
const MIN_WORD_AGREEMENT: f64 = 0.7;
/// Below this many shared words agreement proves nothing: "OK" or "2024"
/// appears in plenty of windows the capture never showed.
const MIN_SHARED_WORDS: usize = 3;

/// Runs of letters and digits, lower-cased.
fn words_of(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// Whether `exact` (read from the window) and `recognized` (read from the
/// capture) are the same text, allowing for a few recognition errors.
pub fn texts_agree(recognized: &str, exact: &str) -> bool {
    let seen = words_of(recognized);
    let read = words_of(exact);
    if seen.is_empty() || read.is_empty() {
        return false;
    }
    // A multiset intersection, so a repeated word is not matched again and
    // again against a single occurrence
    let mut available: HashMap<&str, usize> = HashMap::new();
    for word in &read {
        *available.entry(word).or_default() += 1;
    }
    let mut matched = 0;
    for word in &seen {
        if let Some(count) = available.get_mut(word.as_str())
            && *count > 0
        {
            *count -= 1;
            matched += 1;
        }
    }
    let matched_share = matched as f64;
    matched >= MIN_SHARED_WORDS
        && matched_share >= MIN_WORD_AGREEMENT * seen.len() as f64
        && matched_share >= MIN_WORD_AGREEMENT * read.len() as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_text_agrees() {
        assert!(texts_agree(
            "Build failed at step 3",
            "Build failed at step 3"
        ));
    }

    #[test]
    fn case_and_punctuation_do_not_matter() {
        assert!(texts_agree(
            "build failed, at step 3.",
            "Build failed at step 3"
        ));
    }

    #[test]
    fn a_few_misread_words_still_agree() {
        assert!(texts_agree(
            "the quick brown fox jumps over tl1e lazy dog today",
            "the quick brown fox jumps over the lazy dog today"
        ));
    }

    #[test]
    fn a_window_behind_the_capture_disagrees() {
        assert!(!texts_agree(
            "Quarterly budget review agenda",
            "int main(int argc, char** argv) { return 0; }"
        ));
    }

    #[test]
    fn exact_text_running_past_the_selection_disagrees() {
        assert!(!texts_agree(
            "error C2065 undeclared",
            "capturewidget.cpp(212): error C2065: 'foo': undeclared identifier in function bar \
             of class CaptureWidget"
        ));
    }

    #[test]
    fn a_few_words_are_too_few_to_corroborate() {
        assert!(!texts_agree("OK", "OK"));
        assert!(!texts_agree("Save 2024", "Save 2024"));
    }

    #[test]
    fn nothing_recognized_cannot_corroborate() {
        assert!(!texts_agree("", "anything at all"));
        assert!(!texts_agree("text", ""));
    }

    #[test]
    fn repeated_words_are_matched_once_each() {
        // Three "the" read once: one match, not three
        assert!(!texts_agree("the the the cat", "the dog sat down"));
    }
}
