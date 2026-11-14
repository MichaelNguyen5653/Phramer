//! The CPU renderer: a document and its base image to RGBA8 pixels.
//!
//! Built on `tiny-skia` with its scalar pipeline only, so the overlay
//! (native) and the editor (wasm32) produce the same bytes.

use tiny_skia::{
    BlendMode, Color as SkColor, FillRule, FilterQuality, LineCap, LineJoin, Paint, Path,
    PathBuilder, Pixmap, PixmapPaint, Rect as SkRect, Stroke, Transform,
};

use crate::arrow::{HeadSegment, arrow_geometry};
use crate::color::Color;
use crate::document::Document;
use crate::effects::{self, Readable, Region};
use crate::error::RenderError;
use crate::font::{Decoration, FontLibrary, TEXT_PADDING, lines, text_pixel_size};
use crate::geometry::{Point, PointF, Rect, RectF};
use crate::outline::{counter_wedge, dragged_area, filled_area};
use crate::tool::{
    Annotation, ArrowHead, COUNTER_RING, RedactStyle, Shape, ShapeStyle, TextAlign, TextFont,
    counter_radius,
};

/// Highlight compositing: multiply, at this opacity applied to the whole
/// drawn highlight (the painter), not to the colour's alpha. The two differ
/// under multiply, so overlaps keep the look they always had.
const HIGHLIGHT_OPACITY: f32 = 0.35;

/// Qt's default miter limit, used for square outline corners.
const MITER_LIMIT: f32 = 2.0;

/// Straight (non-premultiplied) RGBA8 pixels of the image being annotated.
#[derive(Debug, Clone, Copy)]
pub struct BaseImage<'a> {
    width: u32,
    height: u32,
    rgba: &'a [u8],
}

impl<'a> BaseImage<'a> {
    /// `rgba` holds `width * height` pixels, row by row, four bytes each.
    pub fn new(width: u32, height: u32, rgba: &'a [u8]) -> Result<Self, RenderError> {
        let expected = (width as usize)
            .saturating_mul(height as usize)
            .saturating_mul(4);
        if rgba.len() != expected {
            return Err(RenderError::BaseDataLength {
                expected,
                actual: rgba.len(),
            });
        }
        Ok(Self {
            width,
            height,
            rgba,
        })
    }
}

/// Settings that are not part of the document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderOptions {
    /// Fills the area past the image that annotations extend the rendering
    /// into, so what is saved is what the editor shows there. The editor
    /// passes its theme's surface colour.
    pub workspace_color: Color,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            workspace_color: Color::WHITE,
        }
    }
}

/// A rendering of a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedImage {
    /// Where the pixels sit in image coordinates. Equal to the image's own
    /// rectangle unless annotations reach past it.
    pub bounds: Rect,
    /// Straight RGBA8, `bounds.width * bounds.height` pixels, row by row.
    pub rgba: Vec<u8>,
}

/// Renders `document` over `base`: the image with every annotation baked
/// in, extended as far as annotations reach past it (see
/// [`Document::rendered_bounds`]).
pub fn render(
    document: &Document,
    base: BaseImage<'_>,
    fonts: &FontLibrary,
    options: &RenderOptions,
) -> Result<RenderedImage, RenderError> {
    let size = document.image_size();
    if size.width != base.width || size.height != base.height {
        return Err(RenderError::BaseSizeMismatch {
            expected_width: size.width,
            expected_height: size.height,
            actual_width: base.width,
            actual_height: base.height,
        });
    }
    if size.width == 0 || size.height == 0 {
        return Err(RenderError::EmptyImage);
    }
    if fonts.is_empty() && document.objects().iter().any(needs_font) {
        return Err(RenderError::MissingFont);
    }

    let bounds = document.rendered_bounds(fonts);
    let mut canvas =
        Pixmap::new(bounds.width, bounds.height).ok_or(RenderError::CanvasTooLarge {
            width: bounds.width,
            height: bounds.height,
        })?;
    // The image sits at this offset in the canvas; within the workspace
    // bounds this always fits in usize and i32.
    let image_x = (-i64::from(bounds.x)) as usize;
    let image_y = (-i64::from(bounds.y)) as usize;
    copy_base(&mut canvas, base, image_x, image_y);

    let mut target = Target {
        canvas: &mut canvas,
        bounds,
        transform: Transform::from_translate(-(bounds.x as f32), -(bounds.y as f32)),
        image: Readable {
            left: image_x,
            top: image_y,
            right: image_x + base.width as usize,
            bottom: image_y + base.height as usize,
        },
        image_rect: document.image_rect(),
    };
    for object in document.objects() {
        draw(&mut target, object, fonts);
    }

    if bounds == document.image_rect() {
        return Ok(RenderedImage {
            bounds,
            rgba: demultiplied(canvas),
        });
    }
    // Past the image the canvas is transparent; back it with the workspace
    // colour so the export looks the way the editor showed it. Transparency
    // would turn black wherever the alpha is dropped.
    let mut backed =
        Pixmap::new(bounds.width, bounds.height).ok_or(RenderError::CanvasTooLarge {
            width: bounds.width,
            height: bounds.height,
        })?;
    backed.fill(sk_color(options.workspace_color));
    backed.draw_pixmap(
        0,
        0,
        canvas.as_ref(),
        &PixmapPaint::default(),
        Transform::identity(),
        None,
    );
    Ok(RenderedImage {
        bounds,
        rgba: demultiplied(backed),
    })
}

