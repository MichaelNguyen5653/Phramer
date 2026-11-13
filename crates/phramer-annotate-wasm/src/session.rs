//! One editor session: a base image, its annotations with their undo
//! history, and the fonts text is set in. Requests arrive as JSON and
//! answers leave as JSON, so the WebAssembly boundary only moves bytes.

use phramer_annotate::{
    Annotation, BaseImage, Color, Command, Document, Editor, FontLibrary, ObjectId, Point,
    RenderOptions, Shape, Size, TextFont, ToolKind, constrain, render, text_top_left_for_anchor,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The overlay's UI font (Inter Variable, SIL OFL 1.1), registered under the
/// family name the overlay gives it, so text and counters match its pixels.
pub const BUNDLED_FONT: &[u8] = include_bytes!("../../phramer-overlay/assets/InterVariable.ttf");
/// The family the overlay registers [`BUNDLED_FONT`] under.
pub const BUNDLED_FAMILY: &str = "Inter";

/// Bytes before the pixels of a rendering: x, y (i32) and width, height
/// (u32) of its bounds in image coordinates, little-endian.
pub const RENDER_HEADER: usize = 16;

/// An edit as JSON names it; the engine's [`Command`].
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Edit {
    Add {
        color: Color,
        size: u32,
        shape: Shape,
    },
    Move {
        id: ObjectId,
        dx: i32,
        dy: i32,
    },
    Reshape {
        id: ObjectId,
        shape: Shape,
    },
    SetColor {
        id: ObjectId,
        color: Color,
    },
    SetSize {
        id: ObjectId,
        size: u32,
    },
    Delete {
        id: ObjectId,
    },
}

impl From<Edit> for Command {
    fn from(edit: Edit) -> Self {
        match edit {
            Edit::Add { color, size, shape } => Command::Add { color, size, shape },
            Edit::Move { id, dx, dy } => Command::Move { id, dx, dy },
            Edit::Reshape { id, shape } => Command::Reshape { id, shape },
            Edit::SetColor { id, color } => Command::SetColor { id, color },
            Edit::SetSize { id, size } => Command::SetSize { id, size },
            Edit::Delete { id } => Command::Delete { id },
        }
    }
}

/// A free-standing annotation, for measuring one that is not placed yet.
#[derive(Debug, Clone, Deserialize)]
pub struct Loose {
    pub color: Color,
    pub size: u32,
    pub shape: Shape,
}

/// What a request asks for.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// One recorded edit; answers the new object's id and the state.
    Apply {
        edit: Edit,
    },
    Undo,
    Redo,
    State,
    SetNextCounter {
        next: u32,
    },
    SetUndoLimit {
        limit: usize,
    },
    HitTest {
        x: i32,
        y: i32,
    },
    Object {
        id: ObjectId,
    },
    /// An object's selection outline after `preview` edits (a drag).
    Bounds {
        id: ObjectId,
        #[serde(default)]
        preview: Vec<Edit>,
    },
    /// The outline of an annotation that is not in the document.
    AnnotationBounds {
        annotation: Loose,
    },
    /// Where text typed at a click point starts (the overlay's rule).
    TextTopLeft {
        x: i32,
        y: i32,
        size: u32,
        #[serde(default)]
        font: TextFont,
    },
    /// Where a drag with `tool` ends with the constraint key (Shift) held.
    Constrain {
        start: Point,
        cursor: Point,
        tool: ToolKind,
    },
    /// The area a rendering covers, after `preview` edits.
    RenderedBounds {
        #[serde(default)]
        preview: Vec<Edit>,
    },
    Document,
}

#[derive(Debug, Serialize)]
struct State {
    can_undo: bool,
    can_redo: bool,
    next_counter: u32,
    objects: usize,
}

/// An image being edited.
pub struct Session {
    editor: Editor,
    base: Vec<u8>,
    fonts: FontLibrary,
    /// The last answer or rendering, read by the caller.
    pub out: Vec<u8>,
}

impl Session {
    /// A session over straight RGBA8 pixels. With `bundled_font` the
    /// overlay's font is registered first, making it the default family.
    pub fn new(width: u32, height: u32, rgba: Vec<u8>, bundled_font: bool) -> Result<Self, String> {
        BaseImage::new(width, height, &rgba).map_err(|error| error.to_string())?;
        if width == 0 || height == 0 {
            return Err("the image has no pixels".to_owned());
        }
        let mut fonts = FontLibrary::new();
        if bundled_font {
            fonts
                .add(BUNDLED_FAMILY, false, false, BUNDLED_FONT.to_vec())
                .map_err(|error| error.to_string())?;
        }
        Ok(Self {
            editor: Editor::new(Document::new(Size::new(width, height))),
            base: rgba,
            fonts,
            out: Vec::new(),
        })
    }

    pub fn add_font(
        &mut self,
        family: &str,
        bold: bool,
        italic: bool,
        data: Vec<u8>,
    ) -> Result<(), String> {
        self.fonts
            .add(family, bold, italic, data)
            .map_err(|error| error.to_string())
    }

    pub fn document(&self) -> &Document {
        self.editor.document()
    }

    fn state(&self) -> State {
        State {
            can_undo: self.editor.can_undo(),
            can_redo: self.editor.can_redo(),
            next_counter: self.editor.document().next_counter(),
            objects: self.editor.document().objects().len(),
        }
    }

