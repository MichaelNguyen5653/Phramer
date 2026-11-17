//! The screenshot with the document's objects drawn in, kept between frames.
//!
//! A pointer move redraws the window from this cache: only the object being
//! drawn (or dragged) is rendered again, into the small area it covers. The
//! whole document is rendered only when objects change in some other way
//! than one more on top (undo, a move, a delete), and a new object on top
//! is drawn into its own area alone.
//!
//! Rendering goes through `phramer_annotate::render`, so the overlay shows
//! exactly what the export will contain.

use phramer_annotate::{
    Annotation, BaseImage, Command, Document, Editor, FontLibrary, ObjectId, Rect, RenderOptions,
    Shape, Size, render,
};

/// Extra pixels around an object's own bounds: anti-aliasing, and the one
/// pixel border the frosted fill reads.
const MARGIN: u32 = 4;

/// What `annotated` holds.
pub(crate) struct Layer {
    objects: Vec<Annotation>,
    excluded: Option<ObjectId>,
    /// BGRA, the frame's size; empty while no object is drawn in.
    annotated: Vec<u8>,
}

impl Layer {
    pub(crate) fn new() -> Self {
        Self {
            objects: Vec::new(),
            excluded: None,
            annotated: Vec::new(),
        }
    }

    /// The screenshot with the objects in: the cache, or the frame itself.
    pub(crate) fn pixels<'a>(&'a self, frame: &'a [u8]) -> &'a [u8] {
        if self.annotated.is_empty() {
            frame
        } else {
            &self.annotated
        }
    }

    /// Brings the cache up to `objects` without `excluded`. Returns the
    /// area that changed, in image pixels, or `None` when nothing did.
    pub(crate) fn update(
        &mut self,
        frame: &[u8],
        size: (u32, u32),
        objects: &[Annotation],
        excluded: Option<ObjectId>,
        fonts: &FontLibrary,
    ) -> Option<Rect> {
        let wanted: Vec<Annotation> = objects
            .iter()
            .filter(|object| Some(object.id) != excluded)
            .cloned()
            .collect();
        if excluded == self.excluded && wanted == self.objects {
            return None;
        }
        let image = Rect::new(0, 0, size.0, size.1);
        let one_more_on_top = excluded == self.excluded
            && wanted.len() == self.objects.len() + 1
            && wanted[..self.objects.len()] == self.objects[..];
        let changed = if one_more_on_top {
            if self.annotated.is_empty() {
                self.annotated = frame.to_vec();
            }
            let added = &wanted[wanted.len() - 1..];
            let area = reach(added, fonts).intersection(&image);
            if let Some(pixels) = render_area(&self.annotated, size, area, added, fonts) {
                paste(&mut self.annotated, size.0, area, &pixels);
            }
            area
        } else if wanted.is_empty() {
            self.annotated = Vec::new();
            image
        } else {
            self.annotated =
                render_area(frame, size, image, &wanted, fonts).unwrap_or_else(|| frame.to_vec());
            image
        };
        self.objects = wanted;
        self.excluded = excluded;
        Some(changed)
    }
}

/// Where `objects` can draw, in image pixels.
pub(crate) fn reach(objects: &[Annotation], fonts: &FontLibrary) -> Rect {
    objects
        .iter()
        .filter_map(|object| {
            let pad = (object.size / 2).saturating_add(2).max(2) + MARGIN;
            object
                .bounding_rect(fonts)
                .map(|bounds| bounds.inflated(pad))
        })
        .fold(Rect::default(), |area, rect| area.union(&rect))
}

/// Renders `objects` over `area` of `source` (BGRA, `size`), returning the
/// area's new BGRA pixels. `None` when the area is empty or nothing could
/// be rendered.
pub(crate) fn render_area(
    source: &[u8],
    size: (u32, u32),
    area: Rect,
    objects: &[Annotation],
    fonts: &FontLibrary,
) -> Option<Vec<u8>> {
    if area.is_empty() || area.x < 0 || area.y < 0 {
        return None;
    }
    let (x0, y0) = (area.x as usize, area.y as usize);
    let (width, height) = (area.width as usize, area.height as usize);
    let stride = size.0 as usize * 4;
    if x0 + width > size.0 as usize || y0 + height > size.1 as usize {
        return None;
    }
    let mut rgba = Vec::with_capacity(width * height * 4);
    for row in y0..y0 + height {
        let line = &source[row * stride + x0 * 4..row * stride + (x0 + width) * 4];
        for pixel in line.as_chunks::<4>().0 {
            rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
        }
    }
    let document = document_of(area, objects);
    let base = BaseImage::new(area.width, area.height, &rgba).ok()?;
    let rendered = render(&document, base, fonts, &RenderOptions::default()).ok()?;
    // The area's own pixels: the rendering grows past it when objects do
    let offset_x = usize::try_from(-rendered.bounds.x).unwrap_or(0);
    let offset_y = usize::try_from(-rendered.bounds.y).unwrap_or(0);
    let rendered_stride = rendered.bounds.width as usize * 4;
    let mut bgra = Vec::with_capacity(width * height * 4);
    for row in 0..height {
        let start = (row + offset_y) * rendered_stride + offset_x * 4;
        let line = rendered.rgba.get(start..start + width * 4)?;
        for pixel in line.as_chunks::<4>().0 {
            bgra.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
        }
    }
    Some(bgra)
}

/// The objects as a document whose image is `area`, moved so `area`'s
/// corner is its origin. Counters keep their numbers.
fn document_of(area: Rect, objects: &[Annotation]) -> Document {
    let mut editor = Editor::with_undo_limit(Document::new(Size::new(area.width, area.height)), 1);
    for object in objects {
        if let Shape::CircleCounter { count, .. } = object.shape {
            editor.set_next_counter(count);
        }
        let added = editor.apply(Command::Add {
            color: object.color,
            size: object.size,
            shape: object.shape.clone(),
        });
        if let Ok(Some(id)) = added
            && (area.x, area.y) != (0, 0)
            && let Err(error) = editor.apply(Command::Move {
                id,
                dx: -area.x,
                dy: -area.y,
            })
        {
            log::debug!("cannot place object {id}: {error}");
        }
    }
    editor.into_document()
}

/// Copies an area's BGRA pixels into the frame-sized `target`.
pub(crate) fn paste(target: &mut [u8], width: u32, area: Rect, pixels: &[u8]) {
    if area.is_empty() || area.x < 0 || area.y < 0 {
        return;
    }
    let stride = width as usize * 4;
    let row_bytes = area.width as usize * 4;
    for (row, line) in pixels.chunks_exact(row_bytes).enumerate() {
        let start = (area.y as usize + row) * stride + area.x as usize * 4;
        if let Some(target) = target.get_mut(start..start + row_bytes) {
            target.copy_from_slice(line);
        }
    }
}
