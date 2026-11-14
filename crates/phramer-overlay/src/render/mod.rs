//! Draws one monitor's overlay: the frozen screenshot with the annotations
//! in, dimmed outside the selection, with the selection frame, the
//! highlighted window, the toolbar, the colour wheel, the size label and the
//! help message on top.
//!
//! The look follows `docs/design/raycast-notes.md` and `app/src/tokens.css`:
//! sizes from the same scales (4/6/8/12/16 spacing, 4/6/12 radii, 13 px
//! type), neutral dark panels with a hairline edge, and the accent (the
//! `uiColor` setting) only on the selection and window frames. Every size is
//! in logical pixels and scaled to the monitor, so the overlay looks the same
//! at every DPI.

pub(crate) mod layer;
mod magnify;
mod modebar;
mod panel;
pub(crate) mod text;
mod tools;

use phramer_capture::{Frame, LogicalRect, Monitor};
use phramer_core::config::Settings;
use tiny_skia::{Color, FillRule, Paint, PathBuilder, PixmapMut, Rect, Transform};

use crate::error::OverlayError;
use crate::state::{GeometryLabel, OverlayState};
use layer::Layer;
use text::{Font, Weight};

// `app/src/tokens.css`, in logical pixels
const SPACE_2: f32 = 2.0;
const SPACE_4: f32 = 4.0;
const SPACE_6: f32 = 6.0;
const SPACE_8: f32 = 8.0;
const SPACE_12: f32 = 12.0;
const SPACE_16: f32 = 16.0;
const SPACE_24: f32 = 24.0;
const RADIUS_4: f32 = 4.0;
const RADIUS_6: f32 = 6.0;
const RADIUS_12: f32 = 12.0;
const TEXT_13: f32 = 13.0;
/// `--ease-out`, for fades in.
const EASE_OUT: (f32, f32, f32, f32) = (0.16, 1.0, 0.3, 1.0);
/// The selection handles are 8 px circles.
const HANDLE_RADIUS: f32 = 4.0;
/// The highlighted window's frame, drawn inside the window (15.0.0: 2 px).
const HOVER_FRAME: f32 = 2.0;

/// `--fg` and `--bg` of the dark scheme: the overlay sits on a dimmed
/// screen, so its panels are always dark.
const FG: (u8, u8, u8) = (0xed, 0xed, 0xef);
const BG: (u8, u8, u8) = (0x18, 0x19, 0x1b);
const PANEL_OPACITY: f32 = 0.92;
/// `--fg-10`, `--fg-20`, `--fg-60`.
const FG_10: f32 = 0.10;
const FG_20: f32 = 0.20;
const FG_60: f32 = 0.60;

/// The colours that come from the settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    /// `uiColor`: the selection and window frames.
    pub accent: (u8, u8, u8),
    /// `contrastOpacity`: how dark the area outside the selection is.
    pub dim_alpha: u8,
}

impl Theme {
    pub fn from_settings(settings: &Settings) -> Self {
        let accent = settings.ui_color();
        Self {
            accent: (accent.r, accent.g, accent.b),
            dim_alpha: u8::try_from(settings.contrast_opacity()).unwrap_or(u8::MAX),
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::from_settings(&Settings::default())
    }
}

/// A frozen screenshot of one monitor: 8-bit BGRA, top row first, as
/// [`phramer_capture::Frame`] holds it.
#[derive(Debug, Clone, Copy)]
pub struct FrameView<'a> {
    width: u32,
    height: u32,
    bgra: &'a [u8],
}

impl<'a> FrameView<'a> {
    pub fn new(width: u32, height: u32, bgra: &'a [u8]) -> Result<Self, OverlayError> {
        let expected = (width as usize)
            .saturating_mul(height as usize)
            .saturating_mul(4);
        if bgra.len() != expected {
            return Err(OverlayError::FrameSize {
                width,
                height,
                expected,
                actual: bgra.len(),
            });
        }
        Ok(Self {
            width,
            height,
            bgra,
        })
    }

