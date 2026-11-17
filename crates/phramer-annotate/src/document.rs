//! The serialisable document: a base image size and the annotations on it,
//! bottom-most first.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::color::Color;
use crate::error::DocumentError;
use crate::font::FontLibrary;
use crate::geometry::{Point, Rect, Size};
use crate::hit;
use crate::tool::{Annotation, COUNTER_MAX, COUNTER_MIN, ObjectId, Shape};

/// Bounds of the workspace margin around the image, in pixels.
const MIN_WORKSPACE_MARGIN: u64 = 160;
const MAX_WORKSPACE_MARGIN: u64 = 800;

/// An image and its annotations.
///
/// The document holds no pixels, only the size of the image it annotates;
/// the pixels are handed to the renderer. Editing goes through
/// [`Editor`](crate::Editor), which keeps the undo history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawDocument")]
pub struct Document {
    image_size: Size,
    objects: Vec<Annotation>,
    next_id: u64,
    next_counter: u32,
}

/// The document as it appears in JSON, before its invariants are checked.
#[derive(Deserialize)]
struct RawDocument {
    image_size: Size,
    #[serde(default)]
    objects: Vec<Annotation>,
    #[serde(default)]
    next_id: u64,
    #[serde(default)]
    next_counter: Option<u32>,
}

impl TryFrom<RawDocument> for Document {
    type Error = DocumentError;

    fn try_from(raw: RawDocument) -> Result<Self, Self::Error> {
        let mut seen = HashSet::with_capacity(raw.objects.len());
        let mut largest_id = None;
        for object in &raw.objects {
            if !seen.insert(object.id) {
                return Err(DocumentError::DuplicateId(object.id));
            }
            largest_id = largest_id.max(Some(object.id.0));
        }
        // Ids must stay unique after loading, whatever the file claimed.
        let after_largest = largest_id.map_or(0, |id| id.saturating_add(1));
        let mut document = Document {
            image_size: raw.image_size,
            objects: raw.objects,
            next_id: raw.next_id.max(after_largest),
            next_counter: COUNTER_MIN,
        };
        match raw.next_counter {
            Some(next) => document.set_next_counter(next),
            None => document.resume_counter(),
        }
        Ok(document)
    }
}

impl Document {
    /// An empty document for an image of the given size.
    pub fn new(image_size: Size) -> Self {
        Self {
            image_size,
            objects: Vec::new(),
            next_id: 0,
            next_counter: COUNTER_MIN,
        }
    }

    pub fn image_size(&self) -> Size {
        self.image_size
    }

    /// The image in its own coordinates: the origin and the image size.
    pub fn image_rect(&self) -> Rect {
        Rect::new(0, 0, self.image_size.width, self.image_size.height)
    }

    /// The annotations, bottom-most first.
    pub fn objects(&self) -> &[Annotation] {
        &self.objects
    }

    pub fn get(&self, id: ObjectId) -> Option<&Annotation> {
        self.objects.iter().find(|object| object.id == id)
    }

    /// The number the next counter bubble will show.
    pub fn next_counter(&self) -> u32 {
        self.next_counter
    }

    /// Chooses the number the next counter bubble will show, within 1..=999.
    pub fn set_next_counter(&mut self, next: u32) {
        self.next_counter = next.clamp(COUNTER_MIN, COUNTER_MAX);
    }

    /// The floating canvas: the image with room around it for annotations
    /// that sit beside it. Nothing outside this area is ever rendered.
    ///
    /// The margin is two fifths of the longer image side, kept between 160
    /// and 800 pixels.
    pub fn workspace_rect(&self) -> Rect {
        let longer = u64::from(self.image_size.width.max(self.image_size.height));
        // round(longer * 0.4) in integers.
        let margin = ((longer * 4 + 5) / 10).clamp(MIN_WORKSPACE_MARGIN, MAX_WORKSPACE_MARGIN);
        // Within the clamp, so it fits.
        self.image_rect().inflated(margin as u32)
    }

    /// The area a rendering covers: the image, grown only as far as
    /// annotations reach past it and never beyond the workspace. With
    /// nothing outside the image it is the image exactly.
    ///
    /// Pixelate and invert act on image pixels and never grow it.
    pub fn rendered_bounds(&self, fonts: &FontLibrary) -> Rect {
        let image = self.image_rect();
        if image.is_empty() {
            return image;
        }
        self.objects
            .iter()
            .filter_map(|object| object.export_reach(fonts))
            .fold(image, |area, reach| area.union(&reach))
            .intersection(&self.workspace_rect())
    }

    /// The topmost object at `point`, with the pick tolerance of the old
    /// editor: within 3 pixels of its ink, else within 5.
    pub fn hit_test(&self, point: Point, fonts: &FontLibrary) -> Option<ObjectId> {
        hit::find(&self.objects, point, fonts)
    }

