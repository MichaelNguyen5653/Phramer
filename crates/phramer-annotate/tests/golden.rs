//! Golden-image tests: one scene per tool, compared byte for byte with the
//! PNGs under `tests/golden/`. `PHRAMER_UPDATE_GOLDEN=1` rewrites them; a
//! mismatch writes the actual and a diff image under `target/` and names
//! them in the failure.

mod common;

use common::*;
use phramer_annotate::{
    ArrowHead, Color, Command, Document, Editor, Point, Rect, RedactStyle, Shape, ShapeStyle, Size,
    TextAlign, TextFont,
};

fn p(x: i32, y: i32) -> Point {
    Point::new(x, y)
}

fn document(capture: &Capture, items: Vec<(Color, u32, Shape)>) -> Document {
    let mut editor = Editor::new(Document::new(Size::new(capture.width, capture.height)));
    for (color, size, shape) in items {
        editor
            .apply(Command::Add { color, size, shape })
            .expect("scene objects are drawable");
    }
    editor.into_document()
}

fn scene(name: &str, capture: &Capture, items: Vec<(Color, u32, Shape)>) {
    let doc = document(capture, items);
    let image = render_doc(&doc, capture);
    check_golden(name, &image);
}

fn rect(start: Point, end: Point, style: ShapeStyle, outline_width: u32) -> Shape {
    Shape::Rectangle {
        start,
        end,
        style,
        outline_width,
    }
}

fn ellipse(start: Point, end: Point, style: ShapeStyle, outline_width: u32) -> Shape {
    Shape::Ellipse {
        start,
        end,
        style,
        outline_width,
    }
}

fn arrow(start: Point, end: Point, head: ArrowHead, reversed: bool) -> Shape {
    Shape::Arrow {
        start,
        end,
        head,
        reversed,
    }
}

fn counter(center: Point, pointer: Point) -> Shape {
    Shape::CircleCounter {
        center,
        pointer,
        count: 0,
    }
}

#[test]
fn pencil() {
    let capture = screenshot(160, 120);
    let zigzag: Vec<Point> = (0..9)
        .map(|i| p(12 + i * 15, if i % 2 == 0 { 20 } else { 55 }))
        .collect();
    let curve: Vec<Point> = (0..40)
        .map(|i| {
            let t = f64::from(i) / 39.0;
            p(
                10 + (t * 140.0) as i32,
                85 - (25.0 * (t * 6.0).sin()) as i32,
            )
        })
        .collect();
    scene(
        "pencil",
        &capture,
        vec![
            (RED, 3, Shape::Pencil { points: zigzag }),
            (BLUE, 1, Shape::Pencil { points: curve }),
        ],
    );
}

#[test]
fn line() {
    let capture = screenshot(160, 120);
    scene(
        "line",
        &capture,
        vec![
            (
                RED,
                1,
                Shape::Line {
                    start: p(10, 10),
                    end: p(150, 10),
                },
            ),
            (
                GREEN,
                4,
                Shape::Line {
                    start: p(10, 25),
                    end: p(150, 60),
                },
            ),
            (
                BLUE,
                10,
                Shape::Line {
                    start: p(20, 105),
                    end: p(140, 40),
                },
            ),
        ],
    );
}

#[test]
fn arrow_heads() {
    let capture = screenshot(160, 120);
    scene(
        "arrow",
        &capture,
        vec![
            (
                RED,
                2,
                arrow(p(10, 15), p(140, 15), ArrowHead::Default, false),
            ),
            (
                BLUE,
                6,
                arrow(p(15, 110), p(90, 40), ArrowHead::Default, false),
            ),
            (
                GREEN,
                4,
                arrow(p(100, 50), p(150, 105), ArrowHead::Curved, false),
            ),
            (
                RED,
                3,
                arrow(p(20, 40), p(70, 40), ArrowHead::Default, true),
            ),
            // Shorter than its head: the head shrinks, a stub of shaft is left.
            (
                BLUE,
                8,
                arrow(p(20, 62), p(32, 62), ArrowHead::Default, false),
            ),
        ],
    );
}

