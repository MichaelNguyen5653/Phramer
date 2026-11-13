//! Colour grab and the magnifier: 15.0.0's `ColorGrabWidget` and
//! `MagnifierWidget`, as views the renderer draws.
//!
//! Both read the frozen screenshot, never the live screen: what is picked is
//! what the overlay shows. Their geometry is worked out here in image pixels
//! (the monitor's physical pixels), with each zoomed pixel a whole number of
//! them, so they are sharp at every scale.

use std::sync::Arc;

use phramer_annotate::{Color, Point};
use phramer_capture::LogicalPoint;

use super::{Drag, Effects, Millis, OverlayState};
use crate::error::OverlayError;
use crate::input::{Key, PointerButton};

/// The magnifier: eight pixels on each side of the pointer's
/// (`m_magPixels`), ten times larger (`magZoom`), sixteen pixels from the
/// pointer (`m_magOffset`).
const MAGNIFIER_REACH: i32 = 8;
const MAGNIFIER_ZOOM: f64 = 10.0;
const MAGNIFIER_OFFSET: f64 = 16.0;
/// Colour grab's preview: 77 pixels across at 11x, so seven screen pixels,
/// centred on the pointer (`WIDTH1`, `ZOOM1`).
const GRAB_REACH: i32 = 3;
const GRAB_ZOOM: f64 = 11.0;
/// Holding the click: 165 pixels at 15x, eleven screen pixels, where the
/// click was (`WIDTH2`, `ZOOM2`), after 250 ms.
const PRECISE_REACH: i32 = 5;
const PRECISE_ZOOM: f64 = 15.0;
const PRECISE_DELAY_MS: Millis = 250;

/// A monitor's frozen screenshot, for picking colours: 8-bit BGRA, top row
/// first, shared with whoever else holds it.
#[derive(Clone)]
pub struct Pixels {
    width: u32,
    height: u32,
    bgra: Arc<dyn AsRef<[u8]> + Send + Sync>,
}

impl std::fmt::Debug for Pixels {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Pixels({}x{})", self.width, self.height)
    }
}

impl Pixels {
    pub fn new(
        width: u32,
        height: u32,
        bgra: Arc<dyn AsRef<[u8]> + Send + Sync>,
    ) -> Result<Self, OverlayError> {
        let expected = (width as usize)
            .saturating_mul(height as usize)
            .saturating_mul(4);
        let actual = (*bgra).as_ref().len();
        if actual != expected {
            return Err(OverlayError::FrameSize {
                width,
                height,
                expected,
                actual,
            });
        }
        Ok(Self {
            width,
            height,
            bgra,
        })
    }

    /// The colour of one pixel; `None` off the screenshot.
    pub fn color_at(&self, point: Point) -> Option<Color> {
        let x = u32::try_from(point.x).ok().filter(|x| *x < self.width)?;
        let y = u32::try_from(point.y).ok().filter(|y| *y < self.height)?;
        let at = (y as usize * self.width as usize + x as usize) * 4;
        let pixel = (*self.bgra).as_ref().get(at..at + 3)?;
        Some(Color::rgb(pixel[2], pixel[1], pixel[0]))
    }
}

/// Round (the default) or square (`squareMagnifier`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MagnifierShape {
    Round,
    Square,
}

/// A zoomed square of screenshot pixels, in image pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Zoom {
    /// The square's top-left on the canvas.
    pub x: i32,
    pub y: i32,
    /// A screenshot pixel's side on the canvas.
    pub cell: i32,
    /// Screenshot pixels across and down.
    pub cells: i32,
    /// The screenshot pixel in the top-left cell; pixels off the
    /// screenshot show black.
    pub source: Point,
    /// The cell (column, row) of the pixel at the pointer.
    pub center: (i32, i32),
}

impl Zoom {
    /// The square's side on the canvas.
    pub fn side(&self) -> i32 {
        self.cell * self.cells
    }