    pub fn to_json(&self) -> Result<String, DocumentError> {
        Ok(serde_json::to_string(self)?)
    }

    pub fn from_json(json: &str) -> Result<Self, DocumentError> {
        Ok(serde_json::from_str(json)?)
    }

    /// Appends a new object on top and returns its id. A counter takes the
    /// next number in the sequence, whatever its shape said.
    pub(crate) fn insert(&mut self, color: Color, size: u32, mut shape: Shape) -> ObjectId {
        if let Shape::CircleCounter { count, .. } = &mut shape {
            *count = self.next_counter;
            self.set_next_counter(self.next_counter.saturating_add(1));
        }
        let id = ObjectId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        self.objects.push(Annotation {
            id,
            color,
            size,
            shape,
        });
        id
    }

    pub(crate) fn get_mut(&mut self, id: ObjectId) -> Option<&mut Annotation> {
        self.objects.iter_mut().find(|object| object.id == id)
    }

    /// Removes an object. Deleting a counter closes the gap it leaves: every
    /// counter numbered at or above it moves down by one, and so does the
    /// next number to be handed out.
    pub(crate) fn remove(&mut self, id: ObjectId) -> Option<Annotation> {
        let index = self.objects.iter().position(|object| object.id == id)?;
        let removed = self.objects.remove(index);
        if let Shape::CircleCounter {
            count: removed_count,
            ..
        } = removed.shape
        {
            for object in &mut self.objects {
                if let Shape::CircleCounter { count, .. } = &mut object.shape
                    && *count >= removed_count
                {
                    *count = count.saturating_sub(1);
                }
            }
            self.set_next_counter(self.next_counter.saturating_sub(1));
        }
        Some(removed)
    }

    /// Moves an object to `index` (clamped to the top), the others keeping
    /// their order. `None` when there is no such object.
    pub(crate) fn reorder(&mut self, id: ObjectId, index: usize) -> Option<()> {
        let from = self.objects.iter().position(|object| object.id == id)?;
        let object = self.objects.remove(from);
        let to = index.min(self.objects.len());
        self.objects.insert(to, object);
        Some(())
    }

    /// Replaces every object, as undo and redo do, and picks the counter
    /// sequence up after the largest number present.
    pub(crate) fn restore(&mut self, objects: Vec<Annotation>) -> Vec<Annotation> {
        let previous = core::mem::replace(&mut self.objects, objects);
        self.resume_counter();
        previous
    }

