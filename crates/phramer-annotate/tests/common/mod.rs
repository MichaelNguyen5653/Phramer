//! Shared test support: a generated pixel font, test base images, and the
//! golden-image comparison.

#![allow(dead_code)]

use std::io::Cursor;
use std::path::{Path, PathBuf};

use phramer_annotate::{
    ArrowHead, BaseImage, Color, Command, Document, Editor, FontLibrary, Point, RedactStyle,
    RenderOptions, RenderedImage, Shape, ShapeStyle, Size, TextAlign, TextFont, render,
};

/// Set to 1 to rewrite the golden PNGs from the current renderer.
pub const UPDATE_ENV: &str = "PHRAMER_UPDATE_GOLDEN";

// ---------------------------------------------------------------------------
// Pixel font
//
// A 5x7 block font built as a TrueType file in memory, so the tests need no
// font from the system or from a third party. Each lit cell is a square in
// the outline; the renderer fills it like any other glyph.

const GLYPHS: &[(char, [&str; 7])] = &[
    (
        '0',
        [
            ".###.", "#...#", "#..##", "#.#.#", "##..#", "#...#", ".###.",
        ],
    ),
    (
        '1',
        [
            "..#..", ".##..", "..#..", "..#..", "..#..", "..#..", ".###.",
        ],
    ),
    (
        '2',
        [
            ".###.", "#...#", "....#", "...#.", "..#..", ".#...", "#####",
        ],
    ),
    (
        '3',
        [
            "#####", "...#.", "..#..", "...#.", "....#", "#...#", ".###.",
        ],
    ),
    (
        '4',
        [
            "...#.", "..##.", ".#.#.", "#..#.", "#####", "...#.", "...#.",
        ],
    ),
    (
        '5',
        [
            "#####", "#....", "####.", "....#", "....#", "#...#", ".###.",
        ],
    ),
    (
        '6',
        [
            "..##.", ".#...", "#....", "####.", "#...#", "#...#", ".###.",
        ],
    ),
    (
        '7',
        [
            "#####", "....#", "...#.", "..#..", ".#...", ".#...", ".#...",
        ],
    ),
    (
        '8',
        [
            ".###.", "#...#", "#...#", ".###.", "#...#", "#...#", ".###.",
        ],
    ),
    (
        '9',
        [
            ".###.", "#...#", "#...#", ".####", "....#", "...#.", ".##..",
        ],
    ),
    (
        'A',
        [
            ".###.", "#...#", "#...#", "#####", "#...#", "#...#", "#...#",
        ],
    ),
    (
        'E',
        [
            "#####", "#....", "#....", "####.", "#....", "#....", "#####",
        ],
    ),
    (
        'H',
        [
            "#...#", "#...#", "#...#", "#####", "#...#", "#...#", "#...#",
        ],
    ),
    (
        'M',
        [
            "#...#", "##.##", "#.#.#", "#.#.#", "#...#", "#...#", "#...#",
        ],
    ),
    (
        'P',
        [
            "####.", "#...#", "#...#", "####.", "#....", "#....", "#....",
        ],
    ),
    (
        'R',
        [
            "####.", "#...#", "#...#", "####.", "#.#..", "#..#.", "#...#",
        ],
    ),
    (
        'S',
        [
            ".####", "#....", "#....", ".###.", "....#", "....#", "####.",
        ],
    ),
    (
        'T',
        [
            "#####", "..#..", "..#..", "..#..", "..#..", "..#..", "..#..",
        ],
    ),
    (
        'X',
        [
            "#...#", "#...#", ".#.#.", "..#..", ".#.#.", "#...#", "#...#",
        ],
    ),
];

const CELL: i16 = 100;
const ADVANCE: u16 = 600;
const LSB: i16 = 50;

/// One glyph as runs of lit cells: (x0, y0, x1, y1) in font units, y up.
fn runs(rows: &[&str; 7]) -> Vec<(i16, i16, i16, i16)> {
    let mut out = Vec::new();
    for (row, line) in rows.iter().enumerate() {
        let top = (7 - row as i16) * CELL;
        let bytes = line.as_bytes();
        let mut column = 0;
        while column < bytes.len() {
            if bytes[column] == b'#' {
                let start = column;
                while column < bytes.len() && bytes[column] == b'#' {
                    column += 1;
                }
                out.push((
                    LSB + start as i16 * CELL,
                    top - CELL,
                    LSB + column as i16 * CELL,
                    top,
                ));
            } else {
                column += 1;
            }
        }
    }
    out
}