fn needs_font(object: &Annotation) -> bool {
    match &object.shape {
        Shape::Text { text, .. } => !text.is_empty(),
        Shape::CircleCounter { .. } => true,
        Shape::Pencil { .. }
        | Shape::Line { .. }
        | Shape::Arrow { .. }
        | Shape::Rectangle { .. }
        | Shape::Ellipse { .. }
        | Shape::Marker { .. }
        | Shape::Pixelate { .. }
        | Shape::Invert { .. } => false,
    }
}

fn copy_base(canvas: &mut Pixmap, base: BaseImage<'_>, x: usize, y: usize) {
    let stride = canvas.width() as usize;
    let width = base.width as usize;
    let data = canvas.data_mut();
    for row in 0..base.height as usize {
        for column in 0..width {
            let s = (row * width + column) * 4;
            let d = ((y + row) * stride + x + column) * 4;
            let a = base.rgba[s + 3];
            let pre = |c: u8| ((u32::from(c) * u32::from(a) + 127) / 255) as u8;
            data[d] = pre(base.rgba[s]);
            data[d + 1] = pre(base.rgba[s + 1]);
            data[d + 2] = pre(base.rgba[s + 2]);
            data[d + 3] = a;
        }
    }
}

fn demultiplied(pixmap: Pixmap) -> Vec<u8> {
    pixmap.take_demultiplied()
}

struct Target<'a> {
    canvas: &'a mut Pixmap,
    /// The canvas in image coordinates.
    bounds: Rect,
    /// Image coordinates to canvas pixels.
    transform: Transform,
    /// The image within the canvas, in canvas pixels.
    image: Readable,
    image_rect: Rect,
}

fn sk_color(color: Color) -> SkColor {
    SkColor::from_rgba8(color.r, color.g, color.b, color.a)
}

fn paint(color: Color) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color(sk_color(color));
    paint.anti_alias = true;
    paint
}

fn stroke(width: u32, cap: LineCap, join: LineJoin) -> Stroke {
    // Width 0 is a one-pixel hairline, as Qt's cosmetic pen is.
    Stroke {
        width: width as f32,
        line_cap: cap,
        line_join: join,
        miter_limit: MITER_LIMIT,
        ..Stroke::default()
    }
}

fn pt(p: PointF) -> (f32, f32) {
    (p.x as f32, p.y as f32)
}

fn sk_rect(rect: RectF) -> Option<SkRect> {
    SkRect::from_ltrb(
        rect.left as f32,
        rect.top as f32,
        rect.right as f32,
        rect.bottom as f32,
    )
}

fn polyline(points: &[Point]) -> Option<Path> {
    let (first, rest) = points.split_first()?;
    let mut builder = PathBuilder::new();
    builder.move_to(first.x as f32, first.y as f32);
    for point in rest {
        builder.line_to(point.x as f32, point.y as f32);
    }
    builder.finish()
}

fn segment(a: PointF, b: PointF) -> Option<Path> {
    let mut builder = PathBuilder::new();
    let (ax, ay) = pt(a);
    let (bx, by) = pt(b);
    builder.move_to(ax, ay);
    builder.line_to(bx, by);
    builder.finish()
}

fn polygon(points: &[PointF]) -> Option<Path> {
    let (first, rest) = points.split_first()?;
    let mut builder = PathBuilder::new();
    let (x, y) = pt(*first);
    builder.move_to(x, y);
    for point in rest {
        let (x, y) = pt(*point);
        builder.line_to(x, y);
    }
    builder.close();
    builder.finish()
}

