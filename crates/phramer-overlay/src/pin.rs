//! A capture pinned on the desktop: 15.0.0's `PinWidget`, as a pure state
//! machine. The Windows shell (`spawn_pin`) feeds it [`PinEvent`]s and
//! carries out the [`PinEffect`]s; [`render_pin`] draws the window.
//!
//! Sizes are physical pixels. The image is shown pixel for pixel until it is
//! zoomed, so a pin is as sharp as the capture on any monitor; only the
//! frame around it follows the monitor's scale.

use crate::export::CapturedImage;

/// The frame around the image, logical pixels (`--space-8`; 15.0.0 had 7).
pub const PIN_MARGIN: f64 = 8.0;
/// Zoom per wheel notch, as a fraction of the original size (`STEP`).
pub const ZOOM_STEP: f64 = 0.03;
/// Zooming out stops once the longer side is this many logical pixels
/// (`MIN_SIZE`).
pub const MIN_SIDE: f64 = 100.0;
/// Zooming in stops here; 15.0.0 had no limit.
pub const MAX_ZOOM: f64 = 8.0;
/// Opacity per menu entry or Ctrl+wheel notch.
pub const OPACITY_STEP: f64 = 0.1;
/// A pin never goes fully transparent: it could no longer be found.
pub const MIN_OPACITY: f64 = 0.1;
/// How strong the glow is right at the image's edge.
const GLOW_ALPHA: f64 = 0.5;
/// One wheel notch (`WHEEL_DELTA`).
const WHEEL_NOTCH: f64 = 120.0;

/// The right-click menu, in 15.0.0's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PinMenuItem {
    CopyToClipboard,
    SaveToFile,
    LoadIntoEditor,
    RotateRight,
    RotateLeft,
    IncreaseOpacity,
    DecreaseOpacity,
    Close,
}

/// The menu as shown: `None` is a separator.
pub const PIN_MENU: [Option<PinMenuItem>; 10] = [
    Some(PinMenuItem::CopyToClipboard),
    Some(PinMenuItem::SaveToFile),
    Some(PinMenuItem::LoadIntoEditor),
    None,
    Some(PinMenuItem::RotateRight),
    Some(PinMenuItem::RotateLeft),
    Some(PinMenuItem::IncreaseOpacity),
    Some(PinMenuItem::DecreaseOpacity),
    None,
    Some(PinMenuItem::Close),
];

impl PinMenuItem {
    pub fn label(self) -> &'static str {
        match self {
            Self::CopyToClipboard => "Copy to clipboard",
            Self::SaveToFile => "Save to file",
            Self::LoadIntoEditor => "Load into editor",
            Self::RotateRight => "Rotate Right",
            Self::RotateLeft => "Rotate Left",
            Self::IncreaseOpacity => "Increase Opacity",
            Self::DecreaseOpacity => "Decrease Opacity",
            Self::Close => "Close",
        }
    }

    /// The menu command id of this entry (never 0, which means "none").
    pub fn id(self) -> usize {
        PIN_MENU
            .iter()
            .position(|entry| *entry == Some(self))
            .map_or(0, |index| index + 1)
    }

    pub fn from_id(id: usize) -> Option<Self> {
        PIN_MENU.get(id.checked_sub(1)?).copied().flatten()
    }
}

/// Keys a pin reacts to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinKey {
    /// 1 to 9 set the opacity to 10 % to 90 %, 0 to 100 %.
    Digit(u8),
    Escape,
    /// Ctrl+Q.
    Quit,
}

/// Input to a pin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PinEvent {
    /// `delta` in wheel units (120 a notch); with Ctrl it changes the
    /// opacity instead of the size.
    Wheel {
        delta: i32,
        ctrl: bool,
    },
    Key(PinKey),
    Menu(PinMenuItem),
    DoubleClick,
    /// The pointer entered (`true`) or left the pin.
    Hover(bool),
    /// The pin moved to a monitor of this scale (1.0 at 96 DPI).
    ScaleChanged(f64),
}

/// What a pin hands to the application, with its current pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinRequest {
    Copy,
    Save,
    OpenInEditor,
}

