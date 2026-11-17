//! Recognised lines into reading order and into text: 15.0.0's
//! `ocrlayout.cpp`, constants and rules unchanged.
//!
//! The engine reports fragments in its own order: side-by-side windows and
//! two-column text come back interleaved, and the whitespace inside a row
//! is lost. Both survive in the geometry, which is what this works from.

use crate::{OcrLine, Rect};

/// Two boxes share a visual row when they overlap vertically by more than
/// this fraction of the shorter one.
const ROW_OVERLAP_FRACTION: f64 = 0.5;
/// A gutter must be this many median glyph heights wide, and this fraction
/// of the total width, before it splits columns.
const MIN_GUTTER_IN_GLYPHS: f64 = 2.0;
const MIN_GUTTER_IN_WIDTHS: f64 = 0.04;
/// Splitting needs evidence of a layout rather than two short lines.
const MIN_LINES_TO_SPLIT: usize = 4;
const MIN_LINES_PER_COLUMN: usize = 2;
/// Wrapped-paragraph guards.
const MAX_HEIGHT_RATIO: f64 = 1.5;
const MIN_ROW_ADVANCE: f64 = 0.5;
const MAX_LINE_GAP: f64 = 0.6;

/// How the text is laid out (`ocrTextLayout`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OcrTextLayout {
    /// One line per visual row, indentation and column gaps turned back
    /// into spaces: code and terminal output paste back usably.
    #[default]
    Preserve,
    /// One trimmed, single-spaced line per visual row.
    Plain,
    /// Preserve, then merge the rows of a wrapped paragraph.
    Join,
}

impl OcrTextLayout {
    /// From the setting's value: 0, 1 or 2.
    pub fn from_setting(value: i64) -> Self {
        match value {
            1 => Self::Plain,
            2 => Self::Join,
            _ => Self::Preserve,
        }
    }
}

fn median(mut values: Vec<f64>) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    values.get(values.len() / 2).copied()
}

fn median_glyph_height(lines: &[OcrLine]) -> f64 {
    median(
        lines
            .iter()
            .map(|line| line.bounding_box.height)
            .filter(|height| *height > 0.0)
            .collect(),
    )
    .unwrap_or(0.0)
}

fn shares_row(a: &Rect, b: &Rect) -> bool {
    let overlap = a.bottom().min(b.bottom()) - a.y.max(b.y);
    let shorter = a.height.min(b.height);
    shorter > 0.0 && overlap > shorter * ROW_OVERLAP_FRACTION
}

/// The x positions of vertical gutters wide enough to be column breaks: a
/// span of x no line crosses. A line spanning the whole width (a heading
/// over both columns) suppresses every gutter, the conservative answer.
fn find_gutters(lines: &[OcrLine]) -> Vec<f64> {
    let mut spans: Vec<(f64, f64)> = lines
        .iter()
        .map(|line| line.bounding_box)
        .filter(|rect| rect.width > 0.0)
        .map(|rect| (rect.x, rect.right()))
        .collect();
    let left = spans
        .iter()
        .map(|span| span.0)
        .fold(f64::INFINITY, f64::min);
    let right = spans
        .iter()
        .map(|span| span.1)
        .fold(f64::NEG_INFINITY, f64::max);
    if spans.len() < MIN_LINES_TO_SPLIT || right <= left {
        return Vec::new();
    }
    let min_gutter = (median_glyph_height(lines) * MIN_GUTTER_IN_GLYPHS)
        .max((right - left) * MIN_GUTTER_IN_WIDTHS);
    if min_gutter <= 0.0 {
        return Vec::new();
    }
    spans.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    let mut gutters = Vec::new();
    let mut reach = spans[0].1;
    for span in &spans[1..] {
        if span.0 - reach > min_gutter {
            // Split in the middle of the empty span
            gutters.push((reach + span.0) / 2.0);
        }
        reach = reach.max(span.1);
    }
    gutters
}