    pub fn from_frame(frame: &'a Frame) -> Self {
        Self {
            width: frame.width(),
            height: frame.height(),
            bgra: frame.bgra(),
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn bgra(&self) -> &'a [u8] {
        self.bgra
    }
}

/// Straight 8-bit RGBA pixels, top row first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// Renders the window of `monitor` (an index into the state's monitors)
/// over that monitor's frame. `font` is TrueType or OpenType bytes; the
/// overlay bundles Inter Variable as [`crate::BUNDLED_FONT`].
pub fn render(
    state: &OverlayState,
    monitor: usize,
    frame: FrameView<'_>,
    theme: &Theme,
    font: &[u8],
) -> Result<RgbaImage, OverlayError> {
    let mut painter = Painter::new(frame, theme);
    let mut font = Font::parse(font)?;
    let mut pixels = vec![0; frame.bgra.len()];
    painter.paint(state, monitor, frame, theme, &mut font, &mut pixels)?;
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    Ok(RgbaImage {
        width: frame.width,
        height: frame.height,
        pixels,
    })
}

/// Paints one window again and again: the screenshot with the annotations
/// in, and its dimmed copy, are kept between frames and redone only where
/// the document changed.
pub(crate) struct Painter {
    dimmed: Vec<u8>,
    dim_alpha: u8,
    layer: Layer,
    panel: panel::PanelCache,
}

impl Painter {
    /// A painter for `frame`. Text objects are set in the state's fonts.
    pub(crate) fn new(frame: FrameView<'_>, theme: &Theme) -> Self {
        Self {
            dimmed: dim(frame.bgra, theme.dim_alpha),
            dim_alpha: theme.dim_alpha,
            layer: Layer::new(),
            panel: panel::PanelCache::default(),
        }
    }

    /// Brings the annotated screenshot and its dimmed copy up to the
    /// state's document.
    fn update_layer(&mut self, state: &OverlayState, frame: FrameView<'_>) {
        let changed = self.layer.update(
            frame.bgra,
            (frame.width, frame.height),
            state.document().objects(),
            state.excluded_object(),
            state.text_fonts(),
        );
        let Some(area) = changed else {
            return;
        };
        let source = self.layer.pixels(frame.bgra);
        if area.x == 0 && area.y == 0 && area.width == frame.width && area.height == frame.height {
            self.dimmed = dim(source, self.dim_alpha);
            return;
        }
        let stride = frame.width as usize * 4;
        let (start, end) = (
            area.x.max(0) as usize * 4,
            (area.x.max(0) as usize + area.width as usize) * 4,
        );
        for row in area.y.max(0) as usize..(area.y.max(0) as usize + area.height as usize) {
            let line = row * stride;
            if let (Some(target), Some(source)) = (
                self.dimmed.get_mut(line + start..line + end),
                source.get(line + start..line + end),
            ) {
                target.copy_from_slice(&dim(source, self.dim_alpha));
            }
        }
    }

    /// Draws the previews (the object being drawn, dragged or recoloured,
    /// the text being typed) into `out`: as they are inside the selection,
    /// dimmed outside it.
    fn paint_previews(
        &self,
        state: &OverlayState,
        frame: FrameView<'_>,
        selection: Option<PixelRect>,
        out: &mut [u8],
    ) {
        let previews = state.previews();
        if previews.is_empty() {
            return;
        }
        let image = phramer_annotate::Rect::new(0, 0, frame.width, frame.height);
        let area = layer::reach(&previews, state.text_fonts()).intersection(&image);
        let source = self.layer.pixels(frame.bgra);
        let Some(pixels) = layer::render_area(
            source,
            (frame.width, frame.height),
            area,
            &previews,
            state.text_fonts(),
        ) else {
            return;
        };
        let stride = frame.width as usize * 4;
        let row_bytes = area.width as usize * 4;
        let keep = 255 - u32::from(self.dim_alpha);
        for (row, line) in pixels.chunks_exact(row_bytes).enumerate() {
            let y = area.y as usize + row;
            for (column, pixel) in line.as_chunks::<4>().0.iter().enumerate() {
                let x = area.x as usize + column;
                let at = y * stride + x * 4;
                let Some(target) = out.get_mut(at..at + 4) else {
                    continue;
                };
                let lit = selection.is_some_and(|s| {
                    (s.left as usize..s.right as usize).contains(&x)
                        && (s.top as usize..s.bottom as usize).contains(&y)
                });
                if lit {
                    target.copy_from_slice(pixel);
                } else {
                    for channel in 0..3 {
                        let product = u32::from(pixel[channel]) * keep + 128;
                        target[channel] = ((product + (product >> 8)) >> 8) as u8;
                    }
                    target[3] = u8::MAX;
                }
            }
        }
    }