/// A rectangle with corners rounded by `radius`, clamped to half of each
/// side as Qt does.
fn rounded_rect(rect: RectF, radius: u32) -> Option<Path> {
    let r = sk_rect(rect)?;
    let rx = f64::from(radius).min(rect.width() / 2.0) as f32;
    let ry = f64::from(radius).min(rect.height() / 2.0) as f32;
    if rx <= 0.0 || ry <= 0.0 {
        return Some(PathBuilder::from_rect(r));
    }
    // Cubic approximation of a quarter ellipse.
    const K: f32 = 0.552_284_8;
    let (l, t, rr, b) = (r.left(), r.top(), r.right(), r.bottom());
    let mut p = PathBuilder::new();
    p.move_to(l + rx, t);
    p.line_to(rr - rx, t);
    p.cubic_to(rr - rx + rx * K, t, rr, t + ry - ry * K, rr, t + ry);
    p.line_to(rr, b - ry);
    p.cubic_to(rr, b - ry + ry * K, rr - rx + rx * K, b, rr - rx, b);
    p.line_to(l + rx, b);
    p.cubic_to(l + rx - rx * K, b, l, b - ry + ry * K, l, b - ry);
    p.line_to(l, t + ry);
    p.cubic_to(l, t + ry - ry * K, l + rx - rx * K, t, l + rx, t);
    p.close();
    p.finish()
}

fn oval(rect: RectF) -> Option<Path> {
    PathBuilder::from_oval(sk_rect(rect)?)
}

fn circle(center: Point, radius: u32) -> Option<Path> {
    PathBuilder::from_circle(center.x as f32, center.y as f32, radius as f32)
}

impl Target<'_> {
    fn fill(&mut self, path: &Path, color: Color) {
        self.canvas
            .fill_path(path, &paint(color), FillRule::Winding, self.transform, None);
    }

    fn stroke(&mut self, path: &Path, color: Color, stroke: &Stroke) {
        self.canvas
            .stroke_path(path, &paint(color), stroke, self.transform, None);
    }

    /// Draws into a scratch layer over `area` and composites the layer with
    /// the highlight compositing.
    fn highlight(&mut self, area: Option<Rect>, draw: impl FnOnce(&mut Pixmap, Transform)) {
        let Some(area) = area
            .map(|a| a.intersection(&self.bounds))
            .filter(|a| !a.is_empty())
        else {
            return;
        };
        let Some(mut layer) = Pixmap::new(area.width, area.height) else {
            return;
        };
        draw(
            &mut layer,
            Transform::from_translate(-(area.x as f32), -(area.y as f32)),
        );
        // Both are inside the canvas, whose size fits i32.
        let x = (i64::from(area.x) - i64::from(self.bounds.x)) as i32;
        let y = (i64::from(area.y) - i64::from(self.bounds.y)) as i32;
        let blend = PixmapPaint {
            opacity: HIGHLIGHT_OPACITY,
            blend_mode: BlendMode::Multiply,
            quality: FilterQuality::Nearest,
        };
        self.canvas
            .draw_pixmap(x, y, layer.as_ref(), &blend, Transform::identity(), None);
    }

    /// The canvas pixels of `area` within the image, for the tools that
    /// transform pixels. They never act on the workspace past the image.
    fn image_region(&mut self, area: Rect) -> Option<Region<'_>> {
        let area = area.intersection(&self.image_rect);
        if area.is_empty() {
            return None;
        }
        let stride = self.canvas.width() as usize;
        Some(Region {
            data: self.canvas.data_mut(),
            stride,
            x: self.image.left + area.x as usize,
            y: self.image.top + area.y as usize,
            width: area.width as usize,
            height: area.height as usize,
        })
    }
}