/// What the shell must do after an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinEffect {
    /// Draw again; the window size may have changed.
    Redraw,
    Request(PinRequest),
    Close,
}

/// One pin.
#[derive(Debug, Clone)]
pub struct PinState {
    /// As shown: rotations are applied to the pixels, as 15.0.0 did.
    image: CapturedImage,
    zoom: f64,
    opacity: f64,
    hover: bool,
    /// The monitor's scale, for the frame and the zoom-out limit.
    scale: f64,
    smooth: bool,
}

impl PinState {
    /// `scale` is the monitor's (1.0 at 96 DPI); `smooth` is
    /// `antialiasingPinZoom`.
    pub fn new(image: CapturedImage, scale: f64, smooth: bool) -> Self {
        Self {
            image,
            zoom: 1.0,
            opacity: 1.0,
            hover: false,
            scale: sane_scale(scale),
            smooth,
        }
    }

    pub fn image(&self) -> &CapturedImage {
        &self.image
    }

    pub fn zoom(&self) -> f64 {
        self.zoom
    }

    pub fn opacity(&self) -> f64 {
        self.opacity
    }

    /// The window's constant alpha.
    pub fn alpha(&self) -> u8 {
        (self.opacity * 255.0).round().clamp(0.0, 255.0) as u8
    }

    pub fn is_hovered(&self) -> bool {
        self.hover
    }

    pub fn is_smooth(&self) -> bool {
        self.smooth
    }

    /// The frame width, physical pixels.
    pub fn margin(&self) -> i32 {
        (PIN_MARGIN * self.scale).round() as i32
    }

    /// The image as displayed, physical pixels.
    pub fn display_size(&self) -> (u32, u32) {
        let side = |pixels: u32| ((f64::from(pixels) * self.zoom).round() as u32).max(1);
        (side(self.image.width), side(self.image.height))
    }

    /// The whole window, physical pixels.
    pub fn window_size(&self) -> (i32, i32) {
        let (width, height) = self.display_size();
        let margin = self.margin();
        (
            i32::try_from(width)
                .unwrap_or(i32::MAX)
                .saturating_add(2 * margin),
            i32::try_from(height)
                .unwrap_or(i32::MAX)
                .saturating_add(2 * margin),
        )
    }

    fn min_zoom(&self) -> f64 {
        let longer = f64::from(self.image.width.max(self.image.height).max(1));
        (MIN_SIDE * self.scale / longer).min(1.0)
    }

    pub fn handle(&mut self, event: PinEvent) -> Vec<PinEffect> {
        match event {
            PinEvent::Wheel { delta, ctrl } => {
                let notches = f64::from(delta) / WHEEL_NOTCH;
                if notches == 0.0 {
                    return Vec::new();
                }
                if ctrl {
                    return self.set_opacity(self.opacity + notches * OPACITY_STEP);
                }
                let zoom = (self.zoom + notches * ZOOM_STEP).clamp(self.min_zoom(), MAX_ZOOM);
                if zoom == self.zoom {
                    return Vec::new();
                }
                self.zoom = zoom;
                vec![PinEffect::Redraw]
            }
            PinEvent::Key(PinKey::Escape | PinKey::Quit) | PinEvent::DoubleClick => {
                vec![PinEffect::Close]
            }
            PinEvent::Key(PinKey::Digit(digit)) => match digit {
                0 => self.set_opacity(1.0),
                1..=9 => self.set_opacity(f64::from(digit) / 10.0),
                _ => Vec::new(),
            },
            PinEvent::Menu(item) => self.menu(item),
            PinEvent::Hover(hover) => {
                if self.hover == hover {
                    return Vec::new();
                }
                self.hover = hover;
                vec![PinEffect::Redraw]
            }
            PinEvent::ScaleChanged(scale) => {
                self.scale = sane_scale(scale);
                self.zoom = self.zoom.max(self.min_zoom());
                vec![PinEffect::Redraw]
            }
        }
    }