/// A `glyf` entry: one clockwise square contour per run.
fn glyph_data(rects: &[(i16, i16, i16, i16)]) -> Vec<u8> {
    if rects.is_empty() {
        return Vec::new();
    }
    let x_min = rects.iter().map(|r| r.0).min().unwrap_or(0);
    let y_min = rects.iter().map(|r| r.1).min().unwrap_or(0);
    let x_max = rects.iter().map(|r| r.2).max().unwrap_or(0);
    let y_max = rects.iter().map(|r| r.3).max().unwrap_or(0);
    let mut out = Vec::new();
    let push16 = |out: &mut Vec<u8>, v: i16| out.extend_from_slice(&v.to_be_bytes());
    push16(&mut out, rects.len() as i16);
    for v in [x_min, y_min, x_max, y_max] {
        push16(&mut out, v);
    }
    for i in 0..rects.len() {
        out.extend_from_slice(&((i * 4 + 3) as u16).to_be_bytes());
    }
    out.extend_from_slice(&0u16.to_be_bytes());
    // Clockwise with y up: bottom-left, top-left, top-right, bottom-right.
    let points: Vec<(i16, i16)> = rects
        .iter()
        .flat_map(|&(x0, y0, x1, y1)| [(x0, y0), (x0, y1), (x1, y1), (x1, y0)])
        .collect();
    out.extend(std::iter::repeat_n(0x01u8, points.len()));
    let mut last = 0;
    for &(x, _) in &points {
        push16(&mut out, x - last);
        last = x;
    }
    last = 0;
    for &(_, y) in &points {
        push16(&mut out, y - last);
        last = y;
    }
    if out.len() % 2 == 1 {
        out.push(0);
    }
    out
}

/// The test font as TrueType bytes.
pub fn pixel_font() -> Vec<u8> {
    // Glyph 0 is the missing-glyph box, glyph 1 the space.
    let notdef = vec![(LSB, 0, LSB + 5 * CELL, 7 * CELL)];
    let mut glyphs: Vec<Vec<(i16, i16, i16, i16)>> = vec![notdef, Vec::new()];
    glyphs.extend(GLYPHS.iter().map(|(_, rows)| runs(rows)));
    let mut mapping: Vec<(u32, u32)> = vec![(' ' as u32, 1)];
    mapping.extend(
        GLYPHS
            .iter()
            .enumerate()
            .map(|(i, (c, _))| (*c as u32, i as u32 + 2)),
    );
    mapping.sort();
    let count = glyphs.len() as u16;

    let mut glyf = Vec::new();
    let mut loca = Vec::new();
    for glyph in &glyphs {
        loca.extend_from_slice(&(glyf.len() as u32).to_be_bytes());
        glyf.extend(glyph_data(glyph));
    }
    loca.extend_from_slice(&(glyf.len() as u32).to_be_bytes());

    let mut head = Vec::new();
    head.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // version
    head.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // revision
    head.extend_from_slice(&0u32.to_be_bytes()); // checksum adjustment
    head.extend_from_slice(&0x5F0F_3CF5u32.to_be_bytes()); // magic
    head.extend_from_slice(&0u16.to_be_bytes()); // flags
    head.extend_from_slice(&1000u16.to_be_bytes()); // units per em
    head.extend_from_slice(&[0; 16]); // created, modified
    for v in [0i16, -100, 600, 700] {
        head.extend_from_slice(&v.to_be_bytes());
    }
    head.extend_from_slice(&0u16.to_be_bytes()); // mac style
    head.extend_from_slice(&8u16.to_be_bytes()); // lowest ppem
    head.extend_from_slice(&2i16.to_be_bytes()); // direction hint
    head.extend_from_slice(&1i16.to_be_bytes()); // long loca
    head.extend_from_slice(&0i16.to_be_bytes()); // glyph data format

    let mut hhea = Vec::new();
    hhea.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    for v in [800i16, -200, 100] {
        hhea.extend_from_slice(&v.to_be_bytes()); // ascender, descender, line gap
    }
    hhea.extend_from_slice(&ADVANCE.to_be_bytes());
    for v in [0i16, 0, 600, 1, 0, 0, 0, 0, 0, 0, 0] {
        hhea.extend_from_slice(&v.to_be_bytes());
    }
    hhea.extend_from_slice(&count.to_be_bytes());

    let mut maxp = Vec::new();
    maxp.extend_from_slice(&0x0000_5000u32.to_be_bytes());
    maxp.extend_from_slice(&count.to_be_bytes());

    let mut hmtx = Vec::new();
    for _ in 0..count {
        hmtx.extend_from_slice(&ADVANCE.to_be_bytes());
        hmtx.extend_from_slice(&LSB.to_be_bytes());
    }

    // cmap: one Unicode subtable, format 12, one group per character.
    let mut cmap = Vec::new();
    cmap.extend_from_slice(&0u16.to_be_bytes());
    cmap.extend_from_slice(&1u16.to_be_bytes());
    cmap.extend_from_slice(&0u16.to_be_bytes()); // platform Unicode
    cmap.extend_from_slice(&4u16.to_be_bytes()); // full repertoire
    cmap.extend_from_slice(&12u32.to_be_bytes());
    let length = 16 + 12 * mapping.len() as u32;
    cmap.extend_from_slice(&12u16.to_be_bytes());
    cmap.extend_from_slice(&0u16.to_be_bytes());
    cmap.extend_from_slice(&length.to_be_bytes());
    cmap.extend_from_slice(&0u32.to_be_bytes());
    cmap.extend_from_slice(&(mapping.len() as u32).to_be_bytes());
    for (code, glyph) in &mapping {
        cmap.extend_from_slice(&code.to_be_bytes());
        cmap.extend_from_slice(&code.to_be_bytes());
        cmap.extend_from_slice(&glyph.to_be_bytes());
    }

    let tables: [(&[u8; 4], Vec<u8>); 7] = [
        (b"cmap", cmap),
        (b"glyf", glyf),
        (b"head", head),
        (b"hhea", hhea),
        (b"hmtx", hmtx),
        (b"loca", loca),
        (b"maxp", maxp),
    ];
    let mut font = Vec::new();
    font.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    font.extend_from_slice(&(tables.len() as u16).to_be_bytes());
    for v in [64u16, 2, 48] {
        font.extend_from_slice(&v.to_be_bytes()); // search range, selector, shift
    }
    let mut offset = 12 + 16 * tables.len();
    let mut body = Vec::new();
    for (tag, data) in &tables {
        font.extend_from_slice(*tag);
        font.extend_from_slice(&0u32.to_be_bytes());
        font.extend_from_slice(&(offset as u32).to_be_bytes());
        font.extend_from_slice(&(data.len() as u32).to_be_bytes());
        body.extend_from_slice(data);
        while body.len() % 4 != 0 {
            body.push(0);
        }
        offset = 12 + 16 * tables.len() + body.len();
    }
    font.extend(body);
    font
}

