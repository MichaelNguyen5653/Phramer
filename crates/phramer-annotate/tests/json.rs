//! A document with every tool survives JSON unchanged, and renders the same
//! after the trip.

mod common;

use common::*;
use phramer_annotate::{Document, ToolKind};

#[test]
fn every_tool_round_trips_through_json() {
    let doc = every_tool();
    let kinds: Vec<ToolKind> = doc.objects().iter().map(|o| o.shape.kind()).collect();
    assert_eq!(kinds, ToolKind::ALL);

    let json = doc.to_json().expect("serialises");
    let back = Document::from_json(&json).expect("deserialises");
    assert_eq!(back, doc);
    assert_eq!(back.to_json().expect("serialises again"), json);

    let capture = screenshot(120, 100);
    assert_eq!(render_doc(&back, &capture), render_doc(&doc, &capture));
}

#[test]
fn json_is_tagged_by_tool_name() {
    let json = every_tool().to_json().expect("serialises");
    for name in [
        "\"tool\":\"pencil\"",
        "\"tool\":\"line\"",
        "\"tool\":\"arrow\"",
        "\"tool\":\"rectangle\"",
        "\"tool\":\"ellipse\"",
        "\"tool\":\"marker\"",
        "\"tool\":\"text\"",
        "\"tool\":\"pixelate\"",
        "\"tool\":\"invert\"",
        "\"tool\":\"circle_counter\"",
        "\"style\":\"highlight\"",
        "\"head\":\"curved\"",
    ] {
        assert!(json.contains(name), "{name} missing from {json}");
    }
}

#[test]
fn malformed_json_is_an_error_not_a_panic() {
    assert!(Document::from_json("{").is_err());
    assert!(Document::from_json(r#"{"image_size":{"width":1}}"#).is_err());
    assert!(
        Document::from_json(
            r#"{"image_size":{"width":1,"height":1},"objects":[{"id":0,"color":{"r":0,"g":0,"b":0},
            "size":1,"shape":{"tool":"hexagon"}}]}"#
        )
        .is_err()
    );
}