    /// The screenshot pixel under canvas point `at`, when it is inside.
    fn pixel_at(&self, at: Point) -> Option<Point> {
        let (dx, dy) = (at.x - self.x, at.y - self.y);
        let side = self.side();
        ((0..side).contains(&dx) && (0..side).contains(&dy)).then(|| {
            Point::new(
                self.source.x + dx / self.cell,
                self.source.y + dy / self.cell,
            )
        })
    }
}

/// The magnifier as drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MagnifierView {
    pub shape: MagnifierShape,
    pub zoom: Zoom,
}

/// Colour grab as drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GrabView {
    /// The preview at the pointer, the larger one while a click is held,
    /// or none when Space or the right button turned it off.
    pub zoom: Option<Zoom>,
    /// The larger view of a held click is showing.
    pub precise: bool,
    /// The colour that Enter or a click would take.
    pub color: Color,
    /// The pointer, in image pixels.
    pub pointer: Point,
}

/// Colour grab in progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Grab {
    /// The colour before, for Esc.
    original: Color,
    /// The preview shows (`m_magnifierActive`).
    zoom: bool,
    /// A press arrived since grabbing began (`m_mousePressReceived`).
    pressed: bool,
    /// A held left click: where, and since when (`m_extraZoomActive`).
    precise: Option<(Point, Millis)>,
}

/// The key list colour grab shows (`OverlayMessage::pushKeyMap`).
pub const GRAB_KEYS: [(&str, &str); 4] = [
    ("Enter or Left Click", "Accept color"),
    ("Hold Left Click", "Precisely select color"),
    ("Space or Right Click", "Toggle magnifier"),
    ("Esc", "Cancel"),
];

/// A zoom of `reach` pixels each side of `pixel` at `zoom` logical pixels
/// each, its middle cell centred on canvas point `at`.
fn centred_zoom(pixel: Point, at: Point, reach: i32, zoom: f64, scale: f64) -> Zoom {
    let cell = (zoom * scale).round().max(1.0) as i32;
    Zoom {
        x: at.x - reach * cell - cell / 2,
        y: at.y - reach * cell - cell / 2,
        cell,
        cells: 2 * reach + 1,
        source: Point::new(pixel.x - reach, pixel.y - reach),
        center: (reach, reach),
    }
}

impl OverlayState {
    /// Gives the overlay a monitor's frozen screenshot to pick colours
    /// from. Without one, colour grab picks nothing.
    pub fn set_pixels(&mut self, monitor: usize, pixels: Pixels) {
        if let Some(slot) = self.pixels.get_mut(monitor) {
            *slot = Some(pixels);
        }
    }

    /// Where the pointer last was on the active monitor.
    pub fn pointer(&self) -> Option<LogicalPoint> {
        self.pointer
    }

    /// Whether colour grab is on.
    pub fn is_grabbing_color(&self) -> bool {
        self.grab.is_some()
    }

    /// The screenshot pixel's colour at `at`.
    fn pixel_color(&self, pixel: Point) -> Option<Color> {
        self.pixels.get(self.active)?.as_ref()?.color_at(pixel)
    }

    fn precise_zoom(&self, grab: &Grab) -> Option<Zoom> {
        let (pixel, since) = grab.precise?;
        if self.now < since + PRECISE_DELAY_MS {
            return None;
        }
        Some(centred_zoom(
            pixel,
            pixel,
            PRECISE_REACH,
            PRECISE_ZOOM,
            self.scale(),
        ))
    }

    /// What colour grab picks at `at`: inside the larger view, the pixel
    /// drawn under the pointer; elsewhere the one at the pointer.
    fn grab_pixel(&self, grab: &Grab, at: LogicalPoint) -> Point {
        let point = self.image_point(at);
        self.precise_zoom(grab)
            .and_then(|zoom| zoom.pixel_at(point))
            .unwrap_or(point)
    }

    /// Colour grab as drawn.
    pub fn color_grab(&self) -> Option<GrabView> {
        let grab = self.grab.as_ref()?;
        let at = self.pointer?;
        let pointer = self.image_point(at);
        let precise = self.precise_zoom(grab);
        let zoom = precise.or_else(|| {
            grab.zoom
                .then(|| centred_zoom(pointer, pointer, GRAB_REACH, GRAB_ZOOM, self.scale()))
        });
        Some(GrabView {
            zoom,
            precise: precise.is_some(),
            color: self.tools.color,
            pointer,
        })
    }