/// One column band in order: rows top to bottom, left to right within a
/// row.
fn append_ordered_band(band: &[OcrLine], out: &mut Vec<OcrLine>) {
    let mut sorted = band.to_vec();
    sorted.sort_by(|a, b| a.bounding_box.y.total_cmp(&b.bounding_box.y));
    let mut start = 0;
    while start < sorted.len() {
        // Against the row's first line, not a growing union, which would let
        // one tall line swallow the column
        let mut end = start + 1;
        while end < sorted.len()
            && shares_row(&sorted[start].bounding_box, &sorted[end].bounding_box)
        {
            end += 1;
        }
        sorted[start..end].sort_by(|a, b| a.bounding_box.x.total_cmp(&b.bounding_box.x));
        out.extend_from_slice(&sorted[start..end]);
        start = end;
    }
}

/// The lines in the order a person reads them (`ocrOrderLines`): column
/// bands first, then visual rows, then left to right.
pub fn order_lines(lines: &[OcrLine]) -> Vec<OcrLine> {
    if lines.len() < 2 {
        return lines.to_vec();
    }
    let gutters = find_gutters(lines);
    let mut bands: Vec<Vec<OcrLine>> = vec![Vec::new(); gutters.len() + 1];
    for line in lines {
        // The left edge decides, so a line overhanging a gutter stays put
        let band = gutters
            .iter()
            .take_while(|gutter| line.bounding_box.x >= **gutter)
            .count();
        bands[band].push(line.clone());
    }
    // A band of one stray fragment means the gutter was an accident
    if bands.iter().any(|band| band.len() < MIN_LINES_PER_COLUMN) {
        bands = vec![lines.to_vec()];
    }
    let mut ordered = Vec::with_capacity(lines.len());
    for band in &bands {
        append_ordered_band(band, &mut ordered);
    }
    ordered
}

/// Whether `next` continues the paragraph `current` starts.
pub fn is_wrapped_continuation(current: &Rect, next: &Rect) -> bool {
    if current.height <= 0.0 || next.height <= 0.0 {
        return false;
    }
    // Different sizes are different blocks
    let min_height = current.height.min(next.height);
    let max_height = current.height.max(next.height);
    if max_height / min_height > MAX_HEIGHT_RATIO {
        return false;
    }
    // Must advance to a distinct row; fragments of one row are not a
    // paragraph
    if next.y - current.y < min_height * MIN_ROW_ADVANCE {
        return false;
    }
    // Normal leading is the same paragraph; extra space is a break
    let gap = next.y - current.bottom();
    gap < (current.height + next.height) / 2.0 * MAX_LINE_GAP
}

/// The width of a typical character, 0 when unknown: the median over
/// words (over lines without words), which an oversized heading cannot
/// skew.
pub fn estimate_char_width(lines: &[OcrLine]) -> f64 {
    let per_char = |text: &str, rect: &Rect| {
        let length = text.trim().chars().count();
        (length > 0 && rect.width > 0.0).then(|| rect.width / length as f64)
    };
    let mut widths = Vec::new();
    for line in lines {
        if line.words.is_empty() {
            widths.extend(per_char(&line.text, &line.bounding_box));
        } else {
            widths.extend(
                line.words
                    .iter()
                    .filter_map(|word| per_char(&word.text, &word.bounding_box)),
            );
        }
    }
    median(widths).unwrap_or(0.0)
}

struct Row {
    text: String,
    rect: Rect,
}

fn spaces_for(distance: f64, char_width: f64) -> usize {
    if char_width <= 0.0 || distance <= 0.0 {
        return 0;
    }
    (distance / char_width).round() as usize
}

