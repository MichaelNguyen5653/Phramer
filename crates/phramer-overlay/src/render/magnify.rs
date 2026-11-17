//! The magnifier and colour grab's preview: screenshot pixels drawn as
//! whole blocks of canvas pixels, straight into the canvas and only inside
//! their own square, so a pointer move costs little more than without them.

use phramer_annotate::Color as InkColor;
use tiny_skia::{Paint, PathBuilder, PixmapMut, Stroke, Transform};

use super::text::{Font, Weight};
use super::tools::ink;
use super::{
    FG, FG_20, FrameView, RADIUS_6, SPACE_2, SPACE_4, SPACE_6, SPACE_8, SPACE_16, TEXT_13, Theme,
    Ui, bgra_color, fill_circle, fill_rect, fill_rounded, solid,
};
use crate::state::{GRAB_KEYS, MagnifierShape, OverlayState, Zoom};

/// The crosshair: `uiColor` at alpha 130 (`MagnifierWidget`).
const CROSSHAIR_ALPHA: u32 = 130;

/// Draws `zoom`'s pixels. `round` keeps them to the inscribed circle, its
/// edge anti-aliased; `crosshair` tints the pointer's row and column but
/// not its pixel.
fn draw_zoom(
    canvas: &mut PixmapMut<'_>,
    frame: FrameView<'_>,
    zoom: &Zoom,
    round: bool,
    crosshair: Option<(u8, u8, u8)>,
) {
    let (width, height) = (canvas.width() as i32, canvas.height() as i32);
    let side = zoom.side();
    let radius = side as f32 / 2.0;
    let (cx, cy) = (zoom.x as f32 + radius, zoom.y as f32 + radius);
    let data = canvas.data_mut();
    let source = frame.bgra();
    let (frame_w, frame_h) = (frame.width() as i32, frame.height() as i32);
    let keep = 255 - CROSSHAIR_ALPHA;
    for y in zoom.y.max(0)..(zoom.y + side).min(height) {
        let row = (y - zoom.y) / zoom.cell;
        let sy = zoom.source.y + row;
        for x in zoom.x.max(0)..(zoom.x + side).min(width) {
            let coverage = if round {
                let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                (radius + 0.5 - dx.hypot(dy)).clamp(0.0, 1.0)
            } else {
                1.0
            };
            if coverage <= 0.0 {
                continue;
            }
            let column = (x - zoom.x) / zoom.cell;
            let sx = zoom.source.x + column;
            let mut pixel = if (0..frame_w).contains(&sx) && (0..frame_h).contains(&sy) {
                let at = (sy as usize * frame_w as usize + sx as usize) * 4;
                [source[at], source[at + 1], source[at + 2]]
            } else {
                [0, 0, 0]
            };
            if let Some((r, g, b)) = crosshair
                && (column == zoom.center.0) != (row == zoom.center.1)
            {
                for (channel, value) in pixel.iter_mut().zip([b, g, r]) {
                    *channel =
                        ((u32::from(*channel) * keep + u32::from(value) * CROSSHAIR_ALPHA + 127)
                            / 255) as u8;
                }
            }
            let at = (y as usize * width as usize + x as usize) * 4;
            let Some(target) = data.get_mut(at..at + 4) else {
                continue;
            };
            for (channel, value) in pixel.into_iter().enumerate() {
                let under = f32::from(target[channel]);
                target[channel] = (under + (f32::from(value) - under) * coverage).round() as u8;
            }
            target[3] = u8::MAX;
        }
    }
}

impl Ui {
    /// The magnifier beside the pointer.
    pub(super) fn magnifier(
        &self,
        canvas: &mut PixmapMut<'_>,
        state: &OverlayState,
        frame: FrameView<'_>,
        theme: &Theme,
    ) {
        let Some(view) = state.magnifier() else {
            return;
        };
        let zoom = view.zoom;
        let round = view.shape == MagnifierShape::Round;
        let side = zoom.side() as f32;
        let (x, y) = (zoom.x as f32, zoom.y as f32);
        let line = self.px(1.0);
        let shadow = solid(bgra_color((0, 0, 0), 0.35));
        let border = self.px(SPACE_2);
        let accent = solid(bgra_color(theme.accent, 1.0));
        if !round {
            fill_rect(
                canvas,
                x - line,
                y - line,
                side + 2.0 * line,
                side + 2.0 * line,
                &shadow,
            );
        } else {
            let radius = side / 2.0;
            fill_circle(canvas, x + radius, y + radius, radius + line, &shadow);
        }
        draw_zoom(canvas, frame, &zoom, round, Some(theme.accent));
        if round {
            let radius = side / 2.0;
            if let Some(path) =
                PathBuilder::from_circle(x + radius, y + radius, radius - border / 2.0)
            {
                let stroke = Stroke {
                    width: border,
                    ..Stroke::default()
                };
                canvas.stroke_path(&path, &accent, &stroke, Transform::identity(), None);
            }
        } else {
            for (rx, ry, rw, rh) in [
                (x, y, side, border),
                (x, y + side - border, side, border),
                (x, y + border, border, side - 2.0 * border),
                (x + side - border, y + border, border, side - 2.0 * border),
            ] {
                fill_rect(canvas, rx, ry, rw, rh, &accent);
            }
        }
    }

