//! A frozen screenshot.

use crate::geometry::PhysicalRect;

/// Bytes per pixel of a [`Frame`].
pub(crate) const BYTES_PER_PIXEL: usize = 4;

/// Pixels grabbed from the screen: exactly one buffer pixel per physical
/// screen pixel, never scaled.
///
/// The buffer is 8-bit BGRA (the byte order GDI and Windows bitmaps use),
/// top row first, rows packed without padding, alpha always 255.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    bounds: PhysicalRect,
    bgra: Vec<u8>,
}

impl Frame {
    /// `bgra` must hold exactly `bounds.width * bounds.height` pixels.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub(crate) fn new(bounds: PhysicalRect, bgra: Vec<u8>) -> Self {
        debug_assert_eq!(
            bgra.len(),
            bounds.width.max(0) as usize * bounds.height.max(0) as usize * BYTES_PER_PIXEL
        );
        Self { bounds, bgra }
    }

    /// The area of the virtual desktop this frame shows. Buffer pixel (0,0)
    /// is the desktop pixel at this rectangle's top-left.
    pub fn bounds(&self) -> PhysicalRect {
        self.bounds
    }

    pub fn width(&self) -> u32 {
        self.bounds.width.max(0) as u32
    }

    pub fn height(&self) -> u32 {
        self.bounds.height.max(0) as u32
    }

    pub fn bgra(&self) -> &[u8] {
        &self.bgra
    }

    pub fn into_bgra(self) -> Vec<u8> {
        self.bgra
    }

    /// The pixels as 8-bit RGBA, for encoders and canvases that want that
    /// order.
    pub fn into_rgba(self) -> Vec<u8> {
        let mut pixels = self.bgra;
        for pixel in pixels.as_chunks_mut::<BYTES_PER_PIXEL>().0 {
            pixel.swap(0, 2);
        }
        pixels
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba_swaps_blue_and_red_only() {
        let frame = Frame::new(
            PhysicalRect::new(-2, 5, 2, 1),
            vec![1, 2, 3, 255, 10, 20, 30, 255],
        );
        assert_eq!((frame.width(), frame.height()), (2, 1));
        assert_eq!(frame.into_rgba(), [3, 2, 1, 255, 30, 20, 10, 255]);
    }
}