pub fn fonts() -> FontLibrary {
    let mut fonts = FontLibrary::new();
    fonts
        .add("Pixel", false, false, pixel_font())
        .expect("the test font parses");
    fonts
}

// ---------------------------------------------------------------------------
// Base images

/// Straight RGBA8 pixels of a test capture.
pub struct Capture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Capture {
    pub fn new(width: u32, height: u32, pixel: impl Fn(u32, u32) -> [u8; 4]) -> Self {
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                rgba.extend_from_slice(&pixel(x, y));
            }
        }
        Self {
            width,
            height,
            rgba,
        }
    }

    pub fn base(&self) -> BaseImage<'_> {
        BaseImage::new(self.width, self.height, &self.rgba).expect("buffer matches its size")
    }
}

/// A light, slightly graded "window" with a checkerboard panel at the top
/// right and a dark bar near the bottom, so tools that read pixels have
/// something to show.
pub fn screenshot(width: u32, height: u32) -> Capture {
    Capture::new(width, height, |x, y| {
        let checker = x >= width * 3 / 5 && x < width - 8 && (8..height / 2).contains(&y);
        if checker {
            return if ((x / 6) + (y / 6)) % 2 == 0 {
                [30, 60, 160, 255]
            } else {
                [250, 210, 40, 255]
            };
        }
        if (height * 2 / 3..height * 2 / 3 + 18).contains(&y) && (8..width - 8).contains(&x) {
            return [36, 36, 40, 255];
        }
        [(235 - x / 5) as u8, (240 - y / 4) as u8, 248, 255]
    })
}

pub const RED: Color = Color::rgb(220, 40, 40);
pub const GREEN: Color = Color::rgb(40, 170, 80);
pub const BLUE: Color = Color::rgb(40, 90, 220);
pub const YELLOW: Color = Color::rgb(255, 220, 0);
pub const WORKSPACE: Color = Color::rgb(0xE8, 0xEA, 0xED);

// ---------------------------------------------------------------------------
// Golden comparison

fn golden_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join(format!("{name}.png"))
}

fn failure_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join("golden-failures")
}

fn encode(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("png header");
        writer.write_image_data(rgba).expect("png data");
        writer.finish().expect("png end");
    }
    out
}

fn decode(bytes: &[u8]) -> (u32, u32, Vec<u8>) {
    let mut reader = png::Decoder::new(Cursor::new(bytes))
        .read_info()
        .expect("golden png header");
    let mut buffer = vec![0; reader.output_buffer_size().expect("golden png size")];
    let info = reader.next_frame(&mut buffer).expect("golden png data");
    assert_eq!(
        info.color_type,
        png::ColorType::Rgba,
        "golden PNGs are RGBA8"
    );
    buffer.truncate(info.buffer_size());
    (info.width, info.height, buffer)
}