    fn menu(&mut self, item: PinMenuItem) -> Vec<PinEffect> {
        match item {
            PinMenuItem::CopyToClipboard => vec![PinEffect::Request(PinRequest::Copy)],
            PinMenuItem::SaveToFile => vec![PinEffect::Request(PinRequest::Save)],
            PinMenuItem::LoadIntoEditor => vec![PinEffect::Request(PinRequest::OpenInEditor)],
            PinMenuItem::RotateRight => {
                self.image = rotate_right(&self.image);
                vec![PinEffect::Redraw]
            }
            PinMenuItem::RotateLeft => {
                self.image = rotate_left(&self.image);
                vec![PinEffect::Redraw]
            }
            PinMenuItem::IncreaseOpacity => self.set_opacity(self.opacity + OPACITY_STEP),
            PinMenuItem::DecreaseOpacity => self.set_opacity(self.opacity - OPACITY_STEP),
            PinMenuItem::Close => vec![PinEffect::Close],
        }
    }

    fn set_opacity(&mut self, opacity: f64) -> Vec<PinEffect> {
        // Rounded to tenths so ten steps down and up land where they began
        let opacity = ((opacity * 10.0).round() / 10.0).clamp(MIN_OPACITY, 1.0);
        if opacity == self.opacity {
            return Vec::new();
        }
        self.opacity = opacity;
        vec![PinEffect::Redraw]
    }
}

fn sane_scale(scale: f64) -> f64 {
    if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    }
}

/// The image turned a quarter clockwise.
pub fn rotate_right(image: &CapturedImage) -> CapturedImage {
    rotate(image, |x, y, _width, height| (height - 1 - y, x))
}

/// The image turned a quarter anticlockwise.
pub fn rotate_left(image: &CapturedImage) -> CapturedImage {
    rotate(image, |x, y, width, _height| (y, width - 1 - x))
}

/// `target(x, y, width, height)` is where source pixel (x, y) lands in the
/// rotated image, which is `height` wide.
fn rotate(
    image: &CapturedImage,
    target: impl Fn(u32, u32, u32, u32) -> (u32, u32),
) -> CapturedImage {
    let (width, height) = (image.width, image.height);
    let mut bgra = vec![0u8; image.bgra.len()];
    for y in 0..height {
        for x in 0..width {
            let (tx, ty) = target(x, y, width, height);
            let from = (y as usize * width as usize + x as usize) * 4;
            let to = (ty as usize * height as usize + tx as usize) * 4;
            if let (Some(source), Some(dest)) =
                (image.bgra.get(from..from + 4), bgra.get_mut(to..to + 4))
            {
                dest.copy_from_slice(source);
            }
        }
    }
    CapturedImage {
        width: height,
        height: width,
        bgra,
    }
}

/// The pin window's pixels: premultiplied BGRA, `window_size()` large, the
/// image in the middle and a glow of `accent` (`hover_accent` while
/// hovered) in the frame, as 15.0.0's drop shadow.
pub fn render_pin(state: &PinState, accent: (u8, u8, u8), hover_accent: (u8, u8, u8)) -> Vec<u8> {
    let (window_width, window_height) = state.window_size();
    let (width, height) = (window_width.max(0) as usize, window_height.max(0) as usize);
    let mut pixels = vec![0u8; width * height * 4];
    let margin = state.margin();
    let (display_width, display_height) = state.display_size();
    let (right, bottom) = (
        margin + display_width as i32,
        margin + display_height as i32,
    );
    let color = if state.is_hovered() {
        hover_accent
    } else {
        accent
    };

    // The glow: strongest at the image's edge, gone at the window's
    if margin > 0 {
        for y in 0..height as i32 {
            for x in 0..width as i32 {
                let dx = (margin - x).max(x - (right - 1)).max(0);
                let dy = (margin - y).max(y - (bottom - 1)).max(0);
                if dx == 0 && dy == 0 {
                    continue;
                }
                let distance = f64::from(dx * dx + dy * dy).sqrt();
                let fade = (1.0 - distance / f64::from(margin)).max(0.0);
                let alpha = GLOW_ALPHA * fade * fade;
                let premultiply = |channel: u8| (f64::from(channel) * alpha).round() as u8;
                let at = (y as usize * width + x as usize) * 4;
                if let Some(pixel) = pixels.get_mut(at..at + 4) {
                    pixel.copy_from_slice(&[
                        premultiply(color.2),
                        premultiply(color.1),
                        premultiply(color.0),
                        (alpha * 255.0).round() as u8,
                    ]);
                }
            }
        }
    }

    let image = state.image();
    let scaled = scale_image(image, display_width, display_height, state.is_smooth());
    let row = display_width as usize * 4;
    for y in 0..display_height as usize {
        let at = ((margin as usize + y) * width + margin as usize) * 4;
        if let (Some(dest), Some(source)) = (
            pixels.get_mut(at..at + row),
            scaled.get(y * row..(y + 1) * row),
        ) {
            dest.copy_from_slice(source);
        }
    }
    pixels
}

