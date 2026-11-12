//! Fonts supplied by the caller, and the text layout built on them.
//!
//! This crate never looks a font up on the system: the overlay and the editor
//! hand over the same font bytes, which is what makes their text identical.
//! Glyph outlines go through the same rasteriser as every other shape.

use tiny_skia::PathBuilder;
use ttf_parser::{Face, GlyphId, OutlineBuilder};

use crate::error::FontError;
use crate::geometry::{Point, Rect};
use crate::tool::TextFont;

/// Blank space kept between the glyphs and the edge of a text object's area,
/// which is both its selection outline and its grabbable area.
pub(crate) const TEXT_PADDING: i32 = 5;

/// Tool size 0 is an 8 pt font; each step adds one point.
const BASE_POINT_SIZE: u32 = 8;

/// Font height in image pixels for a text object of the given tool size.
///
/// The old tool set a point size of `size + 8`, which is 96/72 pixels per
/// point at a display scale of 1.
pub fn text_pixel_size(size: u32) -> f64 {
    f64::from(size.saturating_add(BASE_POINT_SIZE)) * 96.0 / 72.0
}

struct FontEntry {
    family: String,
    bold: bool,
    italic: bool,
    data: Vec<u8>,
}

/// The fonts available to text objects and counter bubbles.
///
/// The first family added is the default: it serves text whose family is
/// empty or not registered, and the numbers in counter bubbles.
#[derive(Default)]
pub struct FontLibrary {
    entries: Vec<FontEntry>,
}

impl FontLibrary {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers one face (TrueType or OpenType bytes) under a family name
    /// and style.
    pub fn add(
        &mut self,
        family: impl Into<String>,
        bold: bool,
        italic: bool,
        data: Vec<u8>,
    ) -> Result<(), FontError> {
        let family = family.into();
        if let Err(source) = Face::parse(&data, 0) {
            return Err(FontError::Parse {
                family,
                reason: source.to_string(),
            });
        }
        self.entries.push(FontEntry {
            family,
            bold,
            italic,
            data,
        });
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The closest registered face: the requested family if present,
    /// otherwise the default family; within it, the nearest style.
    fn face(&self, family: &str, bold: bool, italic: bool) -> Option<Face<'_>> {
        let default_family = self.entries.first()?.family.as_str();
        let wanted = if self
            .entries
            .iter()
            .any(|e| e.family.eq_ignore_ascii_case(family))
        {
            family
        } else {
            default_family
        };
        let entry = self
            .entries
            .iter()
            .filter(|e| e.family.eq_ignore_ascii_case(wanted))
            // A wrong weight is a bigger miss than a wrong slant.
            .min_by_key(|e| u8::from(e.bold != bold) * 2 + u8::from(e.italic != italic))?;
        // Parsed once in `add`, so this cannot fail; `ok()` avoids a panic
        // path all the same.
        Face::parse(&entry.data, 0).ok()
    }

    pub(crate) fn scaled(&self, font: &TextFont, pixel_size: f64) -> Option<ScaledFace<'_>> {
        self.face(&font.family, font.bold, font.italic)
            .map(|face| ScaledFace::new(face, pixel_size))
    }

    /// The bold face of the default family, for counter bubbles.
    pub(crate) fn scaled_default_bold(&self, pixel_size: f64) -> Option<ScaledFace<'_>> {
        self.face("", true, false)
            .map(|face| ScaledFace::new(face, pixel_size))
    }
}

/// A horizontal bar decorating a line of text.
pub(crate) enum Decoration {
    Underline,
    Strikeout,
}

/// A face at a pixel size, with the integer metrics text layout uses.
pub(crate) struct ScaledFace<'a> {
    face: Face<'a>,
    scale: f64,
    pixel_size: f64,
    /// Baseline to the top of a line.
    pub(crate) ascent: i32,
    /// Baseline to the bottom of a line, positive.
    pub(crate) descent: i32,
    /// Extra gap between consecutive lines.
    pub(crate) leading: i32,
}

impl<'a> ScaledFace<'a> {
    fn new(face: Face<'a>, pixel_size: f64) -> Self {
        let scale = pixel_size / f64::from(face.units_per_em());
        let scaled = |units: i16| (f64::from(units) * scale).round() as i32;
        Self {
            ascent: scaled(face.ascender()).max(0),
            descent: (-scaled(face.descender())).max(0),
            leading: scaled(face.line_gap()).max(0),
            face,
            scale,
            pixel_size,
        }
    }

    pub(crate) fn line_height(&self) -> i32 {
        self.ascent.saturating_add(self.descent)
    }

    fn glyph(&self, character: char) -> GlyphId {
        // Glyph 0 is the font's "missing glyph" box.
        self.face.glyph_index(character).unwrap_or(GlyphId(0))
    }