    /// Paints into `out`, BGRA like `frame`, which must be the frame the
    /// painter was made from.
    pub(crate) fn paint(
        &mut self,
        state: &OverlayState,
        monitor: usize,
        frame: FrameView<'_>,
        theme: &Theme,
        font: &mut Font<'_>,
        out: &mut [u8],
    ) -> Result<(), OverlayError> {
        let (width, height) = (frame.width, frame.height);
        if out.len() != self.dimmed.len() {
            return Err(OverlayError::FrameSize {
                width,
                height,
                expected: self.dimmed.len(),
                actual: out.len(),
            });
        }
        let Some(screen) = state.monitors().get(monitor) else {
            return Err(OverlayError::NoSuchMonitor(monitor));
        };
        // Another monitor while following the cursor: dimmed, nothing else
        if monitor != state.active_monitor() {
            out.copy_from_slice(&self.dimmed);
            return Ok(());
        }
        self.update_layer(state, frame);
        out.copy_from_slice(&self.dimmed);

        let mapping = Mapping {
            monitor: screen,
            width,
            height,
        };
        let selection = state.selection().and_then(|rect| mapping.to_pixels(rect));
        let hover = state
            .hover_window()
            .and_then(|rect| mapping.to_pixels(rect));
        if let Some(clear) = selection.or(hover) {
            undim(self.layer.pixels(frame.bgra), frame.width, clear, out);
        }
        self.paint_previews(state, frame, selection, out);

        let mut canvas = PixmapMut::from_bytes(out, width, height)
            .ok_or(OverlayError::Canvas { width, height })?;
        let ui = Ui {
            scale: screen.scale() as f32,
            width: width as f32,
            height: height as f32,
        };
        // The canvas is BGRA; tiny-skia writes RGBA, so every colour goes in
        // with red and blue exchanged
        let accent = bgra_color(theme.accent, 1.0);

        ui.object_marks(&mut canvas, state);
        if let Some(selection) = selection {
            ui.selection_frame(&mut canvas, selection, accent);
            if let Some((position, label)) = state.geometry_label() {
                ui.geometry_label(&mut canvas, font, selection, position, &label);
            }
        } else if let Some(window) = hover {
            ui.window_frame(&mut canvas, window, accent);
        }
        ui.grid(&mut canvas, state, &mapping, selection, theme);
        ui.toolbar(&mut canvas, font, state, &mapping, theme);
        ui.tool_labels(&mut canvas, font, state, &mapping);
        ui.wheel(&mut canvas, state, &mapping, theme);
        ui.size_notice(&mut canvas, font, state);
        if state.help_visible() {
            ui.help(&mut canvas, font, &state.help_rows());
        }
        ui.mode_bar(&mut canvas, font, state, &mapping);
        ui.side_panel_tab(&mut canvas, font, state, &mapping);
        ui.side_panel(&mut canvas, font, state, &mapping, theme, &mut self.panel);
        ui.magnifier(&mut canvas, state, frame, theme);
        ui.color_grab(&mut canvas, font, state, frame);
        if !state.is_focused() {
            ui.focus_hint(&mut canvas, font);
        }
        Ok(())
    }
}

/// Copies the undimmed screenshot (`source`, BGRA rows of `width`) back
/// into `area`.
fn undim(source: &[u8], width: u32, area: PixelRect, out: &mut [u8]) {
    let stride = width as usize * 4;
    let (start, end) = (area.left as usize * 4, area.right as usize * 4);
    for row in area.top as usize..area.bottom as usize {
        let line = row * stride;
        out[line + start..line + end].copy_from_slice(&source[line + start..line + end]);
    }
}

/// `frame` with black at `alpha` laid over it, alpha kept opaque. Written
/// so the compiler vectorises it: it runs once per window, before the
/// first frame.
fn dim(frame: &[u8], alpha: u8) -> Vec<u8> {
    let keep = 255 - u32::from(alpha);
    let mut out = vec![0u8; frame.len()];
    for (target, source) in out
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(frame.as_chunks::<4>().0)
    {
        for channel in 0..3 {
            // channel * keep / 255, rounded
            let product = u32::from(source[channel]) * keep + 128;
            target[channel] = ((product + (product >> 8)) >> 8) as u8;
        }
        target[3] = u8::MAX;
    }
    out
}

/// A rectangle of whole pixels inside the canvas, exclusive right and
/// bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PixelRect {
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
}

impl PixelRect {
    fn width(&self) -> f32 {
        (self.right - self.left) as f32
    }

    fn height(&self) -> f32 {
        (self.bottom - self.top) as f32
    }
}

/// Logical rectangles of the state onto the canvas.
struct Mapping<'a> {
    monitor: &'a Monitor,
    width: u32,
    height: u32,
}

