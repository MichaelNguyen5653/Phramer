//! Text recognition of a capture: 15.0.0's OCR (`src/tools/ocr/`).
//!
//! - [`layout`]: the recognised lines in reading order and joined into text
//!   (`ocrlayout.cpp`). Pure.
//! - [`preprocess`] and [`run_pipeline`]: what the image goes through before
//!   and between recognitions (`ocrpreprocess.cpp`, `ocrpipeline.cpp`).
//!   Pure, with the engine passed in.
//! - [`read_text`] (Windows only): Windows.Media.Ocr behind all of that.
//!
//! Coordinates are pixels of the image handed to the engine; only their
//! relations matter to the layout, so a scaled pass needs no remapping.

pub mod layout;
mod pipeline;
pub mod preprocess;
#[cfg(windows)]
mod win;

pub use layout::{OcrTextLayout, assemble_text, order_lines};
pub use pipeline::{run_pipeline, score};
#[cfg(windows)]
pub use win::{available_languages, read_text, recognize};

/// A rectangle in image pixels (`QRectF`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn right(&self) -> f64 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f64 {
        self.y + self.height
    }

    /// The smallest rectangle holding both.
    pub fn united(&self, other: &Rect) -> Rect {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Rect::new(
            x,
            y,
            self.right().max(other.right()) - x,
            self.bottom().max(other.bottom()) - y,
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OcrWord {
    pub text: String,
    pub bounding_box: Rect,
}

/// One line as the engine reports it: a fragment of a visual row, often.
#[derive(Debug, Clone, PartialEq)]
pub struct OcrLine {
    pub text: String,
    /// The union of the word boxes.
    pub bounding_box: Rect,
    pub words: Vec<OcrWord>,
}

impl OcrLine {
    /// A line of `words`, its box their union.
    pub fn from_words(words: Vec<OcrWord>) -> Self {
        let text = words
            .iter()
            .map(|word| word.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let bounding_box = words
            .iter()
            .map(|word| word.bounding_box)
            .reduce(|a, b| a.united(&b))
            .unwrap_or_default();
        Self {
            text,
            bounding_box,
            words,
        }
    }
}

/// How a recognition ended (`OcrResult::Status`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OcrStatus {
    Ok,
    NoTextFound,
    NoLanguageInstalled,
    Unsupported,
    /// With the engine's message, possibly empty.
    EngineError(String),
}

/// What one recognition found.
#[derive(Debug, Clone, PartialEq)]
pub struct OcrResult {
    pub lines: Vec<OcrLine>,
    pub status: OcrStatus,
}

impl OcrResult {
    pub fn with_status(status: OcrStatus) -> Self {
        Self {
            lines: Vec::new(),
            status,
        }
    }
}

/// Straight RGBA8, top row first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl RgbaImage {
    /// From the capture's BGRA, made opaque.
    pub fn from_bgra(width: u32, height: u32, bgra: &[u8]) -> Self {
        let mut rgba = bgra.to_vec();
        for pixel in rgba.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
            pixel[3] = 255;
        }
        Self {
            width,
            height,
            rgba,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    pub(crate) fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let at = (y as usize * self.width as usize + x as usize) * 4;
        self.rgba
            .get(at..at + 4)
            .and_then(|pixel| pixel.try_into().ok())
            .unwrap_or([0, 0, 0, 255])
    }
}

/// What the user is told, as 15.0.0's results window put it; `None` when
/// text was found.
pub fn status_message(status: &OcrStatus) -> Option<String> {
    Some(match status {
        OcrStatus::Ok => return None,
        OcrStatus::NoTextFound => "No text was found in the selection.".to_owned(),
        OcrStatus::NoLanguageInstalled => "No OCR language is installed. Add one under \
                                           Settings > Time & Language > Language & region > \
                                           Add a language; language packs include text \
                                           recognition."
            .to_owned(),
        OcrStatus::Unsupported => "Text recognition is not supported on this platform.".to_owned(),
        OcrStatus::EngineError(message) if message.is_empty() => {
            "Text recognition failed.".to_owned()
        }
        OcrStatus::EngineError(message) => format!("Text recognition failed: {message}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_match_15_0_0() {
        assert_eq!(status_message(&OcrStatus::Ok), None);
        assert_eq!(
            status_message(&OcrStatus::NoTextFound).as_deref(),
            Some("No text was found in the selection.")
        );
        assert_eq!(
            status_message(&OcrStatus::NoLanguageInstalled).as_deref(),
            Some(
                "No OCR language is installed. Add one under Settings > Time & Language > \
                 Language & region > Add a language; language packs include text recognition."
            )
        );
        assert_eq!(
            status_message(&OcrStatus::EngineError(String::new())).as_deref(),
            Some("Text recognition failed.")
        );
        assert_eq!(
            status_message(&OcrStatus::EngineError("0x80004005".into())).as_deref(),
            Some("Text recognition failed: 0x80004005")
        );
    }

    #[test]
    fn a_line_is_the_union_of_its_words() {
        let line = OcrLine::from_words(vec![
            OcrWord {
                text: "a".into(),
                bounding_box: Rect::new(10.0, 5.0, 10.0, 10.0),
            },
            OcrWord {
                text: "b".into(),
                bounding_box: Rect::new(30.0, 3.0, 10.0, 10.0),
            },
        ]);
        assert_eq!(line.text, "a b");
        assert_eq!(line.bounding_box, Rect::new(10.0, 3.0, 30.0, 12.0));
    }

    #[test]
    fn bgra_becomes_opaque_rgba() {
        let image = RgbaImage::from_bgra(1, 1, &[1, 2, 3, 0]);
        assert_eq!(image.rgba, [3, 2, 1, 255]);
    }
}