fn draw(target: &mut Target<'_>, object: &Annotation, fonts: &FontLibrary) {
    if !object.shape.is_drawable() {
        return;
    }
    let (color, size) = (object.color, object.size);
    match &object.shape {
        Shape::Pencil { points } => {
            if let Some(path) = polyline(points) {
                target.stroke(
                    &path,
                    color,
                    &stroke(size, LineCap::Square, LineJoin::Bevel),
                );
            }
        }
        Shape::Line { start, end } => {
            if let Some(path) = segment((*start).into(), (*end).into()) {
                target.stroke(
                    &path,
                    color,
                    &stroke(size, LineCap::Square, LineJoin::Bevel),
                );
            }
        }
        Shape::Marker { start, end } => {
            let Some(path) = segment((*start).into(), (*end).into()) else {
                return;
            };
            let area = object.export_reach(fonts);
            target.highlight(area, |layer, ts| {
                let s = stroke(size, LineCap::Square, LineJoin::Bevel);
                layer.stroke_path(&path, &paint(color), &s, ts, None);
            });
        }
        Shape::Arrow {
            start,
            end,
            head,
            reversed,
        } => {
            draw_arrow(target, *start, *end, *head, *reversed, color, size);
        }
        Shape::Rectangle {
            start,
            end,
            style,
            outline_width,
        } => {
            let outline = || sk_rect(dragged_area(*start, *end)).map(PathBuilder::from_rect);
            let fill = |plain| rounded_rect(filled_area(*start, *end, size, plain), size);
            draw_closed(
                target,
                object,
                fonts,
                *style,
                *outline_width,
                LineJoin::Miter,
                outline,
                fill,
            );
        }
        Shape::Ellipse {
            start,
            end,
            style,
            outline_width,
        } => {
            let outline = || oval(dragged_area(*start, *end));
            let fill = |plain| oval(filled_area(*start, *end, size, plain));
            draw_closed(
                target,
                object,
                fonts,
                *style,
                *outline_width,
                LineJoin::Round,
                outline,
                fill,
            );
        }
        Shape::Text {
            top_left,
            text,
            font,
            align,
        } => {
            draw_text(target, fonts, *top_left, text, font, *align, color, size);
        }
        Shape::Pixelate { start, end, style } => {
            let area = Rect::from_drag(*start, *end);
            let image = target.image;
            if let Some(mut region) = target.image_region(area) {
                match style {
                    RedactStyle::Frosted => effects::frosted(&mut region, image, size),
                    RedactStyle::Insecure if size <= 1 => effects::blur(&mut region),
                    RedactStyle::Insecure => effects::pixelate(&mut region, size),
                }
            }
        }
        Shape::Invert { start, end } => {
            if let Some(mut region) = target.image_region(Rect::from_drag(*start, *end)) {
                effects::invert(&mut region);
            }
        }
        Shape::CircleCounter {
            center,
            pointer,
            count,
        } => {
            draw_counter(target, fonts, *center, *pointer, *count, color, size);
        }
    }
}

fn draw_arrow(
    target: &mut Target<'_>,
    start: Point,
    end: Point,
    head: ArrowHead,
    reversed: bool,
    color: Color,
    size: u32,
) {
    let Some(geometry) = arrow_geometry(start, end, size, head, reversed) else {
        return;
    };
    let cap = if geometry.flat_cap {
        LineCap::Butt
    } else {
        LineCap::Square
    };
    if let Some(shaft) = segment(geometry.shaft_start, geometry.shaft_end) {
        target.stroke(&shaft, color, &stroke(size, cap, LineJoin::Bevel));
    }
    let mut builder = PathBuilder::new();
    let (x, y) = pt(geometry.tip);
    builder.move_to(x, y);
    for part in &geometry.head {
        match *part {
            HeadSegment::Line(to) => {
                let (x, y) = pt(to);
                builder.line_to(x, y);
            }
            HeadSegment::Quad { control, to } => {
                let (cx, cy) = pt(control);
                let (x, y) = pt(to);
                builder.quad_to(cx, cy, x, y);
            }
        }
    }
    builder.close();
    if let Some(path) = builder.finish() {
        target.fill(&path, color);
    }
}