/// Renders `document` and compares the pixels with `tests/golden/<name>.png`.
pub fn render_doc(document: &Document, capture: &Capture) -> RenderedImage {
    let options = RenderOptions {
        workspace_color: WORKSPACE,
    };
    render(document, capture.base(), &fonts(), &options).expect("document renders")
}

pub fn check_golden(name: &str, image: &RenderedImage) {
    let (width, height) = (image.bounds.width, image.bounds.height);
    let path = golden_path(name);
    if std::env::var(UPDATE_ENV).is_ok_and(|v| v == "1") {
        std::fs::create_dir_all(path.parent().expect("golden dir")).expect("create golden dir");
        std::fs::write(&path, encode(width, height, &image.rgba)).expect("write golden");
        return;
    }
    let expected = std::fs::read(&path).unwrap_or_else(|e| {
        panic!(
            "missing golden {} ({e}); run with {UPDATE_ENV}=1 to create it",
            path.display()
        )
    });
    let (ew, eh, epixels) = decode(&expected);
    if (ew, eh) == (width, height) && epixels == image.rgba {
        return;
    }
    let dir = failure_dir();
    std::fs::create_dir_all(&dir).expect("create failure dir");
    let actual_path = dir.join(format!("{name}.actual.png"));
    let diff_path = dir.join(format!("{name}.diff.png"));
    std::fs::write(&actual_path, encode(width, height, &image.rgba)).expect("write actual");
    let mut differing = 0usize;
    if (ew, eh) == (width, height) {
        // Red where pixels differ, a faded copy of the actual elsewhere.
        let mut diff = Vec::with_capacity(image.rgba.len());
        for (a, e) in image.rgba.chunks(4).zip(epixels.chunks(4)) {
            if a == e {
                diff.extend_from_slice(&[a[0] / 4 + 191, a[1] / 4 + 191, a[2] / 4 + 191, 255]);
            } else {
                differing += 1;
                diff.extend_from_slice(&[255, 0, 0, 255]);
            }
        }
        std::fs::write(&diff_path, encode(width, height, &diff)).expect("write diff");
    }
    panic!(
        "{name}: rendering differs from {} ({differing} pixels differ; expected {ew}x{eh}, got \
         {width}x{height}).\n  actual: {}\n  diff:   {}\nRun with {UPDATE_ENV}=1 to accept it.",
        path.display(),
        actual_path.display(),
        if differing > 0 {
            diff_path.display().to_string()
        } else {
            "(sizes differ)".into()
        },
    );
}

// ---------------------------------------------------------------------------
// Shared documents

fn p(x: i32, y: i32) -> Point {
    Point::new(x, y)
}

/// A 120x100 document holding one object of every tool.
pub fn every_tool() -> Document {
    let shapes = vec![
        Shape::Pencil {
            points: vec![p(1, 2), p(30, 40), p(-5, 60)],
        },
        Shape::Line {
            start: p(0, 0),
            end: p(50, 10),
        },
        Shape::Arrow {
            start: p(5, 5),
            end: p(90, 70),
            head: ArrowHead::Curved,
            reversed: true,
        },
        Shape::Rectangle {
            start: p(10, 10),
            end: p(40, 30),
            style: ShapeStyle::Highlight,
            outline_width: 3,
        },
        Shape::Ellipse {
            start: p(60, 10),
            end: p(90, 40),
            style: ShapeStyle::Outline,
            outline_width: 2,
        },
        Shape::Marker {
            start: p(0, 80),
            end: p(100, 80),
        },
        Shape::Text {
            top_left: p(-20, 90),
            text: "SET 42\nTEXT".into(),
            font: TextFont {
                family: "Pixel".into(),
                bold: true,
                italic: false,
                underline: true,
                strikeout: false,
            },
            align: TextAlign::Center,
        },
        Shape::Pixelate {
            start: p(70, 50),
            end: p(110, 80),
            style: RedactStyle::Insecure,
        },
        Shape::Invert {
            start: p(0, 0),
            end: p(20, 20),
        },
        Shape::CircleCounter {
            center: p(100, 20),
            pointer: p(140, 60),
            count: 0,
        },
    ];
    let mut editor = Editor::new(Document::new(Size::new(120, 100)));
    for (i, shape) in shapes.into_iter().enumerate() {
        let color = Color::rgba(i as u8 * 20, 100, 200, 255 - i as u8);
        editor
            .apply(Command::Add {
                color,
                size: i as u32,
                shape,
            })
            .expect("drawable");
    }
    editor.into_document()
}
