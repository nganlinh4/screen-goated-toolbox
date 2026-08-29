use super::*;

#[test]
fn texture_dimensions_are_bounded_before_decode() {
    let mut png = Vec::new();
    image::RgbaImage::new(1, 1)
        .write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
        .unwrap();
    png[16..20].copy_from_slice(&32_768_u32.to_be_bytes());
    png[20..24].copy_from_slice(&32_768_u32.to_be_bytes());
    let crc = crc32(&png[12..29]);
    png[29..33].copy_from_slice(&crc.to_be_bytes());

    assert!(validate_texture_payload("image/png", &png).is_err());
}

#[test]
fn texture_object_counts_are_bounded_before_loading() {
    let images = vec![Value::Null; MAX_TEXTURE_IMAGES + 1];
    let root = serde_json::json!({ "images": images });
    assert!(validate_textures(root.as_object().unwrap(), None).is_err());

    let textures: Vec<Value> = (0..=MAX_TEXTURES)
        .map(|_| serde_json::json!({ "source": 0 }))
        .collect();
    let root = serde_json::json!({ "images": [], "textures": textures });
    assert!(validate_textures(root.as_object().unwrap(), None).is_err());
}

#[test]
fn repeated_texture_sources_are_charged_per_gpu_texture() {
    let textures = vec![
        serde_json::json!({"source": 0}),
        serde_json::json!({"source": 0}),
        serde_json::json!({"source": 0}),
    ];
    let root = serde_json::json!({"textures": textures});
    assert!(validate_texture_table(root.as_object().unwrap(), &[MAX_TEXTURE_PIXELS]).is_err());
}

#[test]
fn texture_payload_must_decode_completely() {
    let mut png = Vec::new();
    image::RgbaImage::new(1, 1)
        .write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
        .unwrap();
    png.truncate(33);
    assert!(validate_texture_payload("image/png", &png).is_err());
}

#[test]
fn animated_webp_is_rejected_before_decode() {
    let mut webp = b"RIFF\x0e\0\0\0WEBPVP8X\x02\0\0\0".to_vec();
    webp.extend_from_slice(&[0x02, 0]);
    assert!(webp_is_animated(&webp));

    let webp = b"RIFF\x0c\0\0\0WEBPANIM\0\0\0\0";
    assert!(webp_is_animated(webp));
}

#[test]
fn material_texture_references_loader_clones_and_transforms_are_bounded() {
    let root = serde_json::json!({
        "materials": [
            {"pbrMetallicRoughness": {"baseColorTexture": {"index": 0, "texCoord": 1}}},
            {"normalTexture": {"index": 0, "extensions": {
                "KHR_texture_transform": {"offset": [0, 0]}
            }}}
        ]
    });
    assert!(
        validate_material_texture_references(
            root.as_object().unwrap(),
            &[MAX_TEXTURE_PIXELS],
            MAX_TEXTURE_PIXELS,
        )
        .is_err()
    );

    let invalid = serde_json::json!({"materials": [{"normalTexture": {"index": 1}}]});
    assert!(validate_material_texture_references(invalid.as_object().unwrap(), &[1], 1).is_err());

    let excessive = serde_json::json!({"materials": [{"normalTexture": {
        "index": 0,
        "extensions": {"KHR_texture_transform": {"rotation": 20_000_000}}
    }}]});
    assert!(validate_material_texture_references(excessive.as_object().unwrap(), &[1], 1).is_err());

    let equal_channel = serde_json::json!({"materials": [{"normalTexture": {
        "index": 0,
        "texCoord": 1,
        "extensions": {"KHR_texture_transform": {"texCoord": 1}}
    }}]});
    assert!(
        validate_material_texture_references(
            equal_channel.as_object().unwrap(),
            &[MAX_TEXTURE_PIXELS],
            MAX_TEXTURE_PIXELS,
        )
        .is_ok()
    );

    let two_clones = serde_json::json!({"materials": [{"normalTexture": {
        "index": 0,
        "texCoord": 1,
        "extensions": {"KHR_texture_transform": {"offset": [0, 0]}}
    }}]});
    assert!(
        validate_material_texture_references(
            two_clones.as_object().unwrap(),
            &[MAX_TEXTURE_PIXELS],
            MAX_TEXTURE_PIXELS,
        )
        .is_err()
    );

    let material_overflow = serde_json::json!({"materials": [{
        "pbrMetallicRoughness": {"metallicFactor": 20_000_000}
    }]});
    assert!(
        validate_material_texture_references(material_overflow.as_object().unwrap(), &[], 0,)
            .is_err()
    );
    let numeric_string = serde_json::json!({"materials": [{
        "pbrMetallicRoughness": {"metallicFactor": "1e300"}
    }]});
    assert!(
        validate_material_texture_references(numeric_string.as_object().unwrap(), &[], 0).is_err()
    );
    let valid_values = serde_json::json!({"materials": [{
        "name": "bounded",
        "alphaMode": "OPAQUE",
        "doubleSided": true,
        "pbrMetallicRoughness": {
            "baseColorFactor": [1, 1, 1, 1],
            "metallicFactor": 1
        },
        "emissiveFactor": [0, 0, 0],
        "normalTexture": {
            "index": 0,
            "scale": 1,
            "extensions": {"KHR_texture_transform": {
                "offset": [0, 0],
                "scale": [1, 1]
            }}
        },
        "extensions": {
            "KHR_materials_sheen": {"sheenColorFactor": [1, 1, 1]},
            "KHR_materials_specular": {"specularColorFactor": [1, 1, 1]},
            "KHR_materials_volume": {"attenuationColor": [1, 1, 1]}
        }
    }]});
    assert!(
        validate_material_texture_references(valid_values.as_object().unwrap(), &[1], 1).is_ok()
    );
    for invalid in [
        serde_json::json!({"materials": [{"name": 1}]}),
        serde_json::json!({"materials": [{"alphaMode": 1}]}),
        serde_json::json!({"materials": [{"doubleSided": 1}]}),
        serde_json::json!({"materials": [{
            "pbrMetallicRoughness": {"baseColorFactor": [1, 1, "1", 1]}
        }]}),
    ] {
        assert!(
            validate_material_texture_references(invalid.as_object().unwrap(), &[], 0).is_err()
        );
    }
}

#[test]
fn sampler_enums_and_types_are_strict() {
    let valid = serde_json::json!({
        "samplers": [{
            "name": "nearest",
            "magFilter": 9728,
            "minFilter": 9987,
            "wrapS": 33071,
            "wrapT": 10497
        }]
    });
    assert!(validate_texture_table(valid.as_object().unwrap(), &[]).is_ok());
    for sampler in [
        serde_json::json!({"magFilter": "9728"}),
        serde_json::json!({"minFilter": 9999}),
        serde_json::json!({"wrapS": 1}),
        serde_json::json!({"name": 1}),
    ] {
        let root = serde_json::json!({"samplers": [sampler]});
        assert!(validate_texture_table(root.as_object().unwrap(), &[]).is_err());
    }
}

#[test]
fn buffer_data_uri_mime_is_not_reinterpreted() {
    assert!(decode_buffer_uri("data:image/png;base64,AA==").is_err());
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320_u32 & (0_u32.wrapping_sub(crc & 1)));
        }
    }
    !crc
}
