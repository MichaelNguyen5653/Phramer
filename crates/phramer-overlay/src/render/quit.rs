//! The quit prompt: an opaque card in the overlay's panel style over a
//! darker scrim, with the key that answers each button in a chip beside its
//! name.

use tiny_skia::PixmapMut;

use super::text::{Font, Weight};
use super::{
    BG, FG, FG_10, FG_20, FG_60, Mapping, RADIUS_4, RADIUS_6, RADIUS_12, SPACE_2, SPACE_6, SPACE_8,
    SPACE_12, SPACE_16, TEXT_13, Theme, Ui, bgra_color, fill_rect, fill_rounded, solid,
};
use crate::icons::draw_icon;
use crate::state::{OverlayState, QuitControl};

/// `--text-16`: the title.
const TEXT_16: f32 = 16.0;
/// Black laid over the dimmed capture behind the card.
const SCRIM: f32 = 0.35;
const TITLE: &str = "Quit Capture";
const QUESTION: &str = "Are you sure you want to quit capture?";
const STOP_ASKING: &str = "Do not show this again";

impl Ui {
    pub(super) fn quit_prompt(
        &self,
        canvas: &mut PixmapMut<'_>,
        font: &mut Font<'_>,
        state: &OverlayState,
        mapping: &Mapping<'_>,
        theme: &Theme,
    ) {
        let Some(view) = state.quit_prompt() else {
            return;
        };
        fill_rect(
            canvas,
            0.0,
            0.0,
            self.width,
            self.height,
            &solid(bgra_color((0, 0, 0), SCRIM)),
        );
        let (x, y, w, h) = mapping.rect(view.layout.card);
        // Opaque: toolbar names behind a see-through card read as its text
        self.panel(canvas, x, y, w, h, self.px(RADIUS_12));
        fill_rounded(
            canvas,
            x,
            y,
            w,
            h,
            self.px(RADIUS_12),
            &solid(bgra_color(BG, 1.0)),
        );
        let fg = solid(bgra_color(FG, 1.0));
        let pad = self.px(SPACE_16);

        // Title and question
        let title = font.metrics(self.px(TEXT_16));
        let title_top = y + pad;
        font.draw(
            canvas,
            TITLE,
            x + pad,
            title_top + title.ascent,
            self.px(TEXT_16),
            Weight::Semibold,
            &fg,
        );
        let size = self.px(TEXT_13);
        let text = font.metrics(size);
        font.draw(
            canvas,
            QUESTION,
            x + pad,
            title_top + title.height + self.px(SPACE_8) + text.ascent,
            size,
            Weight::Regular,
            &solid(bgra_color(FG, FG_60)),
        );

        // Do not show this again
        let row = mapping.rect(view.layout.stop_asking);
        let check = self.px(SPACE_16);
        let (cx, cy) = (row.0, (row.1 + (row.3 - check) / 2.0).round());
        let corner = self.px(RADIUS_4);
        if view.stop_asking {
            fill_rounded(canvas, cx, cy, check, check, corner, &fg);
            let icon = self.px(SPACE_12);
            draw_icon(
                canvas,
                "check",
                (cx + (check - icon) / 2.0).round(),
                (cy + (check - icon) / 2.0).round(),
                icon,
                &solid(bgra_color(BG, 1.0)),
            );
        } else {
            let line = self.px(1.0);
            let edge = if view.hovered == Some(QuitControl::StopAsking) {
                1.0
            } else {
                FG_60
            };
            fill_rounded(
                canvas,
                cx,
                cy,
                check,
                check,
                corner,
                &solid(bgra_color(FG, edge)),
            );
            fill_rounded(
                canvas,
                cx + line,
                cy + line,
                check - 2.0 * line,
                check - 2.0 * line,
                (corner - line).max(0.0),
                &solid(bgra_color(BG, 1.0)),
            );
        }
        font.draw(
            canvas,
            STOP_ASKING,
            cx + check + self.px(SPACE_8),
            (row.1 + (row.3 - text.height) / 2.0).round() + text.ascent,
            size,
            Weight::Regular,
            &fg,
        );

        // Cancel (Esc) and Quit (Enter), Quit in the accent colour
        let hovered = |control| view.hovered == Some(control);
        let cancel_wash = if hovered(QuitControl::Cancel) {
            FG_20
        } else {
            FG_10
        };
        let quit_alpha = if hovered(QuitControl::Quit) {
            0.85
        } else {
            1.0
        };
        for (rect, name, key, wash) in [
            (
                view.layout.cancel,
                "Cancel",
                "Esc",
                bgra_color(FG, cancel_wash),
            ),
            (
                view.layout.quit,
                "Quit",
                "Enter",
                bgra_color(theme.accent, quit_alpha),
            ),
        ] {
            let (bx, by, bw, bh) = mapping.rect(rect);
            fill_rounded(canvas, bx, by, bw, bh, self.px(RADIUS_6), &solid(wash));
            let chip_pad = self.px(SPACE_6);
            let chip_h = text.height + 2.0 * self.px(SPACE_2);
            let name_w = font.measure(name, size, Weight::Medium).ceil();
            let chip_w = font.measure(key, size, Weight::Medium).ceil() + 2.0 * chip_pad;
            let gap = self.px(SPACE_8);
            let left = (bx + (bw - name_w - gap - chip_w) / 2.0).round();
            font.draw(
                canvas,
                name,
                left,
                (by + (bh - text.height) / 2.0).round() + text.ascent,
                size,
                Weight::Medium,
                &fg,
            );
            let chip_x = left + name_w + gap;
            let chip_y = (by + (bh - chip_h) / 2.0).round();
            fill_rounded(
                canvas,
                chip_x,
                chip_y,
                chip_w,
                chip_h,
                self.px(RADIUS_4),
                &solid(bgra_color(BG, FG_20)),
            );
            font.draw(
                canvas,
                key,
                chip_x + chip_pad,
                chip_y + self.px(SPACE_2) + text.ascent,
                size,
                Weight::Medium,
                &solid(bgra_color(FG, 0.8)),
            );
        }
    }
}