    fn resume_counter(&mut self) {
        let largest = self
            .objects
            .iter()
            .filter_map(|object| match object.shape {
                Shape::CircleCounter { count, .. } => Some(count),
                Shape::Pencil { .. }
                | Shape::Line { .. }
                | Shape::Arrow { .. }
                | Shape::Rectangle { .. }
                | Shape::Ellipse { .. }
                | Shape::Marker { .. }
                | Shape::Text { .. }
                | Shape::Pixelate { .. }
                | Shape::Invert { .. } => None,
            })
            .max()
            .unwrap_or(0);
        self.set_next_counter(largest.saturating_add(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::RedactStyle;

    fn p(x: i32, y: i32) -> Point {
        Point::new(x, y)
    }

    fn document() -> Document {
        Document::new(Size::new(400, 300))
    }

    fn line(start: Point, end: Point) -> Shape {
        Shape::Line { start, end }
    }

    #[test]
    fn workspace_margin_is_two_fifths_of_the_longer_side_within_bounds() {
        // 400 * 0.4 = 160.
        assert_eq!(document().workspace_rect(), Rect::new(-160, -160, 720, 620));
        // 100 * 0.4 = 40, raised to the minimum.
        let small = Document::new(Size::new(100, 50));
        assert_eq!(small.workspace_rect(), Rect::new(-160, -160, 420, 370));
        // 3000 * 0.4 = 1200, capped.
        let large = Document::new(Size::new(3000, 1000));
        assert_eq!(large.workspace_rect(), Rect::new(-800, -800, 4600, 2600));
        // 1001 * 0.4 = 400.4, rounds down.
        let odd = Document::new(Size::new(1001, 10));
        assert_eq!(odd.workspace_rect().x, -400);
    }

    #[test]
    fn rendering_is_the_image_exactly_when_nothing_reaches_past_it() {
        let fonts = FontLibrary::new();
        let mut doc = document();
        assert_eq!(doc.rendered_bounds(&fonts), Rect::new(0, 0, 400, 300));
        doc.insert(Color::BLACK, 3, line(p(50, 50), p(350, 250)));
        assert_eq!(doc.rendered_bounds(&fonts), Rect::new(0, 0, 400, 300));
    }

    #[test]
    fn rendering_grows_only_as_far_as_annotations_reach() {
        let fonts = FontLibrary::new();
        let mut doc = document();
        // Thickness 4: bounds offset round(3.3) = 3, pad 4 / 2 + 2 = 4.
        doc.insert(Color::BLACK, 4, line(p(380, 100), p(450, 120)));
        assert_eq!(doc.rendered_bounds(&fonts), Rect::new(0, 0, 457, 300));
        doc.insert(Color::BLACK, 4, line(p(20, -30), p(40, 10)));
        assert_eq!(doc.rendered_bounds(&fonts), Rect::new(0, -37, 457, 337));
    }

    #[test]
    fn rendering_never_leaves_the_workspace() {
        let fonts = FontLibrary::new();
        let mut doc = document();
        doc.insert(Color::BLACK, 4, line(p(100, 100), p(5000, 100)));
        assert_eq!(doc.rendered_bounds(&fonts), Rect::new(0, 0, 560, 300));
    }

    #[test]
    fn pixel_reading_tools_never_grow_the_rendering() {
        let fonts = FontLibrary::new();
        let mut doc = document();
        doc.insert(
            Color::BLACK,
            4,
            Shape::Invert {
                start: p(350, 250),
                end: p(500, 400),
            },
        );
        doc.insert(
            Color::BLACK,
            4,
            Shape::Pixelate {
                start: p(-50, -50),
                end: p(60, 60),
                style: RedactStyle::Frosted,
            },
        );
        assert_eq!(doc.rendered_bounds(&fonts), Rect::new(0, 0, 400, 300));
    }

    #[test]
    fn counters_are_numbered_in_order_of_creation() {
        let mut doc = document();
        let counter = |x| Shape::CircleCounter {
            center: p(x, 50),
            pointer: p(x, 50),
            count: 77,
        };
        let ids: Vec<_> = (0..3)
            .map(|i| doc.insert(Color::BLACK, 2, counter(i * 60)))
            .collect();
        let counts: Vec<_> = ids.iter().map(|id| counter_of(&doc, *id)).collect();
        assert_eq!(counts, [1, 2, 3]);
        assert_eq!(doc.next_counter(), 4);
    }

    #[test]
    fn deleting_a_counter_renumbers_the_ones_after_it() {
        let mut doc = document();
        let counter = |x| Shape::CircleCounter {
            center: p(x, 50),
            pointer: p(x, 50),
            count: 0,
        };
        let a = doc.insert(Color::BLACK, 2, counter(0));
        let b = doc.insert(Color::BLACK, 2, counter(60));
        let c = doc.insert(Color::BLACK, 2, counter(120));
        doc.remove(b);
        assert_eq!(counter_of(&doc, a), 1);
        assert_eq!(counter_of(&doc, c), 2);
        assert_eq!(doc.next_counter(), 3);
    }

    #[test]
    fn counter_sequence_stops_at_999_and_can_be_set_by_hand() {
        let mut doc = document();
        doc.set_next_counter(5000);
        assert_eq!(doc.next_counter(), 999);
        let counter = Shape::CircleCounter {
            center: p(0, 0),
            pointer: p(0, 0),
            count: 0,
        };
        let a = doc.insert(Color::BLACK, 2, counter.clone());
        let b = doc.insert(Color::BLACK, 2, counter);
        assert_eq!(counter_of(&doc, a), 999);
        assert_eq!(counter_of(&doc, b), 999);
        doc.set_next_counter(0);
        assert_eq!(doc.next_counter(), 1);
    }

    #[test]
    fn loading_rejects_duplicate_ids_and_repairs_the_id_sequence() {
        let duplicate = r#"{"image_size":{"width":10,"height":10},"objects":[
            {"id":3,"color":{"r":0,"g":0,"b":0,"a":255},"size":1,
             "shape":{"tool":"line","start":{"x":0,"y":0},"end":{"x":5,"y":5}}},
            {"id":3,"color":{"r":0,"g":0,"b":0,"a":255},"size":1,
             "shape":{"tool":"line","start":{"x":0,"y":0},"end":{"x":5,"y":5}}}]}"#;
        assert!(Document::from_json(duplicate).is_err());

        let stale = r#"{"image_size":{"width":10,"height":10},"next_id":0,"objects":[
            {"id":7,"color":{"r":0,"g":0,"b":0},"size":1,
             "shape":{"tool":"circle_counter","center":{"x":0,"y":0},
                      "pointer":{"x":0,"y":0},"count":4}}]}"#;
        let mut doc = Document::from_json(stale).unwrap();
        assert_eq!(doc.next_counter(), 5);
        let id = doc.insert(Color::BLACK, 1, line(p(0, 0), p(1, 1)));
        assert_eq!(id, ObjectId(8));
    }

    fn counter_of(doc: &Document, id: ObjectId) -> u32 {
        match doc.get(id).map(|object| &object.shape) {
            Some(Shape::CircleCounter { count, .. }) => *count,
            other => panic!("not a counter: {other:?}"),
        }
    }
}