#[test]
fn rectangle_styles() {
    let capture = screenshot(160, 120);
    scene(
        "rectangle",
        &capture,
        vec![
            (RED, 3, rect(p(10, 10), p(60, 50), ShapeStyle::Outline, 2)),
            (GREEN, 6, rect(p(80, 12), p(140, 48), ShapeStyle::Filled, 2)),
            (
                YELLOW,
                4,
                rect(p(15, 65), p(150, 105), ShapeStyle::Highlight, 2),
            ),
            (BLUE, 0, rect(p(30, 25), p(45, 35), ShapeStyle::Filled, 2)),
        ],
    );
}

#[test]
fn ellipse_styles() {
    let capture = screenshot(160, 120);
    scene(
        "ellipse",
        &capture,
        vec![
            (
                RED,
                3,
                ellipse(p(10, 10), p(70, 55), ShapeStyle::Outline, 2),
            ),
            (
                GREEN,
                5,
                ellipse(p(85, 12), p(145, 50), ShapeStyle::Filled, 2),
            ),
            (
                YELLOW,
                4,
                ellipse(p(20, 62), p(140, 110), ShapeStyle::Highlight, 2),
            ),
        ],
    );
}

#[test]
fn marker() {
    let capture = screenshot(160, 120);
    let marker = |start, end| Shape::Marker { start, end };
    scene(
        "marker",
        &capture,
        vec![
            // Marker objects carry their full width: tool size 4 + 14.
            (YELLOW, 18, marker(p(30, 30), p(130, 30))),
            (YELLOW, 18, marker(p(60, 26), p(60, 94))),
            (Color::rgb(80, 220, 120), 18, marker(p(30, 88), p(130, 88))),
            // Constrained to the diagonal while dragging.
            (Color::rgb(255, 120, 200), 18, marker(p(85, 75), p(125, 35))),
        ],
    );
}

#[test]
fn highlight_over_dark_and_colour() {
    // Half dark, half saturated stripes: multiply keeps what is underneath
    // visible, and the border is what keeps the highlight visible on black.
    let capture = Capture::new(160, 120, |x, y| {
        if x < 80 {
            [20, 22, 28, 255]
        } else if (y / 10) % 2 == 0 {
            [60, 160, 230, 255]
        } else {
            [240, 120, 60, 255]
        }
    });
    scene(
        "highlight",
        &capture,
        vec![
            (
                YELLOW,
                4,
                rect(p(15, 10), p(145, 40), ShapeStyle::Highlight, 2),
            ),
            (
                YELLOW,
                4,
                rect(p(15, 55), p(145, 75), ShapeStyle::Highlight, 0),
            ),
            (
                Color::rgb(255, 80, 200),
                3,
                ellipse(p(40, 82), p(120, 112), ShapeStyle::Highlight, 3),
            ),
            // Overlap darkens twice.
            (
                YELLOW,
                4,
                rect(p(100, 25), p(130, 65), ShapeStyle::Highlight, 0),
            ),
        ],
    );
}

#[test]
fn text() {
    let capture = screenshot(160, 120);
    let text = |top_left, text: &str, font, align| Shape::Text {
        top_left,
        text: text.to_string(),
        font,
        align,
    };
    let underlined = TextFont {
        underline: true,
        ..TextFont::default()
    };
    let struck = TextFont {
        strikeout: true,
        ..TextFont::default()
    };
    scene(
        "text",
        &capture,
        vec![
            (
                RED,
                4,
                text(
                    p(4, 6),
                    "PHRAMER\nTEST 42",
                    TextFont::default(),
                    TextAlign::Left,
                ),
            ),
            (
                BLUE,
                0,
                text(p(4, 44), "AXE\n2", underlined, TextAlign::Center),
            ),
            (
                GREEN,
                0,
                text(p(52, 44), "SET 7\n9", struck, TextAlign::Right),
            ),
        ],
    );
}

#[test]
fn pixelate() {
    let capture = screenshot(240, 120);
    let redact = |start, end, style| Shape::Pixelate { start, end, style };
    scene(
        "pixelate",
        &capture,
        vec![
            // Frosted fill over the checkerboard: only its surroundings show.
            (
                BLUE,
                3,
                redact(p(150, 15), p(185, 50), RedactStyle::Frosted),
            ),
            // Frosted against the image edge, across the dark bar.
            (BLUE, 10, redact(p(0, 70), p(60, 119), RedactStyle::Frosted)),
            // The insecure blur, at size 1.
            (
                BLUE,
                1,
                redact(p(80, 70), p(140, 105), RedactStyle::Insecure),
            ),
            // The insecure pixelation, at size 3, across checkerboard, background and bar.
            (
                BLUE,
                3,
                redact(p(190, 40), p(231, 105), RedactStyle::Insecure),
            ),
        ],
    );
}

