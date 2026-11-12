//! Windows.Media.Ocr on an image of known text: rendered by the annotation
//! engine in Inter, the overlay's bundled font. Ignored by default: it
//! needs an OCR language pack installed, which a build machine may lack.
//!
//! ```text
//! cargo test -p phramer-ocr --test live -- --ignored --nocapture
//! ```

#![cfg(windows)]

use phramer_annotate::{
    BaseImage, Color, Command, Document, Editor, FontLibrary, Point, RenderOptions, Shape, Size,
    TextAlign, TextFont, render,
};
use phramer_ocr::{OcrTextLayout, RgbaImage, available_languages, read_text};

/// Inter Variable, as the overlay bundles it.
const INTER: &[u8] = include_bytes!("../../phramer-overlay/assets/InterVariable.ttf");

/// `lines` drawn black on white in 20 pt Inter.
fn rendered(lines: &str) -> RgbaImage {
    let (width, height) = (900, 260);
    let mut fonts = FontLibrary::new();
    fonts
        .add("Inter", false, false, INTER.to_vec())
        .expect("Inter loads");
    let mut editor = Editor::new(Document::new(Size::new(width, height)));
    editor
        .apply(Command::Add {
            color: Color::BLACK,
            // 12 + 8 = 20 pt
            size: 12,
            shape: Shape::Text {
                top_left: Point::new(20, 20),
                text: lines.into(),
                font: TextFont {
                    family: "Inter".into(),
                    ..TextFont::default()
                },
                align: TextAlign::Left,
            },
        })
        .expect("text placed");
    let white = [255u8, 255, 255, 255].repeat((width * height) as usize);
    let base = BaseImage::new(width, height, &white).expect("base image");
    let image =
        render(editor.document(), base, &fonts, &RenderOptions::default()).expect("rendered");
    RgbaImage {
        width: image.bounds.width,
        height: image.bounds.height,
        rgba: image.rgba,
    }
}

#[test]
#[ignore = "live: needs an OCR language pack"]
fn reads_rendered_text() {
    let languages = available_languages();
    println!("OCR languages: {languages:?}");
    if languages.is_empty() {
        println!("skipped: no OCR language is installed");
        return;
    }
    let image = rendered("The quick brown fox\njumps over 42 lazy dogs");
    let text = read_text(&image, "", OcrTextLayout::Plain).expect("text found");
    println!("recognised:\n{text}");
    assert_eq!(text, "The quick brown fox\njumps over 42 lazy dogs");

    let joined = read_text(&image, "", OcrTextLayout::Join).expect("text found");
    assert_eq!(joined, "The quick brown fox jumps over 42 lazy dogs");
}

#[test]
#[ignore = "live: needs an OCR language pack"]
fn a_blank_image_has_no_text() {
    if available_languages().is_empty() {
        println!("skipped: no OCR language is installed");
        return;
    }
    let blank = RgbaImage {
        width: 300,
        height: 100,
        rgba: [255u8, 255, 255, 255].repeat(300 * 100),
    };
    assert_eq!(
        read_text(&blank, "", OcrTextLayout::Preserve),
        Err(phramer_ocr::OcrStatus::NoTextFound)
    );
}