/// `image` resized to `width` x `height`, opaque BGRA: bilinear when
/// `smooth`, nearest neighbour otherwise; a copy at its own size.
pub fn scale_image(image: &CapturedImage, width: u32, height: u32, smooth: bool) -> Vec<u8> {
    if width == image.width && height == image.height {
        let mut copy = image.bgra.clone();
        for pixel in copy.as_chunks_mut::<4>().0 {
            pixel[3] = 255;
        }
        return copy;
    }
    let (source_width, source_height) = (image.width.max(1), image.height.max(1));
    let pixel = |x: u32, y: u32| -> [f32; 3] {
        let at = (y.min(source_height - 1) as usize * image.width as usize
            + x.min(source_width - 1) as usize)
            * 4;
        image.bgra.get(at..at + 3).map_or([0.0; 3], |p| {
            [f32::from(p[0]), f32::from(p[1]), f32::from(p[2])]
        })
    };
    let step_x = source_width as f32 / width.max(1) as f32;
    let step_y = source_height as f32 / height.max(1) as f32;
    let mut out = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height {
        let sy = ((y as f32 + 0.5) * step_y - 0.5).max(0.0);
        for x in 0..width {
            let sx = ((x as f32 + 0.5) * step_x - 0.5).max(0.0);
            let color = if smooth {
                let (x0, y0) = (sx.floor() as u32, sy.floor() as u32);
                let (fx, fy) = (sx - x0 as f32, sy - y0 as f32);
                let (a, b) = (pixel(x0, y0), pixel(x0 + 1, y0));
                let (c, d) = (pixel(x0, y0 + 1), pixel(x0 + 1, y0 + 1));
                let mut mixed = [0.0f32; 3];
                for (channel, value) in mixed.iter_mut().enumerate() {
                    let top = a[channel] + (b[channel] - a[channel]) * fx;
                    let lower = c[channel] + (d[channel] - c[channel]) * fx;
                    *value = top + (lower - top) * fy;
                }
                mixed
            } else {
                pixel(
                    ((x as f32 + 0.5) * step_x) as u32,
                    ((y as f32 + 0.5) * step_y) as u32,
                )
            };
            out.extend_from_slice(&[
                color[0].round() as u8,
                color[1].round() as u8,
                color[2].round() as u8,
                255,
            ]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `width` x `height` image whose pixel (x, y) is (x, y, 0).
    fn gradient(width: u32, height: u32) -> CapturedImage {
        let mut bgra = Vec::new();
        for y in 0..height {
            for x in 0..width {
                bgra.extend_from_slice(&[x as u8, y as u8, 0, 255]);
            }
        }
        CapturedImage {
            width,
            height,
            bgra,
        }
    }

    fn at(image: &CapturedImage, x: u32, y: u32) -> [u8; 2] {
        let i = ((y * image.width + x) * 4) as usize;
        [image.bgra[i], image.bgra[i + 1]]
    }

    #[test]
    fn opens_at_capture_size_with_a_frame_scaled_to_the_monitor() {
        let pin = PinState::new(gradient(300, 200), 1.5, true);
        assert_eq!(pin.display_size(), (300, 200));
        assert_eq!(pin.margin(), 12);
        assert_eq!(pin.window_size(), (324, 224));
        assert_eq!(pin.alpha(), 255);
    }

    #[test]
    fn the_wheel_zooms_in_steps_of_the_original_size() {
        let mut pin = PinState::new(gradient(1000, 500), 1.0, true);
        assert_eq!(
            pin.handle(PinEvent::Wheel {
                delta: 240,
                ctrl: false
            }),
            [PinEffect::Redraw]
        );
        assert!((pin.zoom() - 1.06).abs() < 1e-9);
        assert_eq!(pin.display_size(), (1060, 530));
        pin.handle(PinEvent::Wheel {
            delta: -120,
            ctrl: false,
        });
        assert!((pin.zoom() - 1.03).abs() < 1e-9);
    }

    #[test]
    fn zooming_out_stops_at_the_minimum_side() {
        let mut pin = PinState::new(gradient(400, 200), 2.0, true);
        for _ in 0..100 {
            pin.handle(PinEvent::Wheel {
                delta: -120,
                ctrl: false,
            });
        }
        // 100 logical pixels at 200 % on the longer side
        assert_eq!(pin.display_size(), (200, 100));
        assert!(
            pin.handle(PinEvent::Wheel {
                delta: -120,
                ctrl: false
            })
            .is_empty()
        );
        // An image already smaller than that does not shrink at all
        let mut small = PinState::new(gradient(50, 40), 1.0, true);
        assert!(
            small
                .handle(PinEvent::Wheel {
                    delta: -120,
                    ctrl: false
                })
                .is_empty()
        );
    }

    #[test]
    fn zooming_in_stops_at_the_maximum() {
        let mut pin = PinState::new(gradient(10, 10), 1.0, true);
        pin.handle(PinEvent::Wheel {
            delta: 120 * 1000,
            ctrl: false,
        });
        assert_eq!(pin.zoom(), MAX_ZOOM);
    }

    #[test]
    fn ctrl_wheel_digits_and_the_menu_set_the_opacity() {
        let mut pin = PinState::new(gradient(10, 10), 1.0, true);
        pin.handle(PinEvent::Wheel {
            delta: -120,
            ctrl: true,
        });
        assert_eq!(pin.opacity(), 0.9);
        assert_eq!(pin.zoom(), 1.0);
        pin.handle(PinEvent::Key(PinKey::Digit(3)));
        assert_eq!(pin.opacity(), 0.3);
        pin.handle(PinEvent::Menu(PinMenuItem::IncreaseOpacity));
        assert_eq!(pin.opacity(), 0.4);
        for _ in 0..10 {
            pin.handle(PinEvent::Menu(PinMenuItem::DecreaseOpacity));
        }
        assert_eq!(pin.opacity(), MIN_OPACITY);
        pin.handle(PinEvent::Key(PinKey::Digit(0)));
        assert_eq!(pin.opacity(), 1.0);
        assert!(
            pin.handle(PinEvent::Menu(PinMenuItem::IncreaseOpacity))
                .is_empty()
        );
        pin.handle(PinEvent::Key(PinKey::Digit(5)));
        assert_eq!(pin.alpha(), 128);
    }

    #[test]
    fn escape_ctrl_q_double_click_and_close_end_the_pin() {
        for event in [
            PinEvent::Key(PinKey::Escape),
            PinEvent::Key(PinKey::Quit),
            PinEvent::DoubleClick,
            PinEvent::Menu(PinMenuItem::Close),
        ] {
            let mut pin = PinState::new(gradient(10, 10), 1.0, true);
            assert_eq!(pin.handle(event), [PinEffect::Close], "{event:?}");
        }
    }

    #[test]
    fn copy_save_and_editor_are_requests() {
        let mut pin = PinState::new(gradient(10, 10), 1.0, true);
        assert_eq!(
            pin.handle(PinEvent::Menu(PinMenuItem::CopyToClipboard)),
            [PinEffect::Request(PinRequest::Copy)]
        );
        assert_eq!(
            pin.handle(PinEvent::Menu(PinMenuItem::SaveToFile)),
            [PinEffect::Request(PinRequest::Save)]
        );
        assert_eq!(
            pin.handle(PinEvent::Menu(PinMenuItem::LoadIntoEditor)),
            [PinEffect::Request(PinRequest::OpenInEditor)]
        );
    }

    #[test]
    fn the_menu_matches_15_0_0() {
        let labels: Vec<&str> = PIN_MENU
            .iter()
            .map(|entry| entry.map_or("-", PinMenuItem::label))
            .collect();
        assert_eq!(
            labels,
            [
                "Copy to clipboard",
                "Save to file",
                "Load into editor",
                "-",
                "Rotate Right",
                "Rotate Left",
                "Increase Opacity",
                "Decrease Opacity",
                "-",
                "Close"
            ]
        );
        for item in PIN_MENU.iter().flatten() {
            assert_eq!(PinMenuItem::from_id(item.id()), Some(*item));
        }
        assert_eq!(PinMenuItem::from_id(0), None);
        assert_eq!(PinMenuItem::from_id(4), None);
        assert_eq!(PinMenuItem::from_id(99), None);
    }

    #[test]
    fn rotation_turns_the_pixels() {
        let image = gradient(3, 2);
        let right = rotate_right(&image);
        assert_eq!((right.width, right.height), (2, 3));
        // The bottom-left pixel goes to the top-left
        assert_eq!(at(&right, 0, 0), [0, 1]);
        assert_eq!(at(&right, 1, 0), [0, 0]);
        assert_eq!(at(&right, 0, 2), [2, 1]);
        let left = rotate_left(&image);
        // The top-right pixel goes to the top-left
        assert_eq!(at(&left, 0, 0), [2, 0]);
        assert_eq!(at(&left, 1, 2), [0, 1]);
        assert_eq!(rotate_left(&right), image);

        let mut pin = PinState::new(image, 1.0, true);
        pin.handle(PinEvent::Menu(PinMenuItem::RotateRight));
        assert_eq!(pin.display_size(), (2, 3));
    }

    #[test]
    fn hover_and_monitor_changes_redraw() {
        let mut pin = PinState::new(gradient(400, 400), 1.0, true);
        assert_eq!(pin.handle(PinEvent::Hover(true)), [PinEffect::Redraw]);
        assert!(pin.handle(PinEvent::Hover(true)).is_empty());
        assert_eq!(pin.handle(PinEvent::ScaleChanged(2.0)), [PinEffect::Redraw]);
        assert_eq!(pin.margin(), 16);
        // The image stays pixel for pixel
        assert_eq!(pin.display_size(), (400, 400));
    }

    #[test]
    fn rendering_puts_the_image_inside_a_fading_glow() {
        let pin = PinState::new(gradient(4, 4), 1.0, true);
        let pixels = render_pin(&pin, (255, 0, 0), (0, 0, 255));
        let (width, _) = pin.window_size();
        let pixel = |x: i32, y: i32| {
            let i = ((y * width + x) * 4) as usize;
            [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
        };
        // Image pixel (1, 2) sits at (9, 10), opaque and unscaled
        assert_eq!(pixel(9, 10), [1, 2, 0, 255]);
        // Next to the image: the accent, premultiplied, half strength
        let edge = pixel(7, 10);
        assert_eq!(edge[0], 0);
        assert!(edge[2] > 80 && edge[2] == edge[3], "{edge:?}");
        // The window's corner is clear
        assert_eq!(pixel(0, 0), [0, 0, 0, 0]);

        let mut hovered = pin.clone();
        hovered.handle(PinEvent::Hover(true));
        let pixels = render_pin(&hovered, (255, 0, 0), (0, 0, 255));
        let i = ((10 * width + 7) * 4) as usize;
        assert!(pixels[i] > 80 && pixels[i + 2] == 0);
    }

    #[test]
    fn scaling_is_exact_at_one_and_smooth_or_blocky_when_zoomed() {
        let image = gradient(2, 1);
        assert_eq!(scale_image(&image, 2, 1, true), image.bgra);
        let blocky = scale_image(&image, 4, 1, false);
        assert_eq!(
            blocky.chunks(4).map(|p| p[0]).collect::<Vec<_>>(),
            [0, 0, 1, 1]
        );
        let mut wide = gradient(2, 1);
        wide.bgra[4] = 200;
        let smooth = scale_image(&wide, 4, 1, true);
        let blues: Vec<u8> = smooth.chunks(4).map(|p| p[0]).collect();
        assert_eq!(blues[0], 0);
        assert_eq!(blues[3], 200);
        assert!(
            blues[1] > 0 && blues[1] < blues[2] && blues[2] < 200,
            "{blues:?}"
        );
    }
}