    /// Advance width of one line, in pixels. No kerning and no shaping:
    /// glyphs are placed by their own advances.
    pub(crate) fn line_width(&self, line: &str) -> f64 {
        line.chars()
            .map(|c| f64::from(self.face.glyph_hor_advance(self.glyph(c)).unwrap_or(0)))
            .sum::<f64>()
            * self.scale
    }

    /// Width and height of the block of lines, as whole pixels.
    pub(crate) fn extent(&self, text: &str) -> (i32, i32) {
        let mut widest = 0.0_f64;
        let mut count = 0_i32;
        for line in lines(text) {
            widest = widest.max(self.line_width(line));
            count = count.saturating_add(1);
        }
        let height = count
            .saturating_mul(self.line_height())
            .saturating_add((count - 1).max(0).saturating_mul(self.leading));
        (widest.ceil() as i32, height)
    }

    /// Distance from one baseline to the next.
    pub(crate) fn line_advance(&self) -> i32 {
        self.line_height().saturating_add(self.leading)
    }

    /// Appends the outlines of `line` with its origin at (`x`, `baseline`).
    pub(crate) fn push_line(&self, builder: &mut PathBuilder, line: &str, x: f64, baseline: f64) {
        let mut pen = x;
        for character in line.chars() {
            let glyph = self.glyph(character);
            let mut sink = GlyphSink {
                builder,
                scale: self.scale,
                x: pen,
                baseline,
            };
            // `None` is a glyph without an outline, such as a space.
            let _ = self.face.outline_glyph(glyph, &mut sink);
            pen += f64::from(self.face.glyph_hor_advance(glyph).unwrap_or(0)) * self.scale;
        }
    }

    /// Top edge (relative to the baseline, positive downwards) and thickness
    /// of a decoration bar.
    pub(crate) fn decoration(&self, decoration: Decoration) -> (f64, f64) {
        let metrics = match decoration {
            Decoration::Underline => self.face.underline_metrics(),
            Decoration::Strikeout => self.face.strikeout_metrics(),
        };
        match metrics {
            Some(m) => (
                -f64::from(m.position) * self.scale,
                (f64::from(m.thickness) * self.scale).max(1.0),
            ),
            // Fonts without the tables get bars in the customary places.
            None => {
                let thickness = (self.pixel_size / 14.0).max(1.0);
                match decoration {
                    Decoration::Underline => (self.pixel_size * 0.1, thickness),
                    Decoration::Strikeout => (-self.pixel_size * 0.3, thickness),
                }
            }
        }
    }
}

/// Lines as the old editor split them: on `\n`, with a trailing newline
/// opening one more (empty) line.
pub(crate) fn lines(text: &str) -> impl Iterator<Item = &str> {
    text.split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
}

struct GlyphSink<'b> {
    builder: &'b mut PathBuilder,
    scale: f64,
    x: f64,
    baseline: f64,
}

impl GlyphSink<'_> {
    /// Font units, y up, to image pixels, y down.
    fn map(&self, x: f32, y: f32) -> (f32, f32) {
        (
            (self.x + f64::from(x) * self.scale) as f32,
            (self.baseline - f64::from(y) * self.scale) as f32,
        )
    }
}

impl OutlineBuilder for GlyphSink<'_> {
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

/// The area a text object occupies: its glyph block plus the padding on
/// every side. `None` when the text is empty or no font is registered.
pub(crate) fn text_area(
    top_left: Point,
    text: &str,
    font: &TextFont,
    size: u32,
    fonts: &FontLibrary,
) -> Option<Rect> {
    if text.is_empty() {
        return None;
    }
    let face = fonts.scaled(font, text_pixel_size(size))?;
    let (width, height) = face.extent(text);
    let pad = i64::from(TEXT_PADDING) * 2;
    Some(Rect::from_edges(
        i64::from(top_left.x),
        i64::from(top_left.y),
        i64::from(top_left.x) + i64::from(width) + pad,
        i64::from(top_left.y) + i64::from(height) + pad,
    ))
}

/// Where a text object's `top_left` goes for a click at `anchor`.
///
/// The click is an insertion point, so the first line straddles it: the
/// glyphs start at the anchor's x, and the anchor's y is the middle of the
/// first line. `None` when no font is registered.
pub fn text_top_left_for_anchor(
    anchor: Point,
    font: &TextFont,
    size: u32,
    fonts: &FontLibrary,
) -> Option<Point> {
    let face = fonts.scaled(font, text_pixel_size(size))?;
    Some(anchor.translated(
        -TEXT_PADDING,
        (-TEXT_PADDING).saturating_sub(face.line_height() / 2),
    ))
}
