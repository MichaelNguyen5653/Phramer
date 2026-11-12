//! Phramer's annotation engine: the tools, the document they edit, undo and
//! redo, hit-testing, and a CPU renderer.
//!
//! The crate is compiled natively for the capture overlay and to
//! WebAssembly for the editor, and both draw identical pixels: rendering
//! uses only the scalar pipeline of `tiny-skia`, fonts come from the caller
//! as bytes, and every float routine is the same pure-Rust code on both.
//!
//! Coordinates are image pixels with (0,0) at the image's top-left corner.
//! Annotations may lie past the image (the floating canvas); a rendering
//! grows only as far as they reach.

mod arrow;
mod color;
mod document;
mod editor;
mod effects;
mod error;
mod font;
mod geometry;
mod hit;
mod outline;
mod render;
mod tool;

pub use color::Color;
pub use document::Document;
pub use editor::{Command, DEFAULT_UNDO_LIMIT, Editor};
pub use error::{DocumentError, EditError, FontError, RenderError};
pub use font::{FontLibrary, text_pixel_size, text_top_left_for_anchor};
pub use geometry::{Constraint, Point, Rect, Size, constrain};
pub use render::{BaseImage, RenderOptions, RenderedImage, render};
pub use tool::{
    Annotation, ArrowHead, COUNTER_MAX, COUNTER_MIN, DEFAULT_HIGHLIGHT_OUTLINE_WIDTH,
    MAX_HIGHLIGHT_OUTLINE_WIDTH, ObjectId, RedactStyle, Shape, ShapeStyle, TextAlign, TextFont,
    ToolKind,
};
