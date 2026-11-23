//! Replays `parity-fixtures/image-to-3d/position-bounds.json` through the
//! canonical Windows glTF validator: declared POSITION bounds must contain the
//! committed vertices, so stale bounds are rejected and recomputed bounds pass.

use serde_json::{Value, json};

use super::*;

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../parity-fixtures/image-to-3d/position-bounds.json"
    ))
    .unwrap()
}

fn vector(value: &Value) -> [f64; 3] {
    let values: Vec<f64> = value
        .as_array()
        .unwrap()
        .iter()
        .map(|component| component.as_f64().unwrap())
        .collect();
    values.try_into().unwrap()
}

fn triangle(minimum: &Value, maximum: &Value) -> Value {
    json!({
        "asset": {"version": "2.0"},
        "buffers": [{"byteLength": 36}],
        "bufferViews": [{"buffer": 0, "byteLength": 36}],
        "accessors": [{
            "bufferView": 0,
            "componentType": 5126,
            "count": 3,
            "type": "VEC3",
            "min": minimum,
            "max": maximum
        }],
        "meshes": [{"primitives": [{"attributes": {"POSITION": 0}}]}],
        "nodes": [{"mesh": 0}],
        "scenes": [{"nodes": [0]}]
    })
}

#[test]
fn shared_position_bounds_reject_stale_and_accept_recomputed_extents() {
    let fixture = fixture();
    let positions: Vec<[f64; 3]> = fixture["positions"]
        .as_array()
        .unwrap()
        .iter()
        .map(vector)
        .collect();
    assert!(!positions.is_empty());

    // The recomputed extents in the fixture are the per-component envelope.
    let minimum = vector(&fixture["minimum"]);
    let maximum = vector(&fixture["maximum"]);
    for axis in 0..3 {
        let values = positions.iter().map(|position| position[axis]);
        assert_eq!(values.clone().fold(f64::INFINITY, f64::min), minimum[axis]);
        assert_eq!(values.fold(f64::NEG_INFINITY, f64::max), maximum[axis]);
    }

    // A triangle needs three vertices; the extra one repeats the first.
    let mut binary = Vec::with_capacity(36);
    for position in positions.iter().chain(positions.first()).take(3) {
        for component in position {
            binary.extend_from_slice(&(*component as f32).to_le_bytes());
        }
    }
    assert_eq!(binary.len(), 36);

    let current = triangle(&fixture["minimum"], &fixture["maximum"]);
    assert!(validate_gltf_semantics(&current, Some(&binary)).is_ok());
    let stale = triangle(&fixture["staleMinimum"], &fixture["staleMaximum"]);
    assert!(validate_gltf_semantics(&stale, Some(&binary)).is_err());
}
