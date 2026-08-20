use base64::{Engine as _, engine::general_purpose};
use serde_json::{Value, json};

use super::*;

#[test]
fn binary_chunk_must_back_exactly_buffer_zero() {
    let encoded = general_purpose::STANDARD.encode([0_u8; 36]);
    let embedded = triangle_document(
        json!([{
            "byteLength": 36,
            "uri": format!("data:application/octet-stream;base64,{encoded}")
        }]),
        json!([{"buffer": 0, "byteLength": 36}]),
        basic_position_accessor(),
    );
    assert!(validate_gltf_semantics(&embedded, None).is_ok());
    assert!(validate_gltf_semantics(&embedded, Some(&[0; 36])).is_err());

    let mut later_uri_less = embedded;
    later_uri_less["buffers"]
        .as_array_mut()
        .unwrap()
        .push(json!({"byteLength": 4}));
    assert!(validate_gltf_semantics(&later_uri_less, None).is_err());
}

#[test]
fn vertex_attributes_require_four_byte_absolute_alignment() {
    let document = triangle_document(
        json!([{"byteLength": 62}]),
        json!([
            {"buffer": 0, "byteLength": 36},
            {"buffer": 0, "byteOffset": 38, "byteLength": 24}
        ]),
        json!([
            {
                "bufferView": 0,
                "componentType": 5126,
                "count": 3,
                "type": "VEC3"
            },
            {
                "bufferView": 1,
                "componentType": 5123,
                "normalized": true,
                "count": 3,
                "type": "VEC4"
            }
        ]),
    );
    let mut document = document;
    document["meshes"][0]["primitives"][0]["attributes"]["COLOR_0"] = json!(1);
    assert!(validate_gltf_semantics(&document, Some(&[0; 64])).is_err());
}

#[test]
fn accessor_bounds_and_committed_float_values_are_verified() {
    let mut bounded = binary_triangle();
    bounded["accessors"][0]["min"] = json!([-1, -1, -1]);
    bounded["accessors"][0]["max"] = json!([1, 1, 1]);
    assert!(validate_gltf_semantics(&bounded, Some(&[0; 36])).is_ok());

    let mut reversed = bounded.clone();
    reversed["accessors"][0]["min"] = json!([2, -1, -1]);
    assert!(validate_gltf_semantics(&reversed, Some(&[0; 36])).is_err());

    let mut outside = [0_u8; 36];
    outside[..4].copy_from_slice(&2_f32.to_le_bytes());
    assert!(validate_gltf_semantics(&bounded, Some(&outside)).is_err());

    let mut quantized = [0_u8; 36];
    let tolerated = (1.0 + floats::POSITION_BOUNDS_ABSOLUTE_TOLERANCE / 2.0) as f32;
    quantized[..4].copy_from_slice(&tolerated.to_le_bytes());
    assert!(validate_gltf_semantics(&bounded, Some(&quantized)).is_ok());
    let excessive_drift = (1.0 + floats::POSITION_BOUNDS_ABSOLUTE_TOLERANCE * 2.0) as f32;
    quantized[..4].copy_from_slice(&excessive_drift.to_le_bytes());
    assert!(validate_gltf_semantics(&bounded, Some(&quantized)).is_err());

    let mut non_finite = [0_u8; 36];
    non_finite[..4].copy_from_slice(&f32::NAN.to_le_bytes());
    let unbounded = binary_triangle();
    assert!(validate_gltf_semantics(&unbounded, Some(&non_finite)).is_err());

    let mut excessive = [0_u8; 36];
    excessive[..4].copy_from_slice(&20_000_000_f32.to_le_bytes());
    assert!(validate_gltf_semantics(&unbounded, Some(&excessive)).is_err());

    let mut morph = triangle_document(
        json!([{"byteLength": 72}]),
        json!([
            {"buffer": 0, "byteLength": 36},
            {"buffer": 0, "byteOffset": 36, "byteLength": 36}
        ]),
        json!([
            {
                "bufferView": 0,
                "componentType": 5126,
                "count": 3,
                "type": "VEC3"
            },
            {
                "bufferView": 1,
                "componentType": 5126,
                "count": 3,
                "type": "VEC3",
                "min": [-1, -1, -1],
                "max": [1, 1, 1]
            }
        ]),
    );
    morph["meshes"][0]["primitives"][0]["targets"] = json!([{"POSITION": 1}]);
    let mut morph_bytes = [0_u8; 72];
    morph_bytes[36..40].copy_from_slice(&2_f32.to_le_bytes());
    assert!(validate_gltf_semantics(&morph, Some(&morph_bytes)).is_err());
}

