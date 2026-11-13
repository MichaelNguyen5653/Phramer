//! The tools' part of the picture: the toolbar with its tooltip and the
//! Shape menu, the colour wheel, the size notice, the selected object's
//! outline and the text caret.

use phramer_annotate::Color as InkColor;
use phramer_capture::{LogicalPoint, LogicalRect};
use tiny_skia::{
    Color, FillRule, LineCap, LineJoin, Paint, PathBuilder, PixmapMut, Stroke, StrokeDash,
    Transform,
};

use super::text::{Font, Weight};
use super::{
    BG, EASE_OUT, FG, FG_10, FG_20, FG_60, Mapping, PANEL_OPACITY, RADIUS_6, SPACE_2, SPACE_4,
    SPACE_8, SPACE_12, SPACE_16, SPACE_24, TEXT_13, Theme, Ui, bgra_color, fill_circle, fill_rect,
    solid,
};
use crate::icons::draw_icon;
use crate::state::{OverlayState, ShapeKind, ShapeOption};

/// The icon inside a button (`--space-16`).
const ICON: f32 = SPACE_16;

/// An object's colour for the BGRA canvas.
pub(super) fn ink(color: InkColor, alpha: f32) -> Color {
    bgra_color(
        (color.r, color.g, color.b),
        alpha * f32::from(color.a) / 255.0,
    )
}

/// `cubic-bezier(x1, y1, x2, y2)` at progress `t`, as CSS evaluates it.
pub(super) fn ease((x1, y1, x2, y2): (f32, f32, f32, f32), t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let curve = |a: f32, b: f32, s: f32| {
        let u = 1.0 - s;
        3.0 * u * u * s * a + 3.0 * u * s * s * b + s * s * s
    };
    // x(s) rises monotonically: find s where it reaches t
    let (mut low, mut high) = (0.0f32, 1.0f32);
    for _ in 0..24 {
        let mid = (low + high) / 2.0;
        if curve(x1, x2, mid) < t {
            low = mid;
        } else {
            high = mid;
        }
    }
    curve(y1, y2, (low + high) / 2.0)
}

/// A disc of hues: angle is the hue, distance the saturation, at full
/// value; its edge anti-aliased over what is under it.
pub(super) fn hue_disc(canvas: &mut PixmapMut<'_>, cx: f32, cy: f32, radius: f32) {
    let (width, height) = (canvas.width() as i32, canvas.height() as i32);
    let data = canvas.data_mut();
    let left = ((cx - radius).floor() as i32).max(0);
    let right = ((cx + radius).ceil() as i32).min(width - 1);
    let top = ((cy - radius).floor() as i32).max(0);
    let bottom = ((cy + radius).ceil() as i32).min(height - 1);
    for y in top..=bottom {
        for x in left..=right {
            let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
            let distance = dx.hypot(dy);
            let coverage = (radius + 0.5 - distance).clamp(0.0, 1.0);
            if coverage <= 0.0 {
                continue;
            }
            let hue = f64::from((-dy).atan2(dx).to_degrees());
            let color = crate::state::disc_color(hue, f64::from(distance / radius));
            let at = (y as usize * width as usize + x as usize) * 4;
            let Some(pixel) = data.get_mut(at..at + 4) else {
                continue;
            };
            for (channel, value) in [(0, color.b), (1, color.g), (2, color.r)] {
                let under = f32::from(pixel[channel]);
                pixel[channel] = (under + (f32::from(value) - under) * coverage).round() as u8;
            }
            pixel[3] = u8::MAX;
        }
    }
}

impl Mapping<'_> {
    /// A logical rectangle in canvas pixels, unclipped: (x, y, w, h).
    pub(super) fn rect(&self, rect: LogicalRect) -> (f32, f32, f32, f32) {
        let physical = self.monitor.logical_rect_to_physical(rect);
        let origin = self.monitor.bounds();
        (
            (physical.x - origin.x) as f32,
            (physical.y - origin.y) as f32,
            physical.width as f32,
            physical.height as f32,
        )
    }

    pub(super) fn point(&self, point: LogicalPoint) -> (f32, f32) {
        let physical = self.monitor.logical_point_to_physical(point);
        let origin = self.monitor.bounds();
        (
            (physical.x - origin.x) as f32,
            (physical.y - origin.y) as f32,
        )
    }
}

