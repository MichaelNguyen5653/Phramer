//! The renderer's output is pinned by a checksum, so the same test proves
//! that a native build and a wasm build draw identical bytes. Natively it
//! runs with `cargo test`; for wasm, build it with
//! `cargo test -p phramer-annotate --test determinism --target wasm32-wasip1 --no-run`
//! and run the printed `.wasm` under any WASI runtime (Node's `node:wasi`
//! works). It must pass on both.

mod common;

use common::*;
use phramer_annotate::{Command, Document, Editor, Point, RedactStyle, Shape, ShapeStyle, Size};

/// FNV-1a, 64 bit: small, and the same on every target.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn checksum(document: &Document, capture: &Capture) -> u64 {
    let image = render_doc(document, capture);
    let mut bytes = Vec::new();
    for v in [image.bounds.x, image.bounds.y] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    bytes.extend_from_slice(&image.rgba);
    fnv1a(&bytes)
}

/// Effects not covered by the every-tool document: frosted fill, blur,
/// and highlight against the workspace.
fn effects_document() -> Document {
    let p = Point::new;
    let mut editor = Editor::new(Document::new(Size::new(120, 100)));
    for (size, shape) in [
        (
            3,
            Shape::Pixelate {
                start: p(70, 5),
                end: p(110, 45),
                style: RedactStyle::Frosted,
            },
        ),
        (
            1,
            Shape::Pixelate {
                start: p(5, 60),
                end: p(60, 95),
                style: RedactStyle::Insecure,
            },
        ),
        (
            4,
            Shape::Ellipse {
                start: p(80, 60),
                end: p(150, 120),
                style: ShapeStyle::Highlight,
                outline_width: 2,
            },
        ),
    ] {
        editor
            .apply(Command::Add {
                color: YELLOW,
                size,
                shape,
            })
            .expect("drawable");
    }
    editor.into_document()
}

#[test]
fn rendering_is_byte_for_byte_stable() {
    let capture = screenshot(120, 100);
    let sums = [
        checksum(&every_tool(), &capture),
        checksum(&effects_document(), &capture),
    ];
    // Regenerate by printing `sums` if the renderer changes on purpose.
    assert_eq!(
        sums,
        [0x2939_3a51_2111_1e0d, 0x1d8d_a383_06ac_73d6],
        "checksums changed: {sums:#018x?}"
    );
}