    /// Colour grab: the zoomed pixels at the pointer (or where a held click
    /// is), the colour as `#rrggbb`, and the key list.
    pub(super) fn color_grab(
        &self,
        canvas: &mut PixmapMut<'_>,
        font: &mut Font<'_>,
        state: &OverlayState,
        frame: FrameView<'_>,
    ) {
        if !state.is_grabbing_color() {
            return;
        }
        let rows: Vec<(String, String)> = GRAB_KEYS
            .iter()
            .map(|(keys, text)| ((*keys).to_owned(), (*text).to_owned()))
            .collect();
        let view = state.color_grab();
        // The key list makes way for the pointer (`OverlayMessage`)
        let (hx, hy, hw, hh) = self.help_rect(font, &rows);
        let covered = view.is_some_and(|view| {
            let (px, py) = (view.pointer.x as f32, view.pointer.y as f32);
            px >= hx && px < hx + hw && py >= hy && py < hy + hh
        });
        if !covered {
            self.help(canvas, font, &rows);
        }
        let Some(view) = view else {
            return;
        };
        let line = self.px(1.0);
        let gap = self.px(SPACE_8);
        let (above, below, center_x) = match view.zoom {
            Some(zoom) => {
                let side = zoom.side() as f32;
                let (x, y) = (zoom.x as f32, zoom.y as f32);
                let radius = self.px(RADIUS_6);
                fill_rounded(
                    canvas,
                    x - 2.0 * line,
                    y - 2.0 * line,
                    side + 4.0 * line,
                    side + 4.0 * line,
                    radius,
                    &solid(bgra_color((0, 0, 0), 0.35)),
                );
                fill_rounded(
                    canvas,
                    x - line,
                    y - line,
                    side + 2.0 * line,
                    side + 2.0 * line,
                    radius,
                    &solid(bgra_color(FG, 1.0)),
                );
                draw_zoom(canvas, frame, &zoom, false, None);
                // The pixel that is picked: a light ring in a dark one
                let cell = zoom.cell as f32;
                let (cx, cy) = (
                    x + zoom.center.0 as f32 * cell,
                    y + zoom.center.1 as f32 * cell,
                );
                outline(
                    canvas,
                    cx - line,
                    cy - line,
                    cell + 2.0 * line,
                    line,
                    (0, 0, 0),
                );
                outline(canvas, cx, cy, cell, line, (255, 255, 255));
                (
                    y - 2.0 * line - gap,
                    y + side + 2.0 * line + gap,
                    x + side / 2.0,
                )
            }
            None => {
                let (x, y) = (view.pointer.x as f32, view.pointer.y as f32);
                let offset = self.px(SPACE_16);
                (y - offset, y + offset, x + offset)
            }
        };
        self.color_chip(canvas, font, view.color, center_x, (above, below));
    }

    /// A pill with a swatch and the colour's `#rrggbb`, centred on
    /// `center_x`, its top at `below`, or its bottom at `above` when that
    /// runs off the monitor.
    fn color_chip(
        &self,
        canvas: &mut PixmapMut<'_>,
        font: &mut Font<'_>,
        color: InkColor,
        center_x: f32,
        (above, below): (f32, f32),
    ) {
        let label = format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b);
        let size = self.px(TEXT_13);
        let metrics = font.metrics(size);
        let swatch = self.px(SPACE_6);
        let pad = self.px(SPACE_8);
        let text = font.measure(&label, size, Weight::Medium).ceil();
        let w = pad + 2.0 * swatch + self.px(SPACE_6) + text + pad;
        let h = metrics.height + 2.0 * self.px(SPACE_4);
        let x = (center_x - w / 2.0)
            .round()
            .clamp(0.0, (self.width - w).max(0.0));
        let y = if below + h <= self.height {
            below.round()
        } else {
            (above - h).round().max(0.0)
        };
        self.panel(canvas, x, y, w, h, self.px(RADIUS_6));
        let (sx, sy) = (x + pad + swatch, y + h / 2.0);
        fill_circle(
            canvas,
            sx,
            sy,
            swatch + self.px(1.0),
            &solid(bgra_color(FG, FG_20)),
        );
        fill_circle(canvas, sx, sy, swatch, &solid(ink(color, 1.0)));
        let paint: Paint<'_> = solid(bgra_color(FG, 1.0));
        font.draw(
            canvas,
            &label,
            (sx + swatch + self.px(SPACE_6)).round(),
            (y + (h - metrics.height) / 2.0).round() + metrics.ascent,
            size,
            Weight::Medium,
            &paint,
        );
    }
}

/// A square ring of width `line` just inside (x, y, side).
fn outline(canvas: &mut PixmapMut<'_>, x: f32, y: f32, side: f32, line: f32, rgb: (u8, u8, u8)) {
    let paint = solid(bgra_color(rgb, 1.0));
    for (rx, ry, rw, rh) in [
        (x, y, side, line),
        (x, y + side - line, side, line),
        (x, y + line, line, side - 2.0 * line),
        (x + side - line, y + line, line, side - 2.0 * line),
    ] {
        fill_rect(canvas, rx, ry, rw, rh, &paint);
    }
}
