//! The side panel, its Tool Settings tab and the Display grid dots.
//!
//! The panel is drawn into a picture of its own, kept while nothing on it
//! changes, and laid over the overlay at its opacity: its fade and slide
//! (opacity and a sideways shift, `--dur-panel` with `--ease-out`) change
//! nothing but how it is composited, and a pointer move elsewhere costs
//! only that.

use phramer_capture::{LogicalPoint, LogicalRect};
use tiny_skia::{Paint, Pixmap, PixmapMut};

use super::text::{Font, Weight};
use super::tools::{ease, hue_disc, ink};
use super::{
    BG, EASE_OUT, FG, FG_10, FG_20, FG_60, Mapping, PixelRect, RADIUS_4, RADIUS_6, RADIUS_12,
    SPACE_2, SPACE_4, SPACE_6, SPACE_8, SPACE_12, SPACE_16, TEXT_13, Theme, Ui, bgra_color,
    fill_circle, fill_rect, fill_rounded, solid,
};
use crate::icons::draw_icon;
use crate::state::tools::MAX_TOOL_SIZE;
use crate::state::{OverlayState, PanelControl, SidePanelView, THUMB, disc_position};

/// `--fg-5`: a hovered list row.
const FG_5: f32 = 0.05;
/// The grid's dots: `uiColor` at alpha 100 (`CaptureWidget::paintEvent`).
const GRID_ALPHA: u32 = 100;
const TAB_TEXT: &str = "Tool Settings";

/// The side panel's last picture and what it showed.
#[derive(Default)]
pub(crate) struct PanelCache {
    key: Option<(SidePanelView, u32, (u8, u8, u8))>,
    sheet: Option<Pixmap>,
}

/// Lays premultiplied `sheet` over the canvas at (x, y), at `opacity`.
fn composite(canvas: &mut PixmapMut<'_>, sheet: &Pixmap, x: i32, y: i32, opacity: f32) {
    let alpha = (opacity.clamp(0.0, 1.0) * 255.0).round() as u32;
    let (width, height) = (canvas.width() as i32, canvas.height() as i32);
    let (sheet_w, sheet_h) = (sheet.width() as i32, sheet.height() as i32);
    let source = sheet.data();
    let data = canvas.data_mut();
    let scale = |value: u8, by: u32| (u32::from(value) * by + 127) / 255;
    for row in y.max(0)..(y + sheet_h).min(height) {
        for column in x.max(0)..(x + sheet_w).min(width) {
            let from = ((row - y) * sheet_w + (column - x)) as usize * 4;
            let to = (row * width + column) as usize * 4;
            let (Some(pixel), Some(target)) =
                (source.get(from..from + 4), data.get_mut(to..to + 4))
            else {
                continue;
            };
            let cover = scale(pixel[3], alpha);
            if cover == 0 {
                continue;
            }
            for channel in 0..4 {
                let over = scale(pixel[channel], alpha);
                let under = scale(target[channel], 255 - cover);
                target[channel] = (over + under).min(255) as u8;
            }
        }
    }
}

/// The panel's own picture: canvas pixels of the panel, from its corner.
struct Sheet<'a> {
    ui: &'a Ui,
    mapping: &'a Mapping<'a>,
    /// The panel's corner on the canvas, less the margin for its ring.
    origin: (f32, f32),
}

impl Sheet<'_> {
    /// A logical rectangle on the sheet: (x, y, w, h).
    fn rect(&self, rect: LogicalRect) -> (f32, f32, f32, f32) {
        let (x, y, w, h) = self.mapping.rect(rect);
        (x - self.origin.0, y - self.origin.1, w, h)
    }
}

