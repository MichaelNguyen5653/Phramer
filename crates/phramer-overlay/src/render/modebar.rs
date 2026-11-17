//! The Screenshot/Video mode bar and the names under the toolbar buttons:
//! dark panels with a hairline, as the rest of the overlay.

use tiny_skia::PixmapMut;

use super::text::{Font, Weight};
use super::{
    FG, FG_10, FG_20, FG_60, Mapping, RADIUS_4, RADIUS_6, RADIUS_12, SPACE_2, SPACE_6, SPACE_8,
    TEXT_13, Ui, bgra_color, fill_rounded, solid,
};
use crate::state::{CaptureMode, OverlayState};

/// `--text-11`: the names under the buttons.
const TEXT_11: f32 = 11.0;

impl Ui {
    /// The mode bar: two segments, each its name and a key chip; the
    /// current mode (Screenshot) has the selected wash.
    pub(super) fn mode_bar(
        &self,
        canvas: &mut PixmapMut<'_>,
        font: &mut Font<'_>,
        state: &OverlayState,
        mapping: &Mapping<'_>,
    ) {
        let Some(view) = state.mode_bar() else {
            return;
        };
        let (x, y, w, h) = mapping.rect(view.layout.bar);
        self.panel(canvas, x, y, w, h, self.px(RADIUS_12));
        let size = self.px(TEXT_13);
        let metrics = font.metrics(size);
        let chip_pad = self.px(SPACE_6);
        let chip_h = metrics.height + 2.0 * self.px(SPACE_2);
        for (rect, mode, name, key) in [
            (
                view.layout.screenshot,
                CaptureMode::Screenshot,
                "Screenshot",
                &view.screenshot_key,
            ),
            (
                view.layout.video,
                CaptureMode::Video,
                "Video",
                &view.video_key,
            ),
        ] {
            let (sx, sy, sw, sh) = mapping.rect(rect);
            let wash = if mode == CaptureMode::Screenshot {
                Some(FG_20)
            } else if view.hovered == Some(mode) {
                Some(FG_10)
            } else {
                None
            };
            if let Some(alpha) = wash {
                fill_rounded(
                    canvas,
                    sx,
                    sy,
                    sw,
                    sh,
                    self.px(RADIUS_6),
                    &solid(bgra_color(FG, alpha)),
                );
            }
            let name_w = font.measure(name, size, Weight::Medium).ceil();
            let chip_w = key.as_ref().map_or(0.0, |key| {
                font.measure(key, size, Weight::Semibold).ceil() + 2.0 * chip_pad
            });
            let gap = if key.is_some() { self.px(SPACE_8) } else { 0.0 };
            let left = (sx + (sw - name_w - gap - chip_w) / 2.0).round();
            let baseline = (sy + (sh - metrics.height) / 2.0).round() + metrics.ascent;
            font.draw(
                canvas,
                name,
                left,
                baseline,
                size,
                Weight::Medium,
                &solid(bgra_color(FG, 1.0)),
            );
            if let Some(key) = key {
                let chip_x = left + name_w + gap;
                let chip_y = (sy + (sh - chip_h) / 2.0).round();
                fill_rounded(
                    canvas,
                    chip_x,
                    chip_y,
                    chip_w,
                    chip_h,
                    self.px(RADIUS_4),
                    &solid(bgra_color(FG, FG_10)),
                );
                font.draw(
                    canvas,
                    key,
                    chip_x + chip_pad,
                    chip_y + self.px(SPACE_2) + metrics.ascent,
                    size,
                    Weight::Semibold,
                    &solid(bgra_color(FG, FG_60)),
                );
            }
        }
    }

    /// Each toolbar button's name under it (`showToolLabels`).
    pub(super) fn tool_labels(
        &self,
        canvas: &mut PixmapMut<'_>,
        font: &mut Font<'_>,
        state: &OverlayState,
        mapping: &Mapping<'_>,
    ) {
        let size = self.px(TEXT_11);
        let metrics = font.metrics(size);
        for (rect, text) in state.toolbar_labels() {
            let (x, y, w, h) = mapping.rect(rect);
            self.panel(canvas, x, y, w, h, self.px(RADIUS_6));
            let text_w = font.measure(&text, size, Weight::Medium).ceil();
            font.draw(
                canvas,
                &text,
                (x + (w - text_w) / 2.0).round(),
                (y + (h - metrics.height) / 2.0).round() + metrics.ascent,
                size,
                Weight::Medium,
                &solid(bgra_color(FG, 1.0)),
            );
        }
    }
}