    /// The magnifier, when it shows: `showMagnifier` on, no tool selected
    /// (15.0.0 showed it for any pointer move without one), and the pointer
    /// not over the toolbar, the panel or the colour wheel.
    pub fn magnifier(&self) -> Option<MagnifierView> {
        if !self.config.show_magnifier
            || self.grab.is_some()
            || self.tools.active.is_some()
            || self.tools.wheel.is_some()
            || matches!(self.drag, Drag::Button(_) | Drag::Panel)
        {
            return None;
        }
        let at = self.pointer?;
        if !self.monitor_rect().contains(at) || self.over_toolbar(at) || self.over_panel(at) {
            return None;
        }
        let scale = self.scale();
        let pixel = self.image_point(at);
        let cell = (MAGNIFIER_ZOOM * scale).round().max(1.0) as i32;
        let cells = 2 * MAGNIFIER_REACH + 1;
        let side = cell * cells;
        let bounds = self.monitors[self.active].bounds();
        let (width, height) = (bounds.width, bounds.height);
        // Below and to the right of the pointer; the other side where that
        // runs off the monitor, and kept whole on it
        let offset = (MAGNIFIER_OFFSET * scale).round() as i32;
        let place = |pointer: i32, extent: i32| {
            let after = pointer + offset;
            let start = if after + side > extent {
                pointer - offset - side
            } else {
                after
            };
            start.clamp(0, (extent - side).max(0))
        };
        let shape = if self.config.square_magnifier {
            MagnifierShape::Square
        } else {
            MagnifierShape::Round
        };
        // The square one keeps its pixels on the screenshot and moves the
        // crosshair instead; the round one shows black past the edge
        let clamp = |p: i32, extent: i32| match shape {
            MagnifierShape::Square => (p - MAGNIFIER_REACH).clamp(0, (extent - cells).max(0)),
            MagnifierShape::Round => p - MAGNIFIER_REACH,
        };
        let source = Point::new(clamp(pixel.x, width), clamp(pixel.y, height));
        Some(MagnifierView {
            shape,
            zoom: Zoom {
                x: place(pixel.x, width),
                y: place(pixel.y, height),
                cell,
                cells,
                source,
                center: (pixel.x - source.x, pixel.y - source.y),
            },
        })
    }

    // --- Colour grab ---------------------------------------------------------

    /// Alt+G or Grab Color (`SidePanelWidget::startColorGrab`): the panel
    /// steps aside and the pointer picks from the screenshot.
    pub(crate) fn start_grab(&mut self, effects: &mut Effects) {
        if self.grab.is_some() {
            return;
        }
        self.commit_text(effects);
        self.finish_hex(effects);
        self.wheel_cancel(effects);
        self.tools.shape_menu = false;
        let original = self.tools.color;
        // Hidden at once: it comes back when grabbing ends
        self.panel.open = false;
        self.panel.since = None;
        self.panel.held = None;
        self.drag = Drag::Idle;
        self.grab = Some(Grab {
            original,
            zoom: true,
            pressed: false,
            precise: None,
        });
        if let Some(at) = self.pointer {
            self.grab_hover(at, effects);
        }
        self.update_cursor(self.pointer, effects);
        effects.redraw(self.active);
    }

    /// The pointer moved while grabbing: the colour under it shows at once.
    pub(crate) fn grab_hover(&mut self, at: LogicalPoint, effects: &mut Effects) {
        let Some(mut grab) = self.grab else {
            return;
        };
        if let Some(zoom) = self.precise_zoom(&grab)
            && zoom.pixel_at(self.image_point(at)).is_none()
        {
            // Leaving the larger view lets the click go
            grab.precise = None;
            self.grab = Some(grab);
        }
        if let Some(color) = self.pixel_color(self.grab_pixel(&grab, at)) {
            self.tools.color = color;
        }
        effects.redraw(self.active);
    }

