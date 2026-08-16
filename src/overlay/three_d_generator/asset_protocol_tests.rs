use super::*;

#[test]
fn glb_header_length_must_match_the_committed_file() {
    let path = std::env::temp_dir().join(format!(
        "sgt-invalid-glb-{}-{}.glb",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let mut bytes = Vec::from(&b"glTF\x02\0\0\0"[..]);
    bytes.extend_from_slice(&20_u32.to_le_bytes());
    bytes.extend_from_slice(&[0; 4]);
    std::fs::write(&path, bytes).unwrap();
    assert!(validate_glb(&path).is_err());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn chunk_table_and_external_uris_are_rejected() {
    let root = std::env::temp_dir().join(format!(
        "sgt-glb-validation-{}-{}",
        std::process::id(),
        TOKEN_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    let valid = root.join("valid.glb");
    let external = root.join("external.glb");
    let geometry = br#"{"asset":{"version":"2.0"},"buffers":[{"byteLength":36}],"bufferViews":[{"buffer":0,"byteLength":36}],"accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}],"meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],"nodes":[{"mesh":0}],"scenes":[{"nodes":[0]}],"scene":0}"#;
    write_glb(&valid, geometry, &[0; 36]);
    write_glb(
        &external,
        br#"{"asset":{"version":"2.0"},"buffers":[{"byteLength":36},{"byteLength":4,"uri":"https://example.com/a.bin"}],"bufferViews":[{"buffer":0,"byteLength":36}],"accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}],"meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],"nodes":[{"mesh":0}],"scenes":[{"nodes":[0]}]}"#,
        &[0; 36],
    );
    assert!(validate_glb(&valid).is_ok());
    assert!(validate_glb(&external).is_err());
    let no_geometry = root.join("no-geometry.glb");
    write_glb(
        &no_geometry,
        br#"{"asset":{"version":"2.0"},"buffers":[{"byteLength":4}],"bufferViews":[{"buffer":0,"byteLength":4}],"accessors":[{"bufferView":0,"componentType":5126,"count":1,"type":"SCALAR"}]}"#,
        &[0; 4],
    );
    assert!(validate_glb(&no_geometry).is_err());
    let out_of_bounds = root.join("out-of-bounds.glb");
    write_glb(
        &out_of_bounds,
        br#"{"asset":{"version":"2.0"},"buffers":[{"byteLength":12}],"bufferViews":[{"buffer":0,"byteLength":12}],"accessors":[{"bufferView":0,"byteOffset":4,"componentType":5126,"count":1,"type":"VEC3"}],"meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}]}"#,
        &[0; 12],
    );
    assert!(validate_glb(&out_of_bounds).is_err());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn aggregate_geometry_budgets_are_enforced() {
    let position = AccessorInfo {
        count: 1,
        component_type: 5126,
        component_count: 3,
        ..AccessorInfo::default()
    };
    let shared_vertices = AccessorInfo {
        count: (MAX_GLTF_VERTICES / 2) + 1,
        ..position
    };
    let root = serde_json::json!({
        "meshes": [{"primitives": [
            {"attributes": {"POSITION": 0}},
            {"attributes": {"POSITION": 0}}
        ]}]
    });
    assert!(
        semantics::validate(root.as_object().unwrap(), &[shared_vertices]).is_err(),
        "reusing one accessor must still count each primitive's vertices"
    );

    let shared_indices = AccessorInfo {
        count: (MAX_GLTF_INDICES / 2) + 1,
        component_type: 5125,
        component_count: 1,
        ..AccessorInfo::default()
    };
    let root = serde_json::json!({
        "meshes": [{"primitives": [
            {"attributes": {"POSITION": 0}, "indices": 1},
            {"attributes": {"POSITION": 0}, "indices": 1}
        ]}]
    });
    assert!(
        semantics::validate(root.as_object().unwrap(), &[position, shared_indices]).is_err(),
        "reusing one accessor must still count each primitive's indices"
    );
}

#[test]
fn morph_and_scene_graph_expansion_are_bounded() {
    let accessor = AccessorInfo {
        count: 1,
        component_type: 5126,
        component_count: 3,
        ..AccessorInfo::default()
    };
    let targets: Vec<Value> = (0..=MAX_GLTF_MORPH_TARGETS)
        .map(|_| serde_json::json!({"POSITION": 1}))
        .collect();
    let root = serde_json::json!({
        "meshes": [{"primitives": [{
            "attributes": {"POSITION": 0},
            "targets": targets
        }]}]
    });
    assert!(semantics::validate(root.as_object().unwrap(), &[accessor; 2]).is_err());

    let root = serde_json::json!({
        "meshes": [{"primitives": [{"attributes": {"POSITION": 0}}]}],
        "nodes": [{"children": [1]}, {"children": [0]}]
    });
    assert!(
        semantics::validate(root.as_object().unwrap(), &[accessor]).is_err(),
        "cyclic node graphs must never reach the viewer"
    );
}

#[test]
fn ordinary_bounded_scene_is_accepted() {
    let accessor = AccessorInfo {
        count: 3,
        component_type: 5126,
        component_count: 3,
        ..AccessorInfo::default()
    };
    let root = serde_json::json!({
        "materials": [{}],
        "meshes": [{"primitives": [{
            "attributes": {"POSITION": 0},
            "material": 0
        }]}],
        "nodes": [{"mesh": 0}, {"children": [0]}],
        "scenes": [{"nodes": [1]}]
    });
    assert!(semantics::validate(root.as_object().unwrap(), &[accessor]).is_ok());
}

#[test]
fn repeated_mesh_instances_cannot_multiply_viewer_work_without_bound() {
    let accessor = AccessorInfo {
        count: (MAX_GLTF_VERTICES / 2) + 1,
        component_type: 5126,
        component_count: 3,
        ..AccessorInfo::default()
    };
    let root = serde_json::json!({
        "meshes": [{"primitives": [{"attributes": {"POSITION": 0}}]}],
        "nodes": [{"mesh": 0}, {"mesh": 0}]
    });
    assert!(semantics::validate(root.as_object().unwrap(), &[accessor]).is_err());
}

#[test]
fn buffer_table_and_unused_viewer_features_are_bounded() {
    let buffers = vec![
        serde_json::json!({
            "byteLength": 1,
            "uri": "data:application/octet-stream;base64,AA=="
        });
        MAX_GLTF_BUFFERS + 1
    ];
    let root = serde_json::json!({
        "asset": {"version": "2.0"},
        "buffers": buffers
    });
    assert!(validate_gltf_semantics(&root, None).is_err());

    let accessor = AccessorInfo {
        count: 3,
        component_type: 5126,
        component_count: 3,
        ..AccessorInfo::default()
    };
    for field in ["animations", "skins", "cameras"] {
        let mut root = serde_json::json!({
            "meshes": [{"primitives": [{"attributes": {"POSITION": 0}}]}],
            "nodes": [{"mesh": 0}],
            "scenes": [{"nodes": [0]}]
        });
        root.as_object_mut()
            .unwrap()
            .insert(field.to_string(), serde_json::json!([{}]));
        assert!(features::validate(root.as_object().unwrap()).is_err());
        assert!(semantics::validate(root.as_object().unwrap(), &[accessor]).is_ok());
    }
}

#[test]
fn geometry_modes_morph_weights_and_node_transforms_are_strict() {
    let accessor = AccessorInfo {
        count: 3,
        component_type: 5126,
        component_count: 3,
        ..AccessorInfo::default()
    };
    for root in [
        serde_json::json!({
            "meshes": [{"primitives": [{"mode": 0, "attributes": {"POSITION": 0}}]}]
        }),
        serde_json::json!({
            "meshes": [{"weights": [0.5], "primitives": [
                {"attributes": {"POSITION": 0}}
            ]}]
        }),
        serde_json::json!({
            "meshes": [{"primitives": [{"attributes": {"POSITION": 0}}]}],
            "nodes": [{"mesh": 0, "matrix": [1, 0], "translation": [0, 0, 0]}]
        }),
    ] {
        assert!(semantics::validate(root.as_object().unwrap(), &[accessor]).is_err());
    }
}

#[test]
fn selected_scene_must_contain_geometry_and_scene_roots_are_not_cloned() {
    let accessor = AccessorInfo {
        count: 3,
        component_type: 5126,
        component_count: 3,
        ..AccessorInfo::default()
    };
    let missing_scene = serde_json::json!({
        "meshes": [{"primitives": [{"attributes": {"POSITION": 0}}]}],
        "nodes": [{"mesh": 0}]
    });
    assert!(semantics::validate(missing_scene.as_object().unwrap(), &[accessor]).is_err());

    let empty_selected = serde_json::json!({
        "meshes": [{"primitives": [{"attributes": {"POSITION": 0}}]}],
        "nodes": [{"mesh": 0}],
        "scenes": [{}, {"nodes": [0]}],
        "scene": 0
    });
    assert!(semantics::validate(empty_selected.as_object().unwrap(), &[accessor]).is_err());

    let repeated_root = serde_json::json!({
        "meshes": [{"primitives": [{"attributes": {"POSITION": 0}}]}],
        "nodes": [{"mesh": 0}],
        "scenes": [{"nodes": [0]}, {"nodes": [0]}]
    });
    assert!(semantics::validate(repeated_root.as_object().unwrap(), &[accessor]).is_err());
}

#[test]
fn typed_array_alignment_and_interleaved_tail_are_loader_safe() {
    let misaligned = triangle_document(
        serde_json::json!([{"buffer": 0, "byteLength": 40}]),
        serde_json::json!([{
            "bufferView": 0,
            "byteOffset": 2,
            "componentType": 5126,
            "count": 3,
            "type": "VEC3"
        }]),
        40,
    );
    assert!(validate_gltf_semantics(&misaligned, Some(&[0; 40])).is_err());

    let short_interleaved = triangle_document(
        serde_json::json!([{
            "buffer": 0,
            "byteLength": 44,
            "byteStride": 16
        }]),
        serde_json::json!([{
            "bufferView": 0,
            "componentType": 5126,
            "count": 3,
            "type": "VEC3"
        }]),
        44,
    );
    assert!(validate_gltf_semantics(&short_interleaved, Some(&[0; 44])).is_err());
}

#[test]
fn overlapping_buffer_views_cannot_amplify_loader_copies() {
    let views = vec![serde_json::json!({"buffer": 0, "byteLength": 4_000}); MAX_GLTF_BUFFER_VIEWS];
    let root = triangle_document(
        Value::Array(views),
        serde_json::json!([{
            "bufferView": 0,
            "componentType": 5126,
            "count": 3,
            "type": "VEC3"
        }]),
        4_000,
    );
    assert!(validate_gltf_semantics(&root, Some(&[0; 4_000])).is_err());
}

#[test]
fn primitive_indices_are_scanned_before_the_viewer() {
    let root = serde_json::json!({
        "asset": {"version": "2.0"},
        "buffers": [{"byteLength": 39}],
        "bufferViews": [
            {"buffer": 0, "byteLength": 36},
            {"buffer": 0, "byteOffset": 36, "byteLength": 3}
        ],
        "accessors": [
            {"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3"},
            {"bufferView": 1, "componentType": 5121, "count": 3, "type": "SCALAR"}
        ],
        "meshes": [{"primitives": [{
            "attributes": {"POSITION": 0},
            "indices": 1
        }]}],
        "nodes": [{"mesh": 0}],
        "scenes": [{"nodes": [0]}]
    });
    let mut binary = [0_u8; 40];
    binary[36..39].copy_from_slice(&[0, 1, 2]);
    assert!(validate_gltf_semantics(&root, Some(&binary)).is_ok());
    binary[38] = 3;
    assert!(validate_gltf_semantics(&root, Some(&binary)).is_err());
}

#[test]
fn logical_buffer_lengths_and_alignment_padding_are_exact() {
    let root = triangle_document(
        serde_json::json!([{"buffer": 0, "byteLength": 36}]),
        serde_json::json!([{
            "bufferView": 0,
            "componentType": 5126,
            "count": 3,
            "type": "VEC3"
        }]),
        37,
    );
    let mut binary = [0_u8; 40];
    assert!(validate_gltf_semantics(&root, Some(&binary)).is_ok());
    binary[39] = 1;
    assert!(validate_gltf_semantics(&root, Some(&binary)).is_err());

    let mut embedded = triangle_document(
        serde_json::json!([{"buffer": 0, "byteLength": 36}]),
        serde_json::json!([{
            "bufferView": 0,
            "componentType": 5126,
            "count": 3,
            "type": "VEC3"
        }]),
        36,
    );
    embedded["buffers"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "byteLength": 2,
            "uri": "data:application/octet-stream;base64,AA=="
        }));
    assert!(validate_gltf_semantics(&embedded, Some(&[0; 36])).is_err());
}

#[test]
fn json_padding_and_asset_version_match_the_local_loader() {
    let root = triangle_document(
        serde_json::json!([{"buffer": 0, "byteLength": 36}]),
        serde_json::json!([{
            "bufferView": 0,
            "componentType": 5126,
            "count": 3,
            "type": "VEC3"
        }]),
        36,
    );
    let mut json = serde_json::to_vec(&root).unwrap();
    json.push(b' ');
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    let mut bytes = glb_bytes(&json, &[0; 36]);
    let json_length = u32::from_le_bytes(bytes[12..16].try_into().unwrap_or_default()) as usize;
    bytes[20 + json_length - 1] = 0;
    let mut cursor = Cursor::new(bytes.as_slice());
    assert!(validate_glb_reader(&mut cursor, Path::new("model.glb"), bytes.len() as u64).is_err());
    for padding in [b'\t', b'\n'] {
        let mut bytes = glb_bytes(&json, &[0; 36]);
        let json_length = u32::from_le_bytes(bytes[12..16].try_into().unwrap_or_default()) as usize;
        bytes[20 + json_length - 1] = padding;
        let mut cursor = Cursor::new(bytes.as_slice());
        assert!(
            validate_glb_reader(&mut cursor, Path::new("model.glb"), bytes.len() as u64).is_err()
        );
    }

    let mut future = root;
    future["asset"]["version"] = Value::String("2.future".to_string());
    assert!(validate_gltf_semantics(&future, Some(&[0; 36])).is_err());
}

#[test]
fn model_safety_fixture_matches_windows_limits() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../parity-fixtures/image-to-3d/state-contract.json"
    ))
    .unwrap();
    assert_eq!(fixture["schemaVersion"].as_u64(), Some(57));
    assert_eq!(
        fixture["readiness"]["unrelatedDialogsDoNotBecomeFirstUseGuidance"].as_bool(),
        Some(true)
    );
    let safety = fixture["modelSafety"].as_object().unwrap();
    for (field, expected) in [
        ("maximumGlbBytes", MAX_GLB_BYTES),
        ("maximumJsonBytes", MAX_GLB_JSON_BYTES),
        (
            "maximumEmbeddedUriCharacters",
            MAX_EMBEDDED_URI_BYTES as u64,
        ),
        ("maximumBuffers", MAX_GLTF_BUFFERS as u64),
        ("maximumBufferViews", MAX_GLTF_BUFFER_VIEWS as u64),
        ("maximumAccessors", MAX_GLTF_ACCESSORS as u64),
        ("maximumAccessorElements", MAX_GLTF_ACCESSOR_ELEMENTS),
        (
            "maximumAggregateBufferViewBytes",
            MAX_TOTAL_BUFFER_VIEW_BYTES,
        ),
        (
            "maximumAbsoluteRendererValue",
            MAX_GLTF_ABSOLUTE_RENDERER_VALUE as u64,
        ),
        ("maximumNodes", MAX_GLTF_NODES as u64),
        ("maximumScenes", MAX_GLTF_SCENES as u64),
        ("maximumMeshes", MAX_GLTF_MESHES as u64),
        ("maximumPrimitives", MAX_GLTF_PRIMITIVES as u64),
        ("maximumMaterials", MAX_GLTF_MATERIALS as u64),
        ("maximumVertices", MAX_GLTF_VERTICES),
        ("maximumIndices", MAX_GLTF_INDICES),
        ("maximumMorphTargets", MAX_GLTF_MORPH_TARGETS as u64),
        ("maximumMorphElements", MAX_GLTF_MORPH_ELEMENTS),
        (
            "maximumPrimitiveAttributes",
            MAX_PRIMITIVE_ATTRIBUTES as u64,
        ),
        ("maximumMorphAttributes", MAX_MORPH_ATTRIBUTES as u64),
        (
            "maximumImages",
            super::super::asset_texture_validation::MAX_TEXTURE_IMAGES as u64,
        ),
        (
            "maximumTextures",
            super::super::asset_texture_validation::MAX_TEXTURES as u64,
        ),
        (
            "maximumSamplers",
            super::super::asset_texture_validation::MAX_TEXTURE_SAMPLERS as u64,
        ),
        (
            "maximumTextureAxisPixels",
            u64::from(super::super::asset_texture_validation::MAX_TEXTURE_AXIS),
        ),
        (
            "maximumPixelsPerTextureImage",
            super::super::asset_texture_validation::MAX_TEXTURE_PIXELS,
        ),
        (
            "maximumDecodedImagePixels",
            super::super::asset_texture_validation::MAX_TOTAL_TEXTURE_PIXELS,
        ),
        (
            "maximumReferencedTexturePixels",
            super::super::asset_texture_validation::MAX_TOTAL_TEXTURE_PIXELS,
        ),
    ] {
        assert_eq!(safety[field].as_u64(), Some(expected), "{field}");
    }
    let allowed = safety["allowedExtensions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(allowed, features::SAFE_EXTENSIONS);
    for field in [
        "staticTriangleGeometryOnly",
        "bufferByteLengthIsExactLogicalBytes",
        "binaryChunkUsesZeroAlignmentPadding",
        "binaryChunkMustBackBufferZero",
        "accessorAbsoluteAlignmentRequired",
        "vertexAccessorFourByteAlignmentRequired",
        "loaderInterleavedTailCoverageRequired",
        "accessorBoundsValidated",
        "positionBoundsContainBinaryValues",
        "rendererBinaryFloatValuesMustBeFinite",
        "primitiveElementCountMultipleOfThree",
        "primitiveIndicesWithinPositionAccessor",
        "texturePayloadMustDecode",
        "materialTextureReferencesValidated",
        "materialNumericValuesBounded",
        "materialRendererValueTypesValidated",
        "textureClonePixelsCharged",
        "textureTransformValuesBounded",
        "samplerEnumsValidated",
        "bufferUriMimeContextRequired",
        "presentationRevalidatesCommittedBytesBeforeLoad",
        "selectedSceneMustContainGeometry",
        "sceneRootsUniqueAcrossScenes",
        "nodeTransformsAndMorphWeightsBounded",
        "extensionsFailClosed",
        "extensionsUsedMustBeUnique",
        "extensionsRequiredMustBeUsed",
        "extensionBodiesMustBeDeclared",
    ] {
        assert_eq!(safety[field].as_bool(), Some(true), "{field}");
    }
    assert_eq!(
        safety["maximumBinaryAlignmentPaddingBytes"].as_u64(),
        Some(3)
    );
    for field in [
        "externalResourcesAllowed",
        "animatedPngAllowed",
        "animatedWebpAllowed",
        "sparseAccessorsAllowed",
        "animationsAllowed",
        "skinsAllowed",
        "authoredCamerasAllowed",
    ] {
        assert_eq!(safety[field].as_bool(), Some(false), "{field}");
    }
    assert_eq!(safety["exactAssetVersion"].as_str(), Some("2.0"));
    assert_eq!(safety["jsonChunkPaddingByte"].as_u64(), Some(32));
    assert_eq!(
        safety["maximumNodeDepth"].as_u64(),
        Some(MAX_GLTF_NODE_DEPTH as u64)
    );
}

fn triangle_document(buffer_views: Value, accessors: Value, buffer_length: u64) -> Value {
    serde_json::json!({
        "asset": {"version": "2.0"},
        "buffers": [{"byteLength": buffer_length}],
        "bufferViews": buffer_views,
        "accessors": accessors,
        "meshes": [{"primitives": [{"attributes": {"POSITION": 0}}]}],
        "nodes": [{"mesh": 0}],
        "scenes": [{"nodes": [0]}]
    })
}

#[cfg(windows)]
#[test]
fn model_validation_rejects_a_same_directory_symlink() {
    use std::os::windows::fs::symlink_file;

    let root = std::env::temp_dir().join(format!(
        "sgt-glb-same-directory-link-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let target = root.join("target.glb");
    let linked = root.join("result.glb");
    let geometry = br#"{"asset":{"version":"2.0"},"buffers":[{"byteLength":36}],"bufferViews":[{"buffer":0,"byteLength":36}],"accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}],"meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],"nodes":[{"mesh":0}],"scenes":[{"nodes":[0]}]}"#;
    write_glb(&target, geometry, &[0; 36]);
    if symlink_file(&target, &linked).is_err() {
        std::fs::remove_dir_all(root).unwrap();
        return;
    }
    assert!(validate_generated(linked.to_string_lossy().as_ref(), &root).is_err());
    std::fs::remove_file(linked).unwrap();
    std::fs::remove_dir_all(root).unwrap();
}

fn write_glb(path: &Path, json: &[u8], binary: &[u8]) {
    std::fs::write(path, glb_bytes(json, binary)).unwrap();
}

fn glb_bytes(json: &[u8], binary: &[u8]) -> Vec<u8> {
    let mut json = json.to_vec();
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    let mut binary = binary.to_vec();
    while !binary.len().is_multiple_of(4) {
        binary.push(0);
    }
    let length = 12 + 8 + json.len() + 8 + binary.len();
    let mut bytes = Vec::from(&b"glTF\x02\0\0\0"[..]);
    bytes.extend_from_slice(&(length as u32).to_le_bytes());
    bytes.extend_from_slice(&(json.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&JSON_CHUNK.to_le_bytes());
    bytes.extend_from_slice(&json);
    bytes.extend_from_slice(&(binary.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&BIN_CHUNK.to_le_bytes());
    bytes.extend_from_slice(&binary);
    bytes
}