impl Mapping<'_> {
    /// The visible part of `rect`, in canvas pixels.
    fn to_pixels(&self, rect: LogicalRect) -> Option<PixelRect> {
        let physical = self.monitor.logical_rect_to_physical(rect);
        let origin = self.monitor.bounds();
        let clamp = |value: i32, max: u32| value.clamp(0, max as i32) as u32;
        let area = PixelRect {
            left: clamp(physical.x - origin.x, self.width),
            top: clamp(physical.y - origin.y, self.height),
            right: clamp(physical.right() - origin.x, self.width),
            bottom: clamp(physical.bottom() - origin.y, self.height),
        };
        (area.right > area.left && area.bottom > area.top).then_some(area)
    }
}

/// Lays out and draws the overlay's controls at one monitor's scale.
struct Ui {
    scale: f32,
    width: f32,
    height: f32,
}

impl Ui {
    /// A logical length in whole canvas pixels, at least one.
    fn px(&self, logical: f32) -> f32 {
        (logical * self.scale).round().max(1.0)
    }

    fn selection_frame(&self, canvas: &mut PixmapMut<'_>, area: PixelRect, accent: Color) {
        // A hairline just outside the selection, so every selected pixel
        // stays visible; at a monitor edge (Ctrl+A) it moves inside
        let line = self.px(1.0);
        let left = (area.left as f32 - line).max(0.0);
        let top = (area.top as f32 - line).max(0.0);
        let right = (area.right as f32 + line).min(self.width);
        let bottom = (area.bottom as f32 + line).min(self.height);
        let paint = solid(accent);
        for (x, y, w, h) in [
            (left, top, right - left, line),
            (left, bottom - line, right - left, line),
            (left, top + line, line, bottom - top - 2.0 * line),
            (right - line, top + line, line, bottom - top - 2.0 * line),
        ] {
            fill_rect(canvas, x, y, w, h, &paint);
        }

        let outer = HANDLE_RADIUS * self.scale;
        let ring = self.px(1.0);
        // Centred on the selection's edges, kept whole on the monitor
        let clamp_x = |x: f32| x.clamp(outer + ring, (self.width - outer - ring).max(outer + ring));
        let clamp_y =
            |y: f32| y.clamp(outer + ring, (self.height - outer - ring).max(outer + ring));
        let (x0, y0) = (clamp_x(area.left as f32), clamp_y(area.top as f32));
        let (x1, y1) = (clamp_x(area.right as f32), clamp_y(area.bottom as f32));
        let (mid_x, mid_y) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        let shadow = solid(bgra_color((0, 0, 0), 0.35));
        let fill = solid(bgra_color((255, 255, 255), 1.0));
        for (x, y) in [
            (x0, y0),
            (mid_x, y0),
            (x1, y0),
            (x1, mid_y),
            (x1, y1),
            (mid_x, y1),
            (x0, y1),
            (x0, mid_y),
        ] {
            fill_circle(canvas, x, y, outer + ring, &shadow);
            fill_circle(canvas, x, y, outer, &fill);
            fill_circle(canvas, x, y, outer - ring, &solid(accent));
        }
    }

    fn window_frame(&self, canvas: &mut PixmapMut<'_>, area: PixelRect, accent: Color) {
        let line = self
            .px(HOVER_FRAME)
            .min(area.width() / 2.0)
            .min(area.height() / 2.0);
        let (left, top) = (area.left as f32, area.top as f32);
        let (width, height) = (area.width(), area.height());
        let paint = solid(accent);
        for (x, y, w, h) in [
            (left, top, width, line),
            (left, top + height - line, width, line),
            (left, top + line, line, height - 2.0 * line),
            (left + width - line, top + line, line, height - 2.0 * line),
        ] {
            fill_rect(canvas, x, y, w, h, &paint);
        }
    }

