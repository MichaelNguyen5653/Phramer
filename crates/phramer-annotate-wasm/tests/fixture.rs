//! The fixture the editor's wasm determinism test reads
//! (`app/src/editor/engine.test.ts`): the engine's own test capture, test
//! font and documents (`phramer-annotate/tests/common`), as edits, with the
//! checksums the native build renders them to. Those checksums are the ones
//! `phramer-annotate/tests/determinism.rs` pins, so a wasm build that renders
//! them through this crate draws the same bytes as the overlay.
//!
//! `PHRAMER_UPDATE_GOLDEN=1` rewrites the fixture; otherwise it must match.

#[path = "../../phramer-annotate/tests/common/mod.rs"]
mod common;

use common::{UPDATE_ENV, WORKSPACE, YELLOW, every_tool, pixel_font, screenshot};
use phramer_annotate::{Color, Document, Point, RedactStyle, Shape, ShapeStyle};
use phramer_annotate_wasm::{RENDER_HEADER, Session};
use serde_json::{Value, json};

/// `determinism.rs`'s pinned checksums: every tool, then the effects.
const NATIVE_CHECKSUMS: [u64; 2] = [0x2939_3a51_2111_1e0d, 0x1d8d_a383_06ac_73d6];

fn fixture_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("determinism.json")
}

/// FNV-1a, 64 bit, as `determinism.rs` computes it.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// `determinism.rs`'s second document.
fn effects_document() -> Vec<(Color, u32, Shape)> {
    let p = Point::new;
    vec![
        (
            YELLOW,
            3,
            Shape::Pixelate {
                start: p(70, 5),
                end: p(110, 45),
                style: RedactStyle::Frosted,
            },
        ),
        (
            YELLOW,
            1,
            Shape::Pixelate {
                start: p(5, 60),
                end: p(60, 95),
                style: RedactStyle::Insecure,
            },
        ),
        (
            YELLOW,
            4,
            Shape::Ellipse {
                start: p(80, 60),
                end: p(150, 120),
                style: ShapeStyle::Highlight,
                outline_width: 2,
            },
        ),
    ]
}

fn edits_of(document: &Document) -> Vec<(Color, u32, Shape)> {
    document
        .objects()
        .iter()
        .map(|object| (object.color, object.size, object.shape.clone()))
        .collect()
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, byte)| n | u32::from(*byte) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(TABLE[(n >> (18 - 6 * i) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Renders `items` the way the editor does: a session, one `add` request
/// per object, then a render.
fn checksum_through_session(items: &[(Color, u32, Shape)]) -> u64 {
    let capture = screenshot(120, 100);
    let mut session =
        Session::new(capture.width, capture.height, capture.rgba.clone(), false).expect("session");
    session
        .add_font("Pixel", false, false, pixel_font(), 0)
        .expect("test font");
    // determinism.rs pins the library's default: no made-up bold or italic.
    // A session starts with them on, as the overlay draws.
    let off = session
        .call(&json!({"op": "set_synthetic_styles", "on": false}).to_string())
        .expect("synthetic styles");
    assert_eq!(off, json!(false));
    for (color, size, shape) in items {
        let request = json!({"op": "apply", "edit": {"kind": "add", "color": color, "size": size, "shape": shape}});
        session.call(&request.to_string()).expect("drawable");
    }
    session.render(Vec::new(), WORKSPACE).expect("renders");
    // Bounds x and y, then the pixels: what determinism.rs hashes
    let mut bytes = session.out[..8].to_vec();
    bytes.extend_from_slice(&session.out[RENDER_HEADER..]);
    fnv1a(&bytes)
}

fn fixture() -> Value {
    let capture = screenshot(120, 100);
    let documents = [
        ("every_tool", edits_of(&every_tool())),
        ("effects", effects_document()),
    ];
    let documents: Vec<Value> = documents
        .iter()
        .zip(NATIVE_CHECKSUMS)
        .map(|((name, items), checksum)| {
            let edits: Vec<Value> = items
                .iter()
                .map(|(color, size, shape)| json!({"kind": "add", "color": color, "size": size, "shape": shape}))
                .collect();
            json!({"name": name, "edits": edits, "checksum": format!("{checksum:016x}")})
        })
        .collect();
    json!({
        "width": capture.width,
        "height": capture.height,
        "base": base64(&capture.rgba),
        "font_family": "Pixel",
        "font": base64(&pixel_font()),
        "workspace": [WORKSPACE.r, WORKSPACE.g, WORKSPACE.b, WORKSPACE.a],
        "documents": documents,
    })
}

#[test]
fn the_session_renders_the_pinned_native_checksums() {
    let sums = [
        checksum_through_session(&edits_of(&every_tool())),
        checksum_through_session(&effects_document()),
    ];
    assert_eq!(sums, NATIVE_CHECKSUMS, "checksums: {sums:#018x?}");
}

#[test]
fn the_wasm_fixture_is_current() {
    let path = fixture_path();
    let expected = serde_json::to_string_pretty(&fixture()).expect("fixture JSON") + "\n";
    if std::env::var(UPDATE_ENV).is_ok_and(|value| value == "1") {
        std::fs::write(&path, &expected).expect("write fixture");
        return;
    }
    let stored = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        stored.replace("\r\n", "\n") == expected,
        "{} is stale; run with {UPDATE_ENV}=1 to rewrite it",
        path.display()
    );
}