    /// The document with `preview` edits applied on top, unrecorded. Edits
    /// that do not apply (a drag that has not moved yet) are skipped.
    fn previewed(&self, preview: Vec<Edit>) -> Document {
        let mut scratch = Editor::with_undo_limit(self.editor.document().clone(), 1);
        for edit in preview {
            let _ = scratch.apply(edit.into());
        }
        scratch.into_document()
    }

    /// Answers a JSON request; `Err` carries the reason it failed.
    pub fn call(&mut self, request: &str) -> Result<Value, String> {
        let request: Request = serde_json::from_str(request).map_err(|error| error.to_string())?;
        let value = match request {
            Request::Apply { edit } => {
                // The engine records only edits that change something
                let before = self.editor.document().objects().to_vec();
                let id = self
                    .editor
                    .apply(edit.into())
                    .map_err(|error| error.to_string())?;
                let recorded = self.editor.document().objects() != before.as_slice();
                json!({ "id": id, "recorded": recorded, "state": self.state() })
            }
            Request::Undo => {
                let done = self.editor.undo();
                json!({ "done": done, "state": self.state() })
            }
            Request::Redo => {
                let done = self.editor.redo();
                json!({ "done": done, "state": self.state() })
            }
            Request::State => json!(self.state()),
            Request::SetNextCounter { next } => {
                self.editor.set_next_counter(next);
                json!(self.state())
            }
            Request::SetUndoLimit { limit } => {
                self.editor.set_undo_limit(limit);
                json!(self.state())
            }
            Request::HitTest { x, y } => {
                json!(
                    self.editor
                        .document()
                        .hit_test(Point::new(x, y), &self.fonts)
                )
            }
            Request::Object { id } => json!(self.editor.document().get(id)),
            Request::Bounds { id, preview } => {
                let document = self.previewed(preview);
                json!(
                    document
                        .get(id)
                        .and_then(|object| object.bounding_rect(&self.fonts))
                )
            }
            Request::AnnotationBounds { annotation } => {
                let annotation = Annotation {
                    id: ObjectId(u64::MAX),
                    color: annotation.color,
                    size: annotation.size,
                    shape: annotation.shape,
                };
                json!(annotation.bounding_rect(&self.fonts))
            }
            Request::TextTopLeft { x, y, size, font } => {
                let anchor = Point::new(x, y);
                json!(text_top_left_for_anchor(anchor, &font, size, &self.fonts).unwrap_or(anchor))
            }
            Request::Constrain {
                start,
                cursor,
                tool,
            } => json!(constrain(start, cursor, tool.constraint())),
            Request::RenderedBounds { preview } => {
                json!(self.previewed(preview).rendered_bounds(&self.fonts))
            }
            Request::Document => {
                serde_json::to_value(self.editor.document()).map_err(|error| error.to_string())?
            }
        };
        Ok(value)
    }

    /// Renders the document with `preview` edits on top into [`Self::out`]:
    /// [`RENDER_HEADER`] bytes of bounds, then straight RGBA8.
    pub fn render(&mut self, preview: Vec<Edit>, workspace: Color) -> Result<(), String> {
        let document = self.previewed(preview);
        let size = document.image_size();
        let base = BaseImage::new(size.width, size.height, &self.base)
            .map_err(|error| error.to_string())?;
        let options = RenderOptions {
            workspace_color: workspace,
        };
        let image =
            render(&document, base, &self.fonts, &options).map_err(|error| error.to_string())?;
        self.out.clear();
        self.out.reserve(RENDER_HEADER + image.rgba.len());
        self.out.extend_from_slice(&image.bounds.x.to_le_bytes());
        self.out.extend_from_slice(&image.bounds.y.to_le_bytes());
        self.out
            .extend_from_slice(&image.bounds.width.to_le_bytes());
        self.out
            .extend_from_slice(&image.bounds.height.to_le_bytes());
        self.out.extend_from_slice(&image.rgba);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> Session {
        Session::new(4, 3, vec![255; 4 * 3 * 4], true).unwrap()
    }

    #[test]
    fn edits_are_recorded_and_undone() {
        let mut s = session();
        let added = s
            .call(
                r#"{"op":"apply","edit":{"kind":"add","color":{"r":255,"g":0,"b":0},"size":1,
                "shape":{"tool":"line","start":{"x":0,"y":0},"end":{"x":3,"y":2}}}}"#,
            )
            .unwrap();
        assert_eq!(added["id"], 0);
        assert_eq!(added["state"]["can_undo"], true);
        let undone = s.call(r#"{"op":"undo"}"#).unwrap();
        assert_eq!(undone["state"]["objects"], 0);
        assert_eq!(undone["state"]["can_redo"], true);
        assert!(s.call(r#"{"op":"nonsense"}"#).is_err());
    }

    #[test]
    fn previews_are_not_recorded() {
        let mut s = Session::new(20, 20, vec![255; 20 * 20 * 4], true).unwrap();
        s.render(
            vec![Edit::Add {
                color: Color::BLACK,
                size: 1,
                shape: Shape::Line {
                    start: Point::new(5, 10),
                    end: Point::new(15, 10),
                },
            }],
            Color::WHITE,
        )
        .unwrap();
        assert_eq!(s.out.len(), RENDER_HEADER + 20 * 20 * 4);
        // The line was drawn; nothing was recorded
        assert!(s.out[RENDER_HEADER..].iter().any(|&byte| byte < 255));
        assert!(s.document().objects().is_empty());
    }

    #[test]
    fn a_wrong_buffer_size_is_refused() {
        assert!(Session::new(4, 3, vec![0; 5], false).is_err());
        assert!(Session::new(0, 0, Vec::new(), false).is_err());
    }
}