    /// The `WxH+X+Y` pill, inside the selection at the configured corner.
    fn geometry_label(
        &self,
        canvas: &mut PixmapMut<'_>,
        font: &mut Font<'_>,
        selection: PixelRect,
        position: GeometryLabel,
        label: &str,
    ) {
        let size = self.px(TEXT_13);
        let metrics = font.metrics(size);
        let text_width = font.measure(label, size, Weight::Medium).ceil();
        let pad_x = self.px(SPACE_8);
        let pad_y = self.px(SPACE_4);
        let (w, h) = (text_width + 2.0 * pad_x, metrics.height + 2.0 * pad_y);
        let inset = self.px(SPACE_8);
        let (left, top) = (selection.left as f32, selection.top as f32);
        let (right, bottom) = (selection.right as f32, selection.bottom as f32);
        let (x, y) = match position {
            GeometryLabel::TopLeft => (left + inset, top + inset),
            GeometryLabel::BottomLeft => (left + inset, bottom - inset - h),
            GeometryLabel::TopRight => (right - inset - w, top + inset),
            GeometryLabel::BottomRight | GeometryLabel::Hidden => {
                (right - inset - w, bottom - inset - h)
            }
            GeometryLabel::Center => (
                ((left + right - w) / 2.0).round(),
                ((top + bottom - h) / 2.0).round(),
            ),
        };
        // Kept on the monitor when the selection is smaller than the label
        let x = x.min(self.width - w).max(0.0);
        let y = y.min(self.height - h).max(0.0);
        self.panel(canvas, x, y, w, h, self.px(RADIUS_6));
        font.draw(
            canvas,
            label,
            x + pad_x,
            y + pad_y + metrics.ascent,
            size,
            Weight::Medium,
            &solid(bgra_color(FG, 1.0)),
        );
    }

    /// The key list, centred on the monitor: keys as chips on the right of
    /// the first column, what they do on the left of the second.
    fn help(&self, canvas: &mut PixmapMut<'_>, font: &mut Font<'_>, rows: &[(String, String)]) {
        let size = self.px(TEXT_13);
        let metrics = font.metrics(size);
        let chip_pad = self.px(SPACE_6);
        let chip_h = metrics.height + 2.0 * self.px(2.0);
        let row_h = self.px(24.0).max(chip_h);
        let gap = self.px(SPACE_12);
        let key_width = |font: &mut Font<'_>, keys: &str| {
            font.measure(keys, size, Weight::Semibold).ceil() + 2.0 * chip_pad
        };
        let keys_w = rows
            .iter()
            .map(|(keys, _)| key_width(font, keys))
            .fold(0.0, f32::max);
        let text_w = rows
            .iter()
            .map(|(_, text)| font.measure(text, size, Weight::Regular).ceil())
            .fold(0.0, f32::max);
        let pad_x = self.px(SPACE_16);
        let pad_y = self.px(SPACE_12);
        let w = pad_x + keys_w + gap + text_w + pad_x;
        let h = pad_y + row_h * rows.len() as f32 + pad_y;
        let x = ((self.width - w) / 2.0).round();
        let y = ((self.height - h) / 2.0).round();
        self.panel(canvas, x, y, w, h, self.px(RADIUS_12));