    pub(crate) fn grab_press(
        &mut self,
        at: LogicalPoint,
        button: PointerButton,
        effects: &mut Effects,
    ) {
        let pixel = self.image_point(at);
        let now = self.now;
        let Some(grab) = self.grab.as_mut() else {
            return;
        };
        grab.pressed = true;
        match button {
            PointerButton::Right => grab.zoom = !grab.zoom,
            PointerButton::Left => grab.precise = Some((pixel, now)),
            PointerButton::Middle => {}
        }
        effects.redraw(self.active);
    }

    pub(crate) fn grab_release(
        &mut self,
        at: LogicalPoint,
        button: PointerButton,
        effects: &mut Effects,
    ) {
        let Some(grab) = self.grab else {
            return;
        };
        // The release of the press that started grabbing is not a pick
        if !grab.pressed || button != PointerButton::Left || grab.precise.is_none() {
            return;
        }
        if let Some(color) = self.pixel_color(self.grab_pixel(&grab, at)) {
            self.tools.color = color;
        }
        self.end_grab(true, effects);
    }

    /// Keys while grabbing: Esc cancels, Enter takes the colour, Space
    /// turns the preview on and off. Every other key is swallowed.
    pub(crate) fn grab_key(&mut self, key: Key, effects: &mut Effects) {
        match key {
            Key::Escape => self.end_grab(false, effects),
            Key::Return | Key::Enter => self.end_grab(true, effects),
            Key::Space => {
                if let Some(grab) = self.grab.as_mut()
                    && grab.precise.is_none()
                {
                    grab.zoom = !grab.zoom;
                    effects.redraw(self.active);
                }
            }
            _ => {}
        }
    }

    /// Ends grabbing: the colour is taken (saved, and given to the selected
    /// object) or the old one comes back. The panel shows again either way
    /// (`SidePanelWidget::finalizeGrab`).
    fn end_grab(&mut self, accept: bool, effects: &mut Effects) {
        let Some(grab) = self.grab.take() else {
            return;
        };
        if accept {
            let color = self.tools.color;
            self.apply_color(color, effects);
        } else {
            self.tools.color = grab.original;
        }
        self.open_panel(effects);
        self.update_cursor(self.pointer, effects);
        effects.redraw(self.active);
    }

    /// When the larger view of a held click is due, for a wake-up.
    pub(crate) fn grab_wake(&self) -> Option<Millis> {
        let (_, since) = self.grab?.precise?;
        let due = since + PRECISE_DELAY_MS;
        (self.now < due).then_some(due)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_zoom_maps_canvas_points_to_their_pixels() {
        let zoom = centred_zoom(Point::new(100, 50), Point::new(400, 300), 3, 11.0, 1.5);
        // 16.5 rounds up: 17 canvas pixels per screenshot pixel
        assert_eq!(zoom.cell, 17);
        assert_eq!(zoom.side(), 119);
        assert_eq!(
            zoom.pixel_at(Point::new(400, 300)),
            Some(Point::new(100, 50))
        );
        assert_eq!(
            zoom.pixel_at(Point::new(400 + 17, 300)),
            Some(Point::new(101, 50))
        );
        assert_eq!(
            zoom.pixel_at(Point::new(zoom.x, zoom.y)),
            Some(Point::new(97, 47))
        );
        assert_eq!(zoom.pixel_at(Point::new(zoom.x - 1, zoom.y)), None);
    }

    #[test]
    fn pixels_read_bgra_and_stop_at_the_edges() {
        let pixels = Pixels::new(2, 1, Arc::new(vec![1, 2, 3, 255, 10, 20, 30, 255]))
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            pixels.color_at(Point::new(1, 0)),
            Some(Color::rgb(30, 20, 10))
        );
        assert_eq!(pixels.color_at(Point::new(2, 0)), None);
        assert_eq!(pixels.color_at(Point::new(-1, 0)), None);
        assert!(Pixels::new(3, 1, Arc::new(vec![0u8; 8])).is_err());
    }
}