impl Ui {
    /// A round button face: a hairline ring, the panel colour (or the
    /// accent when `active`), and the hover or press tint.
    #[allow(clippy::too_many_arguments)]
    fn button_face(
        &self,
        canvas: &mut PixmapMut<'_>,
        (x, y, w, h): (f32, f32, f32, f32),
        active: bool,
        hover: f32,
        pressed: bool,
        accent: Color,
    ) -> (f32, f32) {
        let (cx, cy) = (x + w / 2.0, y + h / 2.0);
        let radius = w.min(h) / 2.0;
        let line = self.px(1.0);
        fill_circle(canvas, cx, cy, radius + line, &solid(bgra_color(FG, FG_10)));
        let body = if active {
            accent
        } else {
            bgra_color(BG, PANEL_OPACITY)
        };
        fill_circle(canvas, cx, cy, radius, &solid(body));
        let tint = if pressed { FG_20 } else { FG_10 * hover };
        if tint > 0.0 {
            fill_circle(canvas, cx, cy, radius, &solid(bgra_color(FG, tint)));
        }
        (cx, cy)
    }

    pub(super) fn toolbar(
        &self,
        canvas: &mut PixmapMut<'_>,
        font: &mut Font<'_>,
        state: &OverlayState,
        mapping: &Mapping<'_>,
        theme: &Theme,
    ) {
        let buttons = state.toolbar();
        if buttons.is_empty() {
            return;
        }
        let accent = bgra_color(theme.accent, 1.0);
        let hover = ease(EASE_OUT, state.hover_progress());
        let icon = self.px(ICON);
        for button in &buttons {
            let (cx, cy) = self.button_face(
                canvas,
                mapping.rect(button.rect),
                button.active,
                if button.hovered { hover } else { 0.0 },
                button.pressed,
                accent,
            );
            let alpha = if button.enabled { 1.0 } else { FG_60 };
            let info = crate::state::tools::button_info(button.button);
            draw_icon(
                canvas,
                info.icon,
                (cx - icon / 2.0).round(),
                (cy - icon / 2.0).round(),
                icon,
                &solid(bgra_color(FG, alpha)),
            );
        }
        for option in state.shape_menu() {
            self.shape_option(canvas, mapping, &option, hover, accent);
        }
        if let Some((anchor, text)) = state.tooltip() {
            self.tooltip(canvas, font, mapping.rect(anchor), &text);
        }
    }

    /// One entry of the Shape menu: the shape itself, as it will look.
    fn shape_option(
        &self,
        canvas: &mut PixmapMut<'_>,
        mapping: &Mapping<'_>,
        option: &ShapeOption,
        hover: f32,
        accent: Color,
    ) {
        let (cx, cy) = self.button_face(
            canvas,
            mapping.rect(option.rect),
            option.active,
            if option.hovered { hover } else { 0.0 },
            false,
            accent,
        );
        let half = self.px(SPACE_12) / 2.0;
        let mut builder = PathBuilder::new();
        match option.kind {
            ShapeKind::Square => {
                if let Some(rect) = tiny_skia::Rect::from_xywh(
                    (cx - half).round(),
                    (cy - half).round(),
                    2.0 * half,
                    2.0 * half,
                ) {
                    builder.push_rect(rect);
                }
            }
            ShapeKind::Circle => builder.push_circle(cx, cy, half),
        }
        let Some(path) = builder.finish() else {
            return;
        };
        let fg = solid(bgra_color(FG, 1.0));
        let stroke = Stroke {
            width: self.px(SPACE_2) * 0.75,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..Stroke::default()
        };
        match option.style {
            phramer_annotate::ShapeStyle::Outline => {
                canvas.stroke_path(&path, &fg, &stroke, Transform::identity(), None);
            }
            phramer_annotate::ShapeStyle::Filled => {
                canvas.fill_path(&path, &fg, FillRule::Winding, Transform::identity(), None);
            }
            phramer_annotate::ShapeStyle::Highlight => {
                let wash = solid(bgra_color(FG, FG_60));
                canvas.fill_path(&path, &wash, FillRule::Winding, Transform::identity(), None);
                canvas.stroke_path(&path, &fg, &stroke, Transform::identity(), None);
            }
        }
    }