impl Ui {
    /// The dots of Display grid, inside the selection.
    pub(super) fn grid(
        &self,
        canvas: &mut PixmapMut<'_>,
        state: &OverlayState,
        mapping: &Mapping<'_>,
        selection: Option<PixelRect>,
        theme: &Theme,
    ) {
        let (Some(size), Some(area), Some(logical)) = (state.grid(), selection, state.selection())
        else {
            return;
        };
        let size = i32::try_from(size).unwrap_or(i32::MAX).max(1);
        let dot = self.px(1.0) as u32;
        let (width, height) = (canvas.width(), canvas.height());
        let data = canvas.data_mut();
        let keep = 255 - GRID_ALPHA;
        let (r, g, b) = theme.accent;
        // The first multiple of the spacing at or after `from`
        let first = |from: i32| {
            from.div_euclid(size) * size + if from.rem_euclid(size) == 0 { 0 } else { size }
        };
        let mut gy = first(logical.y.max(0));
        while gy < logical.bottom() {
            let mut gx = first(logical.x.max(0));
            while gx < logical.right() {
                let (x, y) = mapping.point(LogicalPoint::new(gx, gy));
                let (x, y) = (x.max(0.0) as u32, y.max(0.0) as u32);
                for row in y..(y + dot).min(area.bottom).min(height) {
                    for column in x..(x + dot).min(area.right).min(width) {
                        let at = (row as usize * width as usize + column as usize) * 4;
                        if let Some(pixel) = data.get_mut(at..at + 4) {
                            for (channel, value) in [(0, b), (1, g), (2, r)] {
                                let mixed = u32::from(pixel[channel]) * keep
                                    + u32::from(value) * GRID_ALPHA
                                    + 127;
                                pixel[channel] = (mixed / 255) as u8;
                            }
                        }
                    }
                }
                gx += size;
            }
            gy += size;
        }
    }

    /// The Tool Settings tab on the left edge, its words reading upwards.
    pub(super) fn side_panel_tab(
        &self,
        canvas: &mut PixmapMut<'_>,
        font: &mut Font<'_>,
        state: &OverlayState,
        mapping: &Mapping<'_>,
    ) {
        let Some(tab) = state.side_panel_tab() else {
            return;
        };
        let (x, y, w, h) = mapping.rect(tab);
        let radius = self.px(RADIUS_6);
        // The left corners sit off the monitor
        self.panel(canvas, x - radius, y, w + radius, h, radius);
        let size = self.px(TEXT_13);
        let metrics = font.metrics(size);
        let length = font.measure(TAB_TEXT, size, Weight::Medium);
        font.draw_vertical(
            canvas,
            TAB_TEXT,
            (x + w / 2.0 + metrics.ascent - metrics.height / 2.0).round(),
            (y + h / 2.0 + length / 2.0).round(),
            size,
            Weight::Medium,
            &solid(bgra_color(FG, 1.0)),
        );
    }

    /// The side panel, faded and shifted by how far it has come in.
    pub(super) fn side_panel(
        &self,
        canvas: &mut PixmapMut<'_>,
        font: &mut Font<'_>,
        state: &OverlayState,
        mapping: &Mapping<'_>,
        theme: &Theme,
        cache: &mut PanelCache,
    ) {
        let Some(view) = state.side_panel() else {
            return;
        };
        let eased = ease(EASE_OUT, view.progress);
        let opacity = if view.opening { eased } else { 1.0 - eased };
        if opacity <= 0.0 {
            return;
        }
        let (x, y, w, h) = mapping.rect(view.layout.panel);
        let margin = self.px(SPACE_2);
        let origin = ((x - margin).round(), (y - margin).round());
        let key = (
            SidePanelView {
                progress: 1.0,
                opening: true,
                ..view.clone()
            },
            self.scale.to_bits(),
            theme.accent,
        );
        if cache.key.as_ref() != Some(&key) || cache.sheet.is_none() {
            cache.sheet = Pixmap::new(
                (w + 2.0 * margin).ceil() as u32,
                (h + 2.0 * margin).ceil() as u32,
            );
            if let Some(sheet) = cache.sheet.as_mut() {
                let on = Sheet {
                    ui: self,
                    mapping,
                    origin,
                };
                on.draw(&mut sheet.as_mut(), font, &view, theme);
            }
            cache.key = Some(key);
        }
        let Some(sheet) = cache.sheet.as_ref() else {
            return;
        };
        let shift = -(1.0 - opacity) * self.px(SPACE_16);
        composite(
            canvas,
            sheet,
            (origin.0 + shift).round() as i32,
            origin.1 as i32,
            opacity,
        );
    }
}