#[test]
fn invert() {
    let capture = screenshot(160, 120);
    scene(
        "invert",
        &capture,
        vec![
            (
                RED,
                3,
                Shape::Line {
                    start: p(10, 20),
                    end: p(80, 20),
                },
            ),
            // Over an earlier annotation, the checkerboard and the bar.
            (
                Color::BLACK,
                3,
                Shape::Invert {
                    start: p(30, 10),
                    end: p(120, 90),
                },
            ),
            // Past the image edge: acts on the image only, never grows it.
            (
                Color::BLACK,
                3,
                Shape::Invert {
                    start: p(140, 100),
                    end: p(200, 160),
                },
            ),
        ],
    );
}

#[test]
fn circle_counter() {
    let capture = screenshot(160, 120);
    scene(
        "counter",
        &capture,
        vec![
            (RED, 2, counter(p(25, 25), p(25, 25))),
            (YELLOW, 2, counter(p(70, 30), p(70, 30))),
            (BLUE, 2, counter(p(120, 30), p(150, 70))),
            (GREEN, 10, counter(p(40, 85), p(110, 105))),
        ],
    );
}

#[test]
fn floating_canvas() {
    let capture = screenshot(120, 80);
    let doc = document(
        &capture,
        vec![
            // Points out past the right edge.
            (
                RED,
                3,
                arrow(p(60, 40), p(150, 40), ArrowHead::Default, false),
            ),
            // Beside the image, above its top-left corner.
            (
                BLUE,
                0,
                Shape::Text {
                    top_left: p(-30, -25),
                    text: "TEST".into(),
                    font: TextFont::default(),
                    align: TextAlign::Left,
                },
            ),
            // Crossing the bottom edge.
            (GREEN, 3, rect(p(10, 60), p(50, 95), ShapeStyle::Outline, 2)),
            // A highlight past the edge composites against the workspace.
            (
                YELLOW,
                2,
                rect(p(90, 65), p(130, 90), ShapeStyle::Highlight, 0),
            ),
            // Never grows the rendering.
            (
                Color::BLACK,
                1,
                Shape::Invert {
                    start: p(-60, -60),
                    end: p(20, 20),
                },
            ),
        ],
    );
    let image = render_doc(&doc, &capture);
    assert_eq!(image.bounds, doc.rendered_bounds(&fonts()));
    assert!(image.bounds.x < 0 && image.bounds.y < 0);
    assert!(image.bounds.right() > 120 && image.bounds.bottom() > 80);
    check_golden("floating_canvas", &image);
}

#[test]
fn reorder() {
    let capture = screenshot(160, 120);
    let red = (RED, 0, rect(p(20, 20), p(90, 80), ShapeStyle::Filled, 2));
    let blue = (BLUE, 0, rect(p(60, 40), p(130, 100), ShapeStyle::Filled, 2));
    let marker = (YELLOW, 4, rect(p(10, 50), p(150, 70), ShapeStyle::Highlight, 0));
    let mut editor = Editor::new(document(
        &capture,
        vec![red.clone(), blue.clone(), marker.clone()],
    ));
    let ids: Vec<_> = editor.document().objects().iter().map(|o| o.id).collect();
    // Red to the top, then the highlight to the bottom
    editor
        .apply(Command::Reorder {
            id: ids[0],
            index: 2,
        })
        .expect("red exists");
    editor
        .apply(Command::Reorder {
            id: ids[2],
            index: 0,
        })
        .expect("the highlight exists");
    let image = render_doc(editor.document(), &capture);
    // The same pixels as objects placed in that order
    let placed = document(&capture, vec![marker, blue, red]);
    assert_eq!(image.rgba, render_doc(&placed, &capture).rgba);
    check_golden("reorder", &image);
}

#[test]
fn unannotated_rendering_is_the_image_exactly() {
    let capture = screenshot(64, 48);
    let doc = Document::new(Size::new(64, 48));
    let image = render_doc(&doc, &capture);
    assert_eq!(image.bounds, Rect::new(0, 0, 64, 48));
    assert_eq!(image.rgba, capture.rgba);
}
