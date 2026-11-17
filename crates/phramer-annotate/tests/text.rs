//! Text layout, placement and picking with a real (generated) font.

mod common;

use common::fonts;
use phramer_annotate::{
    Color, Command, Document, Editor, FontLibrary, Point, Rect, Shape, Size, TextAlign, TextFont,
    text_pixel_size, text_top_left_for_anchor,
};

fn text(top_left: Point, text: &str) -> Shape {
    Shape::Text {
        top_left,
        text: text.into(),
        font: TextFont::default(),
        align: TextAlign::Left,
    }
}

#[test]
fn font_size_follows_the_tool_size_in_points() {
    // Size 0 is 8 pt, at 96 pixels per inch.
    assert!((text_pixel_size(0) - 32.0 / 3.0).abs() < 1e-9);
    assert!((text_pixel_size(4) - 16.0).abs() < 1e-9);
}

#[test]
fn text_area_is_the_glyph_block_plus_five_pixels_of_padding() {
    let fonts = fonts();
    let mut editor = Editor::new(Document::new(Size::new(200, 100)));
    let id = editor
        .apply(Command::Add {
            color: Color::BLACK,
            size: 4,
            shape: text(Point::new(10, 20), "TEST\n42"),
        })
        .unwrap()
        .unwrap();
    let area = editor
        .document()
        .get(id)
        .and_then(|o| o.bounding_rect(&fonts));
    // 16 px font: 4 advances of 9.6 px = 38.4, ceil 39; two lines of
    // 13 + 3 with a 2 px gap = 34. Padding 5 on each side.
    assert_eq!(area, Some(Rect::new(10, 20, 49, 44)));
}

#[test]
fn a_click_places_the_first_line_across_the_anchor() {
    let fonts = fonts();
    let top_left =
        text_top_left_for_anchor(Point::new(50, 50), &TextFont::default(), 4, &fonts).unwrap();
    // Glyphs start at the anchor's x; half the 16 px line sits above it.
    assert_eq!(top_left, Point::new(45, 37));
    assert!(
        text_top_left_for_anchor(
            Point::new(0, 0),
            &TextFont::default(),
            4,
            &FontLibrary::new()
        )
        .is_none()
    );
}

#[test]
fn text_is_picked_anywhere_in_its_box_and_a_little_beyond() {
    let fonts = fonts();
    let mut editor = Editor::new(Document::new(Size::new(200, 100)));
    let id = editor
        .apply(Command::Add {
            color: Color::BLACK,
            size: 4,
            shape: text(Point::new(10, 20), "T  T"),
        })
        .unwrap();
    let doc = editor.document();
    // Between the letters, where no glyph is painted.
    assert_eq!(doc.hit_test(Point::new(30, 30), &fonts), id);
    // Eight pixels outside the right edge (59): within 3 + 5.
    assert_eq!(doc.hit_test(Point::new(66, 30), &fonts), id);
    assert_eq!(doc.hit_test(Point::new(70, 30), &fonts), None);
}

#[test]
fn unknown_family_falls_back_to_the_default_face() {
    let fonts = fonts();
    let shape = Shape::Text {
        top_left: Point::new(0, 0),
        text: "42".into(),
        font: TextFont {
            family: "Not Installed".into(),
            bold: true,
            ..TextFont::default()
        },
        align: TextAlign::Left,
    };
    let mut editor = Editor::new(Document::new(Size::new(50, 50)));
    let id = editor
        .apply(Command::Add {
            color: Color::BLACK,
            size: 0,
            shape,
        })
        .unwrap()
        .unwrap();
    assert!(
        editor
            .document()
            .get(id)
            .and_then(|o| o.bounding_rect(&fonts))
            .is_some()
    );
}