impl Sheet<'_> {
    fn draw(
        &self,
        canvas: &mut PixmapMut<'_>,
        font: &mut Font<'_>,
        view: &SidePanelView,
        theme: &Theme,
    ) {
        let ui = self.ui;
        let layout = &view.layout;
        let (x, y, w, h) = self.rect(layout.panel);
        ui.panel(canvas, x, y, w, h, ui.px(RADIUS_12));
        let size = ui.px(TEXT_13);
        let metrics = font.metrics(size);
        let fg = solid(bgra_color(FG, 1.0));
        let secondary = solid(bgra_color(FG, FG_60));
        // A line of text centred in a row
        let baseline = |row: (f32, f32, f32, f32)| {
            (row.1 + (row.3 - metrics.height) / 2.0).round() + metrics.ascent
        };
        let tint = |control: PanelControl| {
            if view.held == Some(control) {
                FG_20
            } else if view.hovered == Some(control) {
                FG_10 + FG_5
            } else {
                FG_10
            }
        };

        // Active tool size
        let row = self.rect(layout.size_label);
        font.draw(
            canvas,
            "Active tool size",
            row.0,
            baseline(row),
            size,
            Weight::Regular,
            &secondary,
        );
        let value = view.size.to_string();
        let value_width = font.measure(&value, size, Weight::Semibold);
        font.draw(
            canvas,
            &value,
            (row.0 + row.2 - value_width).round(),
            baseline(row),
            size,
            Weight::Semibold,
            &fg,
        );
        self.slider(canvas, view);

        // Active color: the label, the hex field with its swatch
        let row = self.rect(layout.color_label);
        font.draw(
            canvas,
            "Active color",
            row.0,
            baseline(row),
            size,
            Weight::Regular,
            &secondary,
        );
        let field = self.rect(layout.hex);
        let radius = ui.px(RADIUS_6);
        if view.hex_focused {
            // `--focus-ring`: 2 px of the accent
            let ring = ui.px(SPACE_2);
            fill_rounded(
                canvas,
                field.0 - ring,
                field.1 - ring,
                field.2 + 2.0 * ring,
                field.3 + 2.0 * ring,
                radius + ring,
                &solid(bgra_color(theme.accent, 1.0)),
            );
            fill_rounded(
                canvas,
                field.0,
                field.1,
                field.2,
                field.3,
                radius,
                &solid(bgra_color(BG, 1.0)),
            );
        }
        fill_rounded(
            canvas,
            field.0,
            field.1,
            field.2,
            field.3,
            radius,
            &solid(bgra_color(FG, tint(PanelControl::Hex))),
        );
        let swatch = ui.px(SPACE_6);
        let (sx, sy) = (field.0 + ui.px(SPACE_12), field.1 + field.3 / 2.0);
        fill_circle(
            canvas,
            sx,
            sy,
            swatch + ui.px(1.0),
            &solid(bgra_color(FG, FG_20)),
        );
        fill_circle(canvas, sx, sy, swatch, &solid(ink(view.color, 1.0)));
        let text_x = (sx + swatch + ui.px(SPACE_8)).round();
        font.draw(
            canvas,
            &view.hex,
            text_x,
            baseline(field),
            size,
            Weight::Regular,
            &fg,
        );
        if view.hex_focused {
            let caret = text_x + font.measure(&view.hex, size, Weight::Regular).ceil();
            fill_rect(
                canvas,
                caret,
                (field.1 + (field.3 - metrics.height) / 2.0).round(),
                ui.px(1.0),
                metrics.height,
                &fg,
            );
        }

        self.disc(canvas, view);

        // Grab Color
        self.button(
            canvas,
            font,
            layout.grab,
            Some("pipette"),
            "Grab Color",
            tint(PanelControl::Grab),
        );

        // Display grid, and its spacing
        let row = self.rect(layout.grid);
        let check = ui.px(SPACE_16);
        let (cx, cy) = (row.0, (row.1 + (row.3 - check) / 2.0).round());
        let corner = ui.px(RADIUS_4);
        if view.grid {
            fill_rounded(canvas, cx, cy, check, check, corner, &fg);
            let icon = ui.px(SPACE_12);
            draw_icon(
                canvas,
                "check",
                (cx + (check - icon) / 2.0).round(),
                (cy + (check - icon) / 2.0).round(),
                icon,
                &solid(bgra_color(BG, 1.0)),
            );
        } else {
            let line = ui.px(1.0);
            fill_rounded(
                canvas,
                cx,
                cy,
                check,
                check,
                corner,
                &solid(bgra_color(FG, FG_60)),
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
            if view.hovered == Some(PanelControl::Grid) {
                fill_rounded(
                    canvas,
                    cx,
                    cy,
                    check,
                    check,
                    corner,
                    &solid(bgra_color(FG, FG_10)),
                );
            }
        }
        font.draw(
            canvas,
            "Display grid",
            (cx + check + ui.px(SPACE_8)).round(),
            baseline(row),
            size,
            Weight::Regular,
            &fg,
        );
        let enabled = |on: bool| if on { 1.0 } else { FG_60 };
        for (rect, icon, control) in [
            (layout.grid_smaller, "minus", PanelControl::GridSmaller),
            (layout.grid_larger, "plus", PanelControl::GridLarger),
        ] {
            self.icon_button(canvas, rect, icon, tint(control), enabled(view.grid));
        }
        let value_row = self.rect(layout.grid_value);
        let value = view.grid_size.to_string();
        let value_width = font.measure(&value, size, Weight::Medium);
        font.draw(
            canvas,
            &value,
            (value_row.0 + (value_row.2 - value_width) / 2.0).round(),
            baseline(value_row),
            size,
            Weight::Medium,
            &solid(bgra_color(FG, enabled(view.grid))),
        );

        // The object list
        let (_, separator, _, _) =
            self.rect(LogicalRect::new(layout.panel.x, layout.separator_y, 1, 1));
        let inner = self.rect(layout.size_label);
        fill_rect(
            canvas,
            inner.0,
            separator,
            inner.2,
            ui.px(1.0),
            &solid(bgra_color(FG, FG_10)),
        );
        self.icon_button(
            canvas,
            layout.delete,
            "trash-2",
            if view.can_delete {
                tint(PanelControl::Delete)
            } else {
                FG_5
            },
            enabled(view.can_delete),
        );
        let icon = ui.px(SPACE_16) - ui.px(SPACE_2);
        for (index, (rect, entry)) in layout.rows.iter().zip(&view.rows).enumerate() {
            let row = self.rect(*rect);
            let hovered = view.hovered == Some(PanelControl::Row(view.first_row + index));
            let wash = if entry.selected {
                FG_10
            } else if hovered {
                FG_5
            } else {
                0.0
            };
            if wash > 0.0 {
                fill_rounded(
                    canvas,
                    row.0,
                    row.1,
                    row.2,
                    row.3,
                    ui.px(RADIUS_4),
                    &solid(bgra_color(FG, wash)),
                );
            }
            let text_x = row.0 + ui.px(SPACE_8);
            if let Some(name) = entry.icon {
                draw_icon(
                    canvas,
                    name,
                    text_x.round(),
                    (row.1 + (row.3 - icon) / 2.0).round(),
                    icon,
                    &secondary,
                );
            }
            font.draw(
                canvas,
                &entry.label,
                (text_x + icon + ui.px(SPACE_8)).round(),
                baseline(row),
                size,
                Weight::Regular,
                if entry.icon.is_some() {
                    &fg
                } else {
                    &secondary
                },
            );
        }

        self.button(
            canvas,
            font,
            layout.close,
            None,
            "Close",
            tint(PanelControl::Close),
        );
    }

    /// Active tool size: a track filled up to the thumb.
    fn slider(&self, canvas: &mut PixmapMut<'_>, view: &SidePanelView) {
        let ui = self.ui;
        let (x, y, w, h) = self.rect(view.layout.slider);
        let thumb = ui.px(THUMB as f32 - 1.0);
        let reach = THUMB as f32 * ui.scale;
        let (left, right) = (x + reach, x + w - reach);
        let fraction = view.size.saturating_sub(1) as f32 / (MAX_TOOL_SIZE - 1) as f32;
        let at = left + (right - left) * fraction;
        let track = ui.px(SPACE_4);
        let cy = y + h / 2.0;
        fill_rounded(
            canvas,
            left,
            (cy - track / 2.0).round(),
            right - left,
            track,
            track / 2.0,
            &solid(bgra_color(FG, FG_20)),
        );
        fill_rounded(
            canvas,
            left,
            (cy - track / 2.0).round(),
            (at - left).max(track),
            track,
            track / 2.0,
            &solid(bgra_color(FG, FG_60)),
        );
        let ring = if view.held == Some(PanelControl::Slider)
            || view.hovered == Some(PanelControl::Slider)
        {
            FG_20
        } else {
            FG_10
        };
        fill_circle(
            canvas,
            at,
            cy,
            thumb + ui.px(SPACE_2),
            &solid(bgra_color(FG, ring)),
        );
        fill_circle(canvas, at, cy, thumb, &solid(bgra_color(FG, 1.0)));
    }

    /// The hue disc with a ring where the colour sits.
    fn disc(&self, canvas: &mut PixmapMut<'_>, view: &SidePanelView) {
        let ui = self.ui;
        let (x, y, w, _) = self.rect(view.layout.disc);
        let radius = w / 2.0;
        let (cx, cy) = (x + radius, y + radius);
        fill_circle(
            canvas,
            cx,
            cy,
            radius + ui.px(1.0),
            &solid(bgra_color(FG, FG_20)),
        );
        hue_disc(canvas, cx, cy, radius);
        let (hue, saturation) = disc_position(view.color);
        let angle = hue.to_radians();
        let mx = cx + (saturation * angle.cos()) as f32 * radius;
        let my = cy - (saturation * angle.sin()) as f32 * radius;
        let marker = ui.px(SPACE_6);
        fill_circle(
            canvas,
            mx,
            my,
            marker + ui.px(1.0),
            &solid(bgra_color((0, 0, 0), 0.35)),
        );
        fill_circle(
            canvas,
            mx,
            my,
            marker,
            &solid(bgra_color((255, 255, 255), 1.0)),
        );
        fill_circle(
            canvas,
            mx,
            my,
            marker - ui.px(SPACE_2),
            &solid(ink(view.color, 1.0)),
        );
    }

    /// A full-width button: an optional icon and a label, centred.
    fn button(
        &self,
        canvas: &mut PixmapMut<'_>,
        font: &mut Font<'_>,
        rect: LogicalRect,
        icon: Option<&str>,
        label: &str,
        tint: f32,
    ) {
        let ui = self.ui;
        let (x, y, w, h) = self.rect(rect);
        fill_rounded(
            canvas,
            x,
            y,
            w,
            h,
            ui.px(RADIUS_6),
            &solid(bgra_color(FG, tint)),
        );
        let size = ui.px(TEXT_13);
        let metrics = font.metrics(size);
        let icon_size = ui.px(SPACE_16);
        let gap = ui.px(SPACE_8);
        let text = font.measure(label, size, Weight::Medium).ceil();
        let content = text + icon.map_or(0.0, |_| icon_size + gap);
        let mut left = (x + (w - content) / 2.0).round();
        let paint: Paint<'_> = solid(bgra_color(FG, 1.0));
        if let Some(name) = icon {
            draw_icon(
                canvas,
                name,
                left,
                (y + (h - icon_size) / 2.0).round(),
                icon_size,
                &paint,
            );
            left += icon_size + gap;
        }
        font.draw(
            canvas,
            label,
            left,
            (y + (h - metrics.height) / 2.0).round() + metrics.ascent,
            size,
            Weight::Medium,
            &paint,
        );
    }

    /// A square button with an icon; `alpha` dims a disabled one.
    fn icon_button(
        &self,
        canvas: &mut PixmapMut<'_>,
        rect: LogicalRect,
        icon: &str,
        tint: f32,
        alpha: f32,
    ) {
        let ui = self.ui;
        let (x, y, w, h) = self.rect(rect);
        fill_rounded(
            canvas,
            x,
            y,
            w,
            h,
            ui.px(RADIUS_6),
            &solid(bgra_color(FG, tint)),
        );
        let size = ui.px(SPACE_16) - ui.px(SPACE_2);
        draw_icon(
            canvas,
            icon,
            (x + (w - size) / 2.0).round(),
            (y + (h - size) / 2.0).round(),
            size,
            &solid(bgra_color(FG, alpha)),
        );
    }
}