#[test]
fn position_bound_tolerances_match_the_shared_contract() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../parity-fixtures/image-to-3d/state-contract.json"
    ))
    .unwrap();
    let safety = &fixture["modelSafety"];
    assert_eq!(
        safety["maximumPositionBoundsAbsoluteTolerance"].as_f64(),
        Some(floats::POSITION_BOUNDS_ABSOLUTE_TOLERANCE)
    );
    assert_eq!(
        safety["maximumPositionBoundsRelativeTolerance"].as_f64(),
        Some(floats::POSITION_BOUNDS_RELATIVE_TOLERANCE)
    );
}

#[test]
fn node_depth_and_renderer_json_values_are_bounded() {
    let mut excessive_transform = binary_triangle();
    excessive_transform["nodes"][0]["translation"] = json!([20_000_000, 0, 0]);
    assert!(validate_gltf_semantics(&excessive_transform, Some(&[0; 36])).is_err());

    let mut deep = binary_triangle();
    let nodes = (0..=MAX_GLTF_NODE_DEPTH)
        .map(|index| {
            if index == MAX_GLTF_NODE_DEPTH {
                json!({"mesh": 0})
            } else {
                json!({"children": [index + 1]})
            }
        })
        .collect::<Vec<_>>();
    deep["nodes"] = Value::Array(nodes);
    assert!(validate_gltf_semantics(&deep, Some(&[0; 36])).is_err());
}

fn binary_triangle() -> Value {
    triangle_document(
        json!([{"byteLength": 36}]),
        json!([{"buffer": 0, "byteLength": 36}]),
        basic_position_accessor(),
    )
}

fn basic_position_accessor() -> Value {
    json!([{
        "bufferView": 0,
        "componentType": 5126,
        "count": 3,
        "type": "VEC3"
    }])
}

#[test]
fn face_loop_lines_are_accepted_beside_the_surface_they_describe() {
    // A quad mesh is delivered as a triangulated surface plus the original face
    // loops, so a viewer can draw a quad as one face instead of two triangles.
    let mut document = triangle_document(
        json!([{"byteLength": 60}]),
        json!([
            {"buffer": 0, "byteLength": 36},
            {"buffer": 0, "byteOffset": 36, "byteLength": 24}
        ]),
        json!([
            {
                "bufferView": 0,
                "componentType": 5126,
                "count": 3,
                "type": "VEC3",
                "min": [0.0, 0.0, 0.0],
                "max": [1.0, 1.0, 1.0]
            },
            {"bufferView": 1, "componentType": 5125, "count": 6, "type": "SCALAR"}
        ]),
    );
    document["meshes"][0]["primitives"] = json!([
        {"attributes": {"POSITION": 0}, "mode": 4},
        {"attributes": {"POSITION": 0}, "indices": 1, "mode": 1}
    ]);
    assert!(validate_gltf_semantics(&document, Some(&[0; 60])).is_ok());
}

#[test]
fn line_geometry_must_still_pair_its_indices() {
    let mut document = triangle_document(
        json!([{"byteLength": 56}]),
        json!([
            {"buffer": 0, "byteLength": 36},
            {"buffer": 0, "byteOffset": 36, "byteLength": 20}
        ]),
        json!([
            {
                "bufferView": 0,
                "componentType": 5126,
                "count": 3,
                "type": "VEC3",
                "min": [0.0, 0.0, 0.0],
                "max": [1.0, 1.0, 1.0]
            },
            {"bufferView": 1, "componentType": 5125, "count": 5, "type": "SCALAR"}
        ]),
    );
    document["meshes"][0]["primitives"] = json!([
        {"attributes": {"POSITION": 0}, "indices": 1, "mode": 1}
    ]);
    assert!(validate_gltf_semantics(&document, Some(&[0; 56])).is_err());
}

#[test]
fn every_mode_other_than_triangles_and_lines_stays_rejected() {
    for mode in [0, 2, 3, 5, 6, 7, u64::MAX] {
        let mut document = triangle_document(
            json!([{"byteLength": 36}]),
            json!([{"buffer": 0, "byteLength": 36}]),
            basic_position_accessor(),
        );
        document["meshes"][0]["primitives"][0]["mode"] = json!(mode);
        assert!(
            validate_gltf_semantics(&document, Some(&[0; 36])).is_err(),
            "mode {mode} must be rejected"
        );
    }
}

fn triangle_document(buffers: Value, buffer_views: Value, accessors: Value) -> Value {
    json!({
        "asset": {"version": "2.0"},
        "buffers": buffers,
        "bufferViews": buffer_views,
        "accessors": accessors,
        "meshes": [{"primitives": [{"attributes": {"POSITION": 0}}]}],
        "nodes": [{"mesh": 0}],
        "scenes": [{"nodes": [0]}]
    })
}
