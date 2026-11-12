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

#[test]
fn bad_font_bytes_are_rejected() {
    let mut fonts = FontLibrary::new();
    assert!(fonts.add("Broken", false, false, vec![0, 1, 2, 3]).is_err());
    assert!(fonts.is_empty());
}
