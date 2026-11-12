//! Error types, one per domain.

use thiserror::Error;

use crate::tool::{ObjectId, ToolKind};

/// An editing command that could not be applied. The document is unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EditError {
    #[error("no annotation with id {0}")]
    UnknownObject(ObjectId),
    #[error("a {0:?} with this geometry draws nothing")]
    NotDrawable(ToolKind),
    #[error("cannot reshape a {existing:?} into a {replacement:?}")]
    KindMismatch {
        existing: ToolKind,
        replacement: ToolKind,
    },
}

/// A document that could not be read or written.
#[derive(Debug, Error)]
pub enum DocumentError {
    #[error("invalid document JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("annotation id {0} appears more than once")]
    DuplicateId(ObjectId),
}

/// A font the library refused.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FontError {
    #[error("font for family {family:?} could not be parsed: {reason}")]
    Parse { family: String, reason: String },
}

/// A rendering that could not be produced.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RenderError {
    #[error(
        "base image is {actual_width}x{actual_height} but the document was made for \
         {expected_width}x{expected_height}"
    )]
    BaseSizeMismatch {
        expected_width: u32,
        expected_height: u32,
        actual_width: u32,
        actual_height: u32,
    },
    #[error("base image has {actual} bytes of pixel data, expected {expected}")]
    BaseDataLength { expected: usize, actual: usize },
    #[error("the image has no pixels")]
    EmptyImage,
    #[error("a {width}x{height} canvas cannot be allocated")]
    CanvasTooLarge { width: u32, height: u32 },
    #[error("the document contains text or a counter but no font is registered")]
    MissingFont,
}
