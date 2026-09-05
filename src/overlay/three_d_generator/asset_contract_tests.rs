use super::*;
use crate::overlay::three_d_generator::asset_texture_validation;

#[test]
fn model_safety_fixture_matches_windows_limits() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../parity-fixtures/image-to-3d/state-contract.json"
    ))
    .unwrap();
    assert_eq!(fixture["schemaVersion"].as_u64(), Some(73));
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
        ("maximumSkins", MAX_GLTF_SKINS as u64),
        ("maximumJointsPerSkin", MAX_GLTF_JOINTS_PER_SKIN as u64),
        ("maximumTotalJoints", MAX_GLTF_TOTAL_JOINTS as u64),
        (
            "maximumPrimitiveAttributes",
            MAX_PRIMITIVE_ATTRIBUTES as u64,
        ),
        ("maximumMorphAttributes", MAX_MORPH_ATTRIBUTES as u64),
        (
            "maximumImages",
            asset_texture_validation::MAX_TEXTURE_IMAGES as u64,
        ),
        (
            "maximumTextures",
            asset_texture_validation::MAX_TEXTURES as u64,
        ),
        (
            "maximumSamplers",
            asset_texture_validation::MAX_TEXTURE_SAMPLERS as u64,
        ),
        (
            "maximumTextureAxisPixels",
            u64::from(asset_texture_validation::MAX_TEXTURE_AXIS),
        ),
        (
            "maximumPixelsPerTextureImage",
            asset_texture_validation::MAX_TEXTURE_PIXELS,
        ),
        (
            "maximumDecodedImagePixels",
            asset_texture_validation::MAX_TOTAL_TEXTURE_PIXELS,
        ),
        (
            "maximumReferencedTexturePixels",
            asset_texture_validation::MAX_TOTAL_TEXTURE_PIXELS,
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
        "sharedIndexedVertexStorageChargedOnce",
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
        "skinsAllowed",
        "skinReferencesValidated",
        "inverseBindMatricesValidated",
        "jointIndicesWithinSkin",
        "skinWeightsNormalized",
        "skinScopesUnambiguous",
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
        "authoredCamerasAllowed",
    ] {
        assert_eq!(safety[field].as_bool(), Some(false), "{field}");
    }
    assert_eq!(safety["exactAssetVersion"].as_str(), Some("2.0"));
    assert_eq!(safety["animationsAllowed"].as_bool(), Some(true));
    assert_eq!(safety["staticTriangleGeometryOnly"].as_bool(), Some(false));
    for (field, expected) in [
        ("maximumAnimationClips", animations::MAX_CLIPS as u64),
        ("maximumAnimationChannels", animations::MAX_CHANNELS as u64),
        ("maximumAnimationKeyframes", animations::MAX_KEYS),
        (
            "maximumAnimationOutputComponents",
            animations::MAX_COMPONENTS,
        ),
        (
            "maximumAnimationDurationSeconds",
            animations::MAX_DURATION as u64,
        ),
    ] {
        assert_eq!(safety[field].as_u64(), Some(expected), "{field}");
    }
    assert_eq!(safety["jsonChunkPaddingByte"].as_u64(), Some(32));
    assert_eq!(
        safety["maximumNodeDepth"].as_u64(),
        Some(MAX_GLTF_NODE_DEPTH as u64)
    );
}