        let chip = solid(bgra_color(FG, FG_10));
        let key_paint = solid(bgra_color(FG, 1.0));
        let text_paint = solid(bgra_color(FG, FG_60));
        for (index, (keys, text)) in rows.iter().enumerate() {
            let row_top = y + pad_y + row_h * index as f32;
            let chip_w = key_width(font, keys);
            let chip_x = x + pad_x + keys_w - chip_w;
            let chip_y = (row_top + (row_h - chip_h) / 2.0).round();
            fill_rounded(
                canvas,
                chip_x,
                chip_y,
                chip_w,
                chip_h,
                self.px(RADIUS_4),
                &chip,
            );
            let baseline = chip_y + self.px(2.0) + metrics.ascent;
            font.draw(
                canvas,
                keys,
                chip_x + chip_pad,
                baseline,
                size,
                Weight::Semibold,
                &key_paint,
            );
            font.draw(
                canvas,
                text,
                x + pad_x + keys_w + gap,
                baseline,
                size,
                Weight::Regular,
                &text_paint,
            );
        }
    }

    /// Where [`Ui::help`] puts the key list: (x, y, w, h).
    fn help_rect(&self, font: &mut Font<'_>, rows: &[(String, String)]) -> (f32, f32, f32, f32) {
        let size = self.px(TEXT_13);
        let metrics = font.metrics(size);
        let chip_pad = self.px(SPACE_6);
        let chip_h = metrics.height + 2.0 * self.px(2.0);
        let row_h = self.px(24.0).max(chip_h);
        let keys_w = rows
            .iter()
            .map(|(keys, _)| font.measure(keys, size, Weight::Semibold).ceil() + 2.0 * chip_pad)
            .fold(0.0, f32::max);
        let text_w = rows
            .iter()
            .map(|(_, text)| font.measure(text, size, Weight::Regular).ceil())
            .fold(0.0, f32::max);
        let (pad_x, pad_y) = (self.px(SPACE_16), self.px(SPACE_12));
        let w = pad_x + keys_w + self.px(SPACE_12) + text_w + pad_x;
        let h = pad_y + row_h * rows.len() as f32 + pad_y;
        (
            ((self.width - w) / 2.0).round(),
            ((self.height - h) / 2.0).round(),
            w,
            h,
        )
    }

    /// 15.0.0's "lost focus" notice, bottom right.
    fn focus_hint(&self, canvas: &mut PixmapMut<'_>, font: &mut Font<'_>) {
        const TEXT: &str =
            "Phramer has lost focus. Keyboard shortcuts won't work until you click somewhere.";
        let size = self.px(TEXT_13);
        let metrics = font.metrics(size);
        let pad_x = self.px(SPACE_8);
        let pad_y = self.px(SPACE_4);
        let w = font.measure(TEXT, size, Weight::Regular).ceil() + 2.0 * pad_x;
        let h = metrics.height + 2.0 * pad_y;
        let margin = self.px(SPACE_16);
        let x = (self.width - margin - w).max(0.0);
        let y = (self.height - margin - h).max(0.0);
        self.panel(canvas, x, y, w, h, self.px(RADIUS_6));
        font.draw(
            canvas,
            TEXT,
            x + pad_x,
            y + pad_y + metrics.ascent,
            size,
            Weight::Regular,
            &solid(bgra_color(FG, 1.0)),
        );
    }

    /// A dark rounded panel with a hairline edge (`--shadow-panel`'s ring).
    fn panel(&self, canvas: &mut PixmapMut<'_>, x: f32, y: f32, w: f32, h: f32, radius: f32) {
        let line = self.px(1.0);
        fill_rounded(
            canvas,
            x - line,
            y - line,
            w + 2.0 * line,
            h + 2.0 * line,
            radius + line,
            &solid(bgra_color(FG, FG_10)),
        );
        fill_rounded(
            canvas,
            x,
            y,
            w,
            h,
            radius,
            &solid(bgra_color(BG, PANEL_OPACITY)),
        );
    }
}

/// A colour for the BGRA canvas: red and blue exchanged.
fn bgra_color((r, g, b): (u8, u8, u8), alpha: f32) -> Color {
    Color::from_rgba8(b, g, r, (alpha * 255.0).round() as u8)
}

fn solid(color: Color) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color(color);
    paint.anti_alias = true;
    paint
}

fn fill_rect(canvas: &mut PixmapMut<'_>, x: f32, y: f32, w: f32, h: f32, paint: &Paint<'_>) {
    if let Some(rect) = Rect::from_xywh(x, y, w, h) {
        canvas.fill_rect(rect, paint, Transform::identity(), None);
    }
}

fn fill_circle(canvas: &mut PixmapMut<'_>, x: f32, y: f32, radius: f32, paint: &Paint<'_>) {
    if let Some(path) = PathBuilder::from_circle(x, y, radius) {
        canvas.fill_path(&path, paint, FillRule::Winding, Transform::identity(), None);
    }
}

fn fill_rounded(
    canvas: &mut PixmapMut<'_>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    radius: f32,
    paint: &Paint<'_>,
) {
    let r = radius.min(w / 2.0).min(h / 2.0);
    // A quarter circle as one cubic: the usual 0.5523 handle length
    let k = r * 0.552_284_8;
    let mut path = PathBuilder::new();
    path.move_to(x + r, y);
    path.line_to(x + w - r, y);
    path.cubic_to(x + w - r + k, y, x + w, y + r - k, x + w, y + r);
    path.line_to(x + w, y + h - r);
    path.cubic_to(x + w, y + h - r + k, x + w - r + k, y + h, x + w - r, y + h);
    path.line_to(x + r, y + h);
    path.cubic_to(x + r - k, y + h, x, y + h - r + k, x, y + h - r);
    path.line_to(x, y + r);
    path.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    path.close();
    if let Some(path) = path.finish() {
        canvas.fill_path(&path, paint, FillRule::Winding, Transform::identity(), None);
    }
}

#[cfg(test)]
mod tests {
    use super::dim;

    #[test]
    fn dimming_matches_black_at_the_contrast_opacity() {
        // 190 (the default) leaves 65/255 of each channel
        assert_eq!(dim(&[255, 128, 0, 255], 190), [65, 33, 0, 255]);
        assert_eq!(dim(&[200, 100, 50, 255], 0), [200, 100, 50, 255]);
        assert_eq!(dim(&[200, 100, 50, 255], 255), [0, 0, 0, 255]);
    }
}