/// "SET" in the test font, in `font`, rendered with `fonts`.
fn styled_render(fonts: &FontLibrary, font: TextFont) -> phramer_annotate::RenderedImage {
    let capture = common::screenshot(120, 60);
    let mut editor = Editor::new(Document::new(Size::new(120, 60)));
    editor
        .apply(Command::Add {
            color: Color::BLACK,
            size: 6,
            shape: Shape::Text {
                top_left: Point::new(4, 4),
                text: "SET".into(),
                font,
                align: TextAlign::Left,
            },
        })
        .unwrap();
    phramer_annotate::render(
        editor.document(),
        capture.base(),
        fonts,
        &phramer_annotate::RenderOptions::default(),
    )
    .unwrap()
}

fn ink(image: &phramer_annotate::RenderedImage) -> usize {
    image
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| pixel[0] < 100 && pixel[1] < 100 && pixel[2] < 100)
        .count()
}

#[test]
fn bold_and_italic_without_a_face_are_unchanged_unless_synthesised() {
    let regular = TextFont::default();
    let bold = TextFont {
        bold: true,
        ..TextFont::default()
    };
    let italic = TextFont {
        italic: true,
        ..TextFont::default()
    };
    // Off (the default): the one face as it is, pixel for pixel
    let plain = fonts();
    assert!(!plain.synthetic_styles());
    let reference = styled_render(&plain, regular.clone());
    assert_eq!(styled_render(&plain, bold.clone()).rgba, reference.rgba);
    assert_eq!(styled_render(&plain, italic.clone()).rgba, reference.rgba);

    let mut synthetic = fonts();
    synthetic.set_synthetic_styles(true);
    // Regular text is untouched by the switch
    assert_eq!(styled_render(&synthetic, regular).rgba, reference.rgba);
    let emboldened = styled_render(&synthetic, bold.clone());
    assert!(ink(&emboldened) > ink(&reference), "bold adds ink");
    let slanted = styled_render(&synthetic, italic);
    assert_ne!(slanted.rgba, reference.rgba, "italic leans");

    // A bold face widens the text area by the emboldening
    let area = |fonts: &FontLibrary, font: &TextFont| {
        let shape = Shape::Text {
            top_left: Point::new(0, 0),
            text: "SET".into(),
            font: font.clone(),
            align: TextAlign::Left,
        };
        let mut editor = Editor::new(Document::new(Size::new(50, 50)));
        let id = editor
            .apply(Command::Add {
                color: Color::BLACK,
                size: 6,
                shape,
            })
            .unwrap()
            .unwrap();
        editor.document().get(id).and_then(|o| o.bounding_rect(fonts))
    };
    let wide = area(&synthetic, &bold).unwrap();
    let narrow = area(&plain, &bold).unwrap();
    assert!(wide.width > narrow.width, "{wide:?} vs {narrow:?}");
    assert_eq!(wide.height, narrow.height);
}

#[test]
fn a_registered_bold_face_is_used_as_it_is() {
    let mut fonts = fonts();
    fonts.set_synthetic_styles(true);
    // The same bytes registered as bold: chosen for bold text, so nothing is
    // synthesised and it draws exactly like the regular face
    fonts
        .add_face("Pixel", true, false, common::pixel_font(), 0)
        .unwrap();
    let regular = styled_render(&fonts, TextFont::default());
    let bold = styled_render(
        &fonts,
        TextFont {
            bold: true,
            ..TextFont::default()
        },
    );
    assert_eq!(bold.rgba, regular.rgba);
    assert!(fonts.has_family("pixel"));
    assert!(!fonts.has_family("Arial"));
}

#[test]
fn a_face_index_past_the_font_is_rejected() {
    let mut fonts = FontLibrary::new();
    assert!(fonts.add_face("Pixel", false, false, common::pixel_font(), 1).is_err());
    assert!(fonts.is_empty());
}

#[test]
fn bad_font_bytes_are_rejected() {
    let mut fonts = FontLibrary::new();
    assert!(fonts.add("Broken", false, false, vec![0, 1, 2, 3]).is_err());
    assert!(fonts.is_empty());
}