/// Rectangles and ellipses: the three styles share everything but the path.
#[allow(clippy::too_many_arguments)]
fn draw_closed(
    target: &mut Target<'_>,
    object: &Annotation,
    fonts: &FontLibrary,
    style: ShapeStyle,
    outline_width: u32,
    join: LineJoin,
    outline: impl Fn() -> Option<Path>,
    fill: impl Fn(bool) -> Option<Path>,
) {
    let (color, size) = (object.color, object.size);
    match style {
        // The outline is the whole annotation, so it sits exactly where the
        // user dragged, never on the inflated fill area.
        ShapeStyle::Outline => {
            if let Some(path) = outline() {
                target.stroke(&path, color, &stroke(size, LineCap::Square, join));
            }
        }
        ShapeStyle::Filled => {
            if size == 0 {
                // A plain shape with a one-pixel pen, as at size 0 before.
                if let Some(path) = outline() {
                    target.fill(&path, color);
                    target.stroke(&path, color, &stroke(1, LineCap::Square, join));
                }
            } else if let Some(path) = fill(true) {
                target.fill(&path, color);
            }
        }
        ShapeStyle::Highlight => {
            let Some(path) = fill(false) else { return };
            let area = object
                .export_reach(fonts)
                .map(|r| r.inflated(outline_width));
            // Nothing is stroked under multiply: an outline would composite
            // over its own fill and darken the border.
            target.highlight(area, |layer, ts| {
                layer.fill_path(&path, &paint(color), FillRule::Winding, ts, None);
            });
            // Multiply cannot lighten, so on a dark capture the fill alone
            // vanishes. The border composites normally and keeps it visible.
            if outline_width > 0 {
                target.stroke(
                    &path,
                    color,
                    &stroke(outline_width, LineCap::Square, LineJoin::Round),
                );
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_text(
    target: &mut Target<'_>,
    fonts: &FontLibrary,
    top_left: Point,
    text: &str,
    font: &TextFont,
    align: TextAlign,
    color: Color,
    size: u32,
) {
    let Some(face) = fonts.scaled(font, text_pixel_size(size)) else {
        return;
    };
    let (block_width, _) = face.extent(text);
    let left = f64::from(top_left.x) + f64::from(TEXT_PADDING);
    let mut baseline = f64::from(top_left.y) + f64::from(TEXT_PADDING) + f64::from(face.ascent);
    let mut glyphs = PathBuilder::new();
    let mut bars = PathBuilder::new();
    for line in lines(text) {
        let width = face.line_width(line);
        let x = match align {
            TextAlign::Left => left,
            TextAlign::Center => left + ((f64::from(block_width) - width) / 2.0).round(),
            TextAlign::Right => left + (f64::from(block_width) - width).round(),
        };
        face.push_line(&mut glyphs, line, x, baseline);
        for (wanted, decoration) in [
            (font.underline, Decoration::Underline),
            (font.strikeout, Decoration::Strikeout),
        ] {
            if wanted && width > 0.0 {
                let (offset, thickness) = face.decoration(decoration);
                let top = baseline + offset;
                if let Some(bar) =
                    SkRect::from_xywh(x as f32, top as f32, width as f32, thickness as f32)
                {
                    bars.push_rect(bar);
                }
            }
        }
        baseline += f64::from(face.line_advance());
    }
    if let Some(path) = glyphs.finish() {
        target.fill(&path, color);
        // Synthetic bold: the outline widened by a stroke around it
        if face.embolden > 0.0 {
            let pen = Stroke {
                width: face.embolden as f32,
                line_join: LineJoin::Round,
                ..Stroke::default()
            };
            target.stroke(&path, color, &pen);
        }
    }
    if let Some(path) = bars.finish() {
        target.fill(&path, color);
    }
}

fn draw_counter(
    target: &mut Target<'_>,
    fonts: &FontLibrary,
    center: Point,
    pointer: Point,
    count: u32,
    color: Color,
    size: u32,
) {
    let (contrast, anti_contrast) = if color.is_dark() {
        (Color::WHITE, Color::BLACK)
    } else {
        (Color::BLACK, Color::WHITE)
    };
    let radius = counter_radius(size);
    // The wedge points at what is being counted once the pointer is
    // dragged out of the bubble.
    if let Some(path) = counter_wedge(center, pointer, radius).and_then(|w| polygon(&w)) {
        target.fill(&path, color);
        target.stroke(&path, color, &stroke(0, LineCap::Square, LineJoin::Bevel));
    }
    let pen = stroke(1, LineCap::Square, LineJoin::Bevel);
    if let Some(ring) = circle(center, radius.saturating_add(COUNTER_RING)) {
        target.fill(&ring, anti_contrast);
        target.stroke(&ring, contrast, &pen);
    }
    if let Some(bubble) = circle(center, radius) {
        target.fill(&bubble, color);
        target.stroke(&bubble, contrast, &pen);
    }

    // The number in bold, as large as the bubble allows: the font starts at
    // the bubble radius and shrinks until the number fits a square of that
    // side centred on the bubble.
    let label = count.to_string();
    let side = i64::from(radius);
    let mut pixel_size = side;
    let face = loop {
        let Some(face) = fonts.scaled_default_bold(pixel_size as f64) else {
            return;
        };
        if face.line_width(&label).ceil() as i64 <= side || pixel_size <= 1 {
            break face;
        }
        pixel_size -= 1;
    };
    let box_left = i64::from(center.x) - side / 2;
    let box_top = i64::from(center.y) - side / 2;
    let x = box_left as f64 + (side as f64 - face.line_width(&label)) / 2.0;
    let line_top = box_top + (side - i64::from(face.line_height())) / 2;
    let baseline = line_top as f64 + f64::from(face.ascent);
    let mut builder = PathBuilder::new();
    face.push_line(&mut builder, &label, x, baseline);
    if let Some(path) = builder.finish() {
        target.fill(&path, contrast);
    }
}