    /// The description and shortcut of the hovered button: below it, or
    /// above when that is off the monitor, kept on screen sideways.
    fn tooltip(
        &self,
        canvas: &mut PixmapMut<'_>,
        font: &mut Font<'_>,
        (bx, by, bw, bh): (f32, f32, f32, f32),
        text: &str,
    ) {
        let size = self.px(TEXT_13);
        let metrics = font.metrics(size);
        let pad_x = self.px(SPACE_8);
        let pad_y = self.px(SPACE_4);
        let w = font.measure(text, size, Weight::Regular).ceil() + 2.0 * pad_x;
        let h = metrics.height + 2.0 * pad_y;
        let gap = self.px(SPACE_8);
        let below = by + bh + gap;
        let y = if below + h <= self.height {
            below
        } else {
            (by - gap - h).max(0.0)
        };
        let x = (bx + bw / 2.0 - w / 2.0)
            .round()
            .clamp(0.0, (self.width - w).max(0.0));
        self.panel(canvas, x, y, w, h, self.px(RADIUS_6));
        font.draw(
            canvas,
            text,
            x + pad_x,
            y + pad_y + metrics.ascent,
            size,
            Weight::Regular,
            &solid(bgra_color(FG, 1.0)),
        );
    }

    /// The colour wheel: a panel disc, the palette round it, the current
    /// colour (or the custom disc) in the middle.
    pub(super) fn wheel(
        &self,
        canvas: &mut PixmapMut<'_>,
        state: &OverlayState,
        mapping: &Mapping<'_>,
        theme: &Theme,
    ) {
        let Some(view) = state.wheel() else {
            return;
        };
        let (cx, cy) = mapping.point(view.center);
        let swatch = view.swatch_radius as f32 * self.scale;
        let line = self.px(1.0);
        let outer = view.radius as f32 * self.scale + swatch + self.px(SPACE_8);
        fill_circle(canvas, cx, cy, outer + line, &solid(bgra_color(FG, FG_10)));
        fill_circle(canvas, cx, cy, outer, &solid(bgra_color(BG, PANEL_OPACITY)));
        let accent = solid(bgra_color(theme.accent, 1.0));
        let backing = solid(bgra_color(BG, 1.0));
        for (index, (center, color)) in view.swatches.iter().enumerate() {
            let (x, y) = mapping.point(*center);
            if view.highlighted == Some(index) {
                fill_circle(canvas, x, y, swatch + self.px(SPACE_4), &accent);
                fill_circle(canvas, x, y, swatch + self.px(SPACE_2), &backing);
            }
            fill_circle(canvas, x, y, swatch + line, &solid(bgra_color(FG, FG_20)));
            match color {
                Some(color) => fill_circle(canvas, x, y, swatch, &solid(ink(*color, 1.0))),
                None => hue_disc(canvas, x, y, swatch),
            }
        }
        match view.custom_radius {
            Some(radius) => {
                let radius = radius as f32 * self.scale;
                fill_circle(canvas, cx, cy, radius + line, &solid(bgra_color(FG, FG_20)));
                hue_disc(canvas, cx, cy, radius);
            }
            None => {
                let radius = self.px(SPACE_12);
                fill_circle(canvas, cx, cy, radius + line, &solid(bgra_color(FG, FG_20)));
                fill_circle(canvas, cx, cy, radius, &solid(ink(view.color, 1.0)));
            }
        }
    }

