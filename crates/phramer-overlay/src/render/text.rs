//! Single lines of text from caller-supplied font bytes: shaping is one
//! glyph per character with the font's advances, which is all the labels and
//! the help message need.

use tiny_skia::{FillRule, Paint, PathBuilder, PixmapMut, Transform};
use ttf_parser::{Face, OutlineBuilder, Tag};

use crate::error::OverlayError;

/// `--weight-regular`, `--weight-medium`, `--weight-semibold`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Weight {
    Regular,
    Medium,
    Semibold,
}

impl Weight {
    fn value(self) -> f32 {
        match self {
            Weight::Regular => 400.0,
            Weight::Medium => 500.0,
            Weight::Semibold => 600.0,
        }
    }
}

/// A parsed font. With a variable font the weight axis is set per use; a
/// static font draws every weight the same.
pub(crate) struct Font<'a> {
    face: Face<'a>,
}

/// Vertical metrics of a line, in pixels.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LineMetrics {
    pub(crate) ascent: f32,
    pub(crate) height: f32,
}

impl<'a> Font<'a> {
    pub(crate) fn parse(data: &'a [u8]) -> Result<Self, OverlayError> {
        let face = Face::parse(data, 0).map_err(|error| OverlayError::Font(error.to_string()))?;
        Ok(Self { face })
    }

    fn set_weight(&mut self, weight: Weight) {
        // A font without a weight axis keeps its one weight
        let _ = self
            .face
            .set_variation(Tag::from_bytes(b"wght"), weight.value());
    }

    fn scale(&self, size: f32) -> f32 {
        size / f32::from(self.face.units_per_em())
    }

    /// Ascent and line height at a pixel size, rounded to whole pixels so
    /// baselines land on the pixel grid.
    pub(crate) fn metrics(&self, size: f32) -> LineMetrics {
        let scale = self.scale(size);
        let ascent = f32::from(self.face.ascender()) * scale;
        let descent = -f32::from(self.face.descender()) * scale;
        LineMetrics {
            ascent: ascent.round(),
            height: (ascent + descent).round(),
        }
    }

    /// The advance width of `text` at a pixel size.
    pub(crate) fn measure(&mut self, text: &str, size: f32, weight: Weight) -> f32 {
        self.set_weight(weight);
        let scale = self.scale(size);
        text.chars()
            .map(|c| {
                let glyph = self.face.glyph_index(c).unwrap_or_default();
                f32::from(self.face.glyph_hor_advance(glyph).unwrap_or(0)) * scale
            })
            .sum()
    }

    /// Draws `text` with its baseline's left end at (x, baseline).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw(
        &mut self,
        canvas: &mut PixmapMut<'_>,
        text: &str,
        x: f32,
        baseline: f32,
        size: f32,
        weight: Weight,
        paint: &Paint<'_>,
    ) {
        if let Some(path) = self.outline(text, x, baseline, size, weight) {
            canvas.fill_path(&path, paint, FillRule::Winding, Transform::identity(), None);
        }
    }

    /// Draws `text` reading from bottom to top, its baseline's lower end at
    /// (x, y): the glyphs stand to the left of the baseline.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_vertical(
        &mut self,
        canvas: &mut PixmapMut<'_>,
        text: &str,
        x: f32,
        y: f32,
        size: f32,
        weight: Weight,
        paint: &Paint<'_>,
    ) {
        if let Some(path) = self.outline(text, 0.0, 0.0, size, weight) {
            let transform = Transform::from_rotate(-90.0).post_translate(x, y);
            canvas.fill_path(&path, paint, FillRule::Winding, transform, None);
        }
    }

    fn outline(
        &mut self,
        text: &str,
        x: f32,
        baseline: f32,
        size: f32,
        weight: Weight,
    ) -> Option<tiny_skia::Path> {
        self.set_weight(weight);
        let scale = self.scale(size);
        let mut sink = GlyphSink {
            builder: PathBuilder::new(),
            scale,
            x,
            y: baseline,
        };
        for c in text.chars() {
            let glyph = self.face.glyph_index(c).unwrap_or_default();
            // Whitespace has no outline, only an advance
            let _ = self.face.outline_glyph(glyph, &mut sink);
            sink.x += f32::from(self.face.glyph_hor_advance(glyph).unwrap_or(0)) * scale;
        }
        sink.builder.finish()
    }
}

/// Glyph outlines, font units to canvas pixels (y grows down).
struct GlyphSink {
    builder: PathBuilder,
    scale: f32,
    x: f32,
    y: f32,
}

impl GlyphSink {
    fn map(&self, x: f32, y: f32) -> (f32, f32) {
        (self.x + x * self.scale, self.y - y * self.scale)
    }
}

impl OutlineBuilder for GlyphSink {
    fn move_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.map(x, y);
        self.builder.move_to(x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.map(x, y);
        self.builder.line_to(x, y);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (x1, y1) = self.map(x1, y1);
        let (x, y) = self.map(x, y);
        self.builder.quad_to(x1, y1, x, y);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (x1, y1) = self.map(x1, y1);
        let (x2, y2) = self.map(x2, y2);
        let (x, y) = self.map(x, y);
        self.builder.cubic_to(x1, y1, x2, y2, x, y);
    }

    fn close(&mut self) {
        self.builder.close();
    }
}