/// One entry per visual row with the whitespace put back. Fragments of a
/// row arrive adjacent because [`order_lines`] grouped them.
fn build_rows(lines: &[OcrLine], layout: OcrTextLayout, char_width: f64) -> Vec<Row> {
    let preserve = layout != OcrTextLayout::Plain;
    let margin = lines
        .iter()
        .filter(|line| line.bounding_box.width > 0.0)
        .map(|line| line.bounding_box.x)
        .reduce(f64::min);

    let mut rows = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let mut end = index + 1;
        while end < lines.len() && shares_row(&lines[index].bounding_box, &lines[end].bounding_box)
        {
            end += 1;
        }
        let mut text = String::new();
        let mut rect: Option<Rect> = None;
        let mut previous_right = 0.0;
        for fragment in &lines[index..end] {
            let trimmed = fragment.text.trim();
            if trimmed.is_empty() {
                continue;
            }
            let fragment_box = &fragment.bounding_box;
            if rect.is_none() {
                if preserve && let Some(margin) = margin {
                    // Indentation from the leftmost thing on the page, which
                    // is what makes nested code line up
                    text.push_str(&" ".repeat(spaces_for(fragment_box.x - margin, char_width)));
                }
            } else if preserve {
                // The gap decides, never fewer than one space
                text.push_str(
                    &" ".repeat(spaces_for(fragment_box.x - previous_right, char_width).max(1)),
                );
            } else {
                text.push(' ');
            }
            text.push_str(trimmed);
            rect = Some(rect.map_or(*fragment_box, |rect| rect.united(fragment_box)));
            previous_right = fragment_box.right();
        }
        if let Some(rect) = rect
            && !text.trim().is_empty()
        {
            rows.push(Row { text, rect });
        }
        index = end;
    }
    rows
}