    /// The tool size while it changes: a dot of that size in the colour
    /// being drawn with, and the number, at the top of the monitor.
    pub(super) fn size_notice(
        &self,
        canvas: &mut PixmapMut<'_>,
        font: &mut Font<'_>,
        state: &OverlayState,
    ) {
        let Some(size) = state.size_notice() else {
            return;
        };
        let label = size.to_string();
        let text = self.px(TEXT_13);
        let metrics = font.metrics(text);
        let dot = (size as f32).clamp(SPACE_2, SPACE_24) * self.scale;
        let pad = self.px(SPACE_8);
        let gap = self.px(SPACE_8);
        let text_width = font.measure(&label, text, Weight::Semibold).ceil();
        let w = pad + self.px(SPACE_24) + gap + text_width + pad;
        let h = self.px(SPACE_24) + 2.0 * self.px(SPACE_4);
        let x = ((self.width - w) / 2.0).round();
        let y = self.px(SPACE_16);
        self.panel(canvas, x, y, w, h, self.px(RADIUS_6));
        let dot_center = (x + pad + self.px(SPACE_24) / 2.0, y + h / 2.0);
        fill_circle(
            canvas,
            dot_center.0,
            dot_center.1,
            dot / 2.0 + self.px(1.0),
            &solid(bgra_color(FG, FG_20)),
        );
        fill_circle(
            canvas,
            dot_center.0,
            dot_center.1,
            dot / 2.0,
            &solid(ink(state.draw_color(), 1.0)),
        );
        font.draw(
            canvas,
            &label,
            x + pad + self.px(SPACE_24) + gap,
            (y + (h - metrics.height) / 2.0).round() + metrics.ascent,
            text,
            Weight::Semibold,
            &solid(bgra_color(FG, 1.0)),
        );
    }

    /// The selected object's outline (15.0.0: a black frame under a white
    /// dotted one) and the caret of the text being typed. Both are in image
    /// pixels, which are canvas pixels.
    pub(super) fn object_marks(&self, canvas: &mut PixmapMut<'_>, state: &OverlayState) {
        if let Some(bounds) = state.selected_bounds() {
            let inset = self.px(SPACE_2);
            if let Some(rect) = tiny_skia::Rect::from_xywh(
                bounds.x as f32 - inset,
                bounds.y as f32 - inset,
                bounds.width as f32 + 2.0 * inset,
                bounds.height as f32 + 2.0 * inset,
            ) {
                let path = PathBuilder::from_rect(rect);
                let dark = Stroke {
                    width: self.px(3.0),
                    ..Stroke::default()
                };
                canvas.stroke_path(
                    &path,
                    &solid(bgra_color((0, 0, 0), 1.0)),
                    &dark,
                    Transform::identity(),
                    None,
                );
                let dots = self.px(SPACE_4);
                let light = Stroke {
                    width: self.px(1.0),
                    dash: StrokeDash::new(vec![dots, dots], 0.0),
                    ..Stroke::default()
                };
                canvas.stroke_path(
                    &path,
                    &solid(bgra_color((255, 255, 255), 1.0)),
                    &light,
                    Transform::identity(),
                    None,
                );
            }
        }
        if let Some((top, height)) = state.caret() {
            let paint: Paint<'_> = solid(ink(state.draw_color(), 1.0));
            fill_rect(
                canvas,
                top.x as f32,
                top.y as f32,
                self.px(1.0),
                height.max(1) as f32,
                &paint,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ease_out_curve_starts_fast_and_ends_at_one() {
        assert!((ease(EASE_OUT, 0.0)).abs() < 1e-3);
        assert!((ease(EASE_OUT, 1.0) - 1.0).abs() < 1e-3);
        // An expo-style ease-out is most of the way there by a third
        assert!(ease(EASE_OUT, 0.33) > 0.8);
    }
}