/// The text shown to the user (`ocrAssembleText`), from lines already in
/// reading order.
pub fn assemble_text(lines: &[OcrLine], layout: OcrTextLayout) -> String {
    let rows = build_rows(lines, layout, estimate_char_width(lines));
    let mut text = String::new();
    for (index, row) in rows.iter().enumerate() {
        if index > 0 {
            let continuation = layout == OcrTextLayout::Join
                && is_wrapped_continuation(&rows[index - 1].rect, &row.rect);
            if continuation {
                // One paragraph: the second row's indentation would be a gap
                text.push(' ');
                text.push_str(row.text.trim());
                continue;
            }
            text.push('\n');
        }
        text.push_str(&row.text);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OcrWord;

    /// A line of single-word fragments: each `(text, x)` is a word of
    /// `char_width` per character at `y`, `height` tall.
    fn line(words: &[(&str, f64)], y: f64, height: f64) -> OcrLine {
        const CHAR_WIDTH: f64 = 10.0;
        OcrLine::from_words(
            words
                .iter()
                .map(|(text, x)| OcrWord {
                    text: (*text).to_owned(),
                    bounding_box: Rect::new(
                        *x,
                        y,
                        text.chars().count() as f64 * CHAR_WIDTH,
                        height,
                    ),
                })
                .collect(),
        )
    }

    fn texts(lines: &[OcrLine]) -> Vec<&str> {
        lines.iter().map(|line| line.text.as_str()).collect()
    }

    #[test]
    fn rows_are_read_top_to_bottom_then_left_to_right() {
        let lines = vec![
            line(&[("second", 0.0)], 40.0, 20.0),
            line(&[("right", 200.0)], 2.0, 20.0),
            line(&[("left", 0.0)], 0.0, 20.0),
        ];
        assert_eq!(texts(&order_lines(&lines)), ["left", "right", "second"]);
    }

    #[test]
    fn two_columns_are_read_one_after_the_other() {
        let mut lines = Vec::new();
        for row in 0..3 {
            let y = f64::from(row) * 30.0;
            lines.push(line(&[(&format!("L{row}"), 0.0)], y, 20.0));
            lines.push(line(&[(&format!("R{row}"), 400.0)], y, 20.0));
        }
        assert_eq!(
            texts(&order_lines(&lines)),
            ["L0", "L1", "L2", "R0", "R1", "R2"]
        );
    }

    #[test]
    fn a_heading_across_both_columns_keeps_them_as_rows() {
        let mut lines = vec![line(&[("a wide heading over everything", 0.0)], 0.0, 20.0)];
        lines[0].bounding_box.width = 500.0;
        for row in 1..4 {
            let y = f64::from(row) * 30.0;
            lines.push(line(&[(&format!("L{row}"), 0.0)], y, 20.0));
            lines.push(line(&[(&format!("R{row}"), 400.0)], y, 20.0));
        }
        let ordered = order_lines(&lines);
        assert_eq!(texts(&ordered)[1..3], ["L1", "R1"]);
    }

    #[test]
    fn a_lone_fragment_is_no_column() {
        let lines = vec![
            line(&[("one", 0.0)], 0.0, 20.0),
            line(&[("two", 0.0)], 30.0, 20.0),
            line(&[("three", 0.0)], 60.0, 20.0),
            line(&[("stray", 400.0)], 30.0, 20.0),
        ];
        assert_eq!(
            texts(&order_lines(&lines)),
            ["one", "two", "stray", "three"]
        );
    }

    #[test]
    fn preserve_puts_indentation_and_gaps_back() {
        let lines = vec![
            line(&[("fn", 0.0), ("main()", 30.0)], 0.0, 20.0),
            line(&[("body", 40.0)], 25.0, 20.0),
            // A separate fragment on the same row, 3 characters to the right
            line(&[("//", 120.0)], 25.0, 20.0),
        ];
        assert_eq!(
            assemble_text(&lines, OcrTextLayout::Preserve),
            "fn main()\n    body    //"
        );
        assert_eq!(
            assemble_text(&lines, OcrTextLayout::Plain),
            "fn main()\nbody //"
        );
    }

    #[test]
    fn join_merges_a_wrapped_paragraph_and_keeps_breaks() {
        let lines = vec![
            line(&[("one", 0.0)], 0.0, 20.0),
            line(&[("two", 20.0)], 25.0, 20.0),
            // A blank line's worth of space: a new paragraph
            line(&[("three", 0.0)], 80.0, 20.0),
            // Twice the size: a different block
            line(&[("big", 0.0)], 105.0, 40.0),
        ];
        assert_eq!(
            assemble_text(&lines, OcrTextLayout::Join),
            "one two\nthree\nbig"
        );
        assert_eq!(
            assemble_text(&lines, OcrTextLayout::Preserve),
            "one\n  two\nthree\nbig"
        );
    }

    #[test]
    fn continuation_rules() {
        let current = Rect::new(0.0, 0.0, 100.0, 20.0);
        assert!(is_wrapped_continuation(
            &current,
            &Rect::new(0.0, 25.0, 100.0, 20.0)
        ));
        // The same row
        assert!(!is_wrapped_continuation(
            &current,
            &Rect::new(0.0, 5.0, 100.0, 20.0)
        ));
        // Too far below
        assert!(!is_wrapped_continuation(
            &current,
            &Rect::new(0.0, 40.0, 100.0, 20.0)
        ));
        // Too different in size
        assert!(!is_wrapped_continuation(
            &current,
            &Rect::new(0.0, 25.0, 100.0, 31.0)
        ));
        assert!(!is_wrapped_continuation(
            &current,
            &Rect::new(0.0, 25.0, 100.0, 0.0)
        ));
    }

    #[test]
    fn the_character_width_is_a_median() {
        let mut lines = vec![
            line(&[("abcd", 0.0)], 0.0, 20.0),
            line(&[("ef", 0.0)], 30.0, 20.0),
        ];
        // A heading at triple width does not move it
        lines.push(OcrLine {
            text: "HEAD".into(),
            bounding_box: Rect::new(0.0, 60.0, 120.0, 40.0),
            words: Vec::new(),
        });
        assert_eq!(estimate_char_width(&lines), 10.0);
        assert_eq!(estimate_char_width(&[]), 0.0);
    }

    #[test]
    fn empty_input_is_empty_text() {
        assert_eq!(assemble_text(&[], OcrTextLayout::Preserve), "");
        assert!(order_lines(&[]).is_empty());
    }

    #[test]
    fn the_setting_picks_the_layout() {
        assert_eq!(OcrTextLayout::from_setting(0), OcrTextLayout::Preserve);
        assert_eq!(OcrTextLayout::from_setting(1), OcrTextLayout::Plain);
        assert_eq!(OcrTextLayout::from_setting(2), OcrTextLayout::Join);
        assert_eq!(OcrTextLayout::from_setting(7), OcrTextLayout::Preserve);
    }
}
