use serde_json::{Map, Value};

use super::super::asset_protocol::MAX_GLTF_ABSOLUTE_RENDERER_VALUE;
use super::MAX_TOTAL_TEXTURE_PIXELS;

pub(super) fn validate_material_texture_references(
    root: &Map<String, Value>,
    texture_pixels: &[u64],
    mut referenced_pixels: u64,
) -> Result<(), String> {
    let Some(materials) = root.get("materials") else {
        return Ok(());
    };
    for material in materials.as_array().ok_or_else(invalid)? {
        validate_material_values(material)?;
        let material = material.as_object().ok_or_else(invalid)?;
        for key in ["normalTexture", "occlusionTexture", "emissiveTexture"] {
            charge_texture_info(material.get(key), 1, texture_pixels, &mut referenced_pixels)?;
        }
        if let Some(pbr) = material.get("pbrMetallicRoughness") {
            let pbr = pbr.as_object().ok_or_else(invalid)?;
            charge_texture_info(
                pbr.get("baseColorTexture"),
                1,
                texture_pixels,
                &mut referenced_pixels,
            )?;
            charge_texture_info(
                pbr.get("metallicRoughnessTexture"),
                2,
                texture_pixels,
                &mut referenced_pixels,
            )?;
        }
        let Some(extensions) = material.get("extensions") else {
            continue;
        };
        let extensions = extensions.as_object().ok_or_else(invalid)?;
        for (name, body) in extensions {
            let body = body.as_object().ok_or_else(invalid)?;
            let slots: &[(&str, u64)] = match name.as_str() {
                "KHR_materials_clearcoat" => &[
                    ("clearcoatTexture", 1),
                    ("clearcoatRoughnessTexture", 1),
                    ("clearcoatNormalTexture", 1),
                ],
                "KHR_materials_iridescence" => &[
                    ("iridescenceTexture", 1),
                    ("iridescenceThicknessTexture", 1),
                ],
                "KHR_materials_sheen" => &[("sheenColorTexture", 1), ("sheenRoughnessTexture", 1)],
                "KHR_materials_specular" => &[("specularTexture", 1), ("specularColorTexture", 1)],
                "KHR_materials_transmission" => &[("transmissionTexture", 1)],
                "KHR_materials_volume" => &[("thicknessTexture", 1)],
                "KHR_materials_anisotropy" => &[("anisotropyTexture", 1)],
                "EXT_materials_bump" => &[("bumpTexture", 1)],
                "KHR_materials_dispersion"
                | "KHR_materials_emissive_strength"
                | "KHR_materials_ior"
                | "KHR_materials_unlit" => &[],
                _ => return Err(invalid()),
            };
            for (key, assignments) in slots {
                charge_texture_info(
                    body.get(*key),
                    *assignments,
                    texture_pixels,
                    &mut referenced_pixels,
                )?;
            }
        }
    }
    Ok(())
}

fn validate_material_values(value: &Value) -> Result<(), String> {
    validate_material_value(value, None)
}

fn validate_material_value(value: &Value, field: Option<&str>) -> Result<(), String> {
    if field.is_some_and(material_number_field) && !value.is_number()
        || field.is_some_and(|field| material_array_length(field).is_some()) && !value.is_array()
        || field == Some("scale") && !value.is_number() && !value.is_array()
        || matches!(field, Some("name" | "alphaMode")) && !value.is_string()
        || field == Some("doubleSided") && !value.is_boolean()
    {
        return Err(invalid());
    }
    match value {
        Value::Object(values) => {
            for (name, value) in values {
                if name != "extras" {
                    validate_material_value(value, Some(name))?;
                }
            }
        }
        Value::Array(values) => {
            let expected = field
                .and_then(material_array_length)
                .or_else(|| (field == Some("scale")).then_some(2));
            if expected.is_none_or(|expected| values.len() != expected)
                || values.iter().any(|value| !value.is_number())
            {
                return Err(invalid());
            }
            for value in values {
                validate_material_value(value, None)?;
            }
        }
        Value::Number(value) => {
            if value.as_f64().is_none_or(|value| {
                !value.is_finite() || value.abs() > MAX_GLTF_ABSOLUTE_RENDERER_VALUE
            }) {
                return Err(invalid());
            }
        }
        Value::String(value) => match field {
            Some("name") => {}
            Some("alphaMode") if matches!(value.as_str(), "OPAQUE" | "MASK" | "BLEND") => {}
            _ => return Err(invalid()),
        },
        Value::Bool(_) if field == Some("doubleSided") => {}
        Value::Bool(_) | Value::Null => return Err(invalid()),
    }
    Ok(())
}

fn material_number_field(field: &str) -> bool {
    matches!(
        field,
        "metallicFactor"
            | "roughnessFactor"
            | "strength"
            | "alphaCutoff"
            | "emissiveStrength"
            | "clearcoatFactor"
            | "clearcoatRoughnessFactor"
            | "dispersion"
            | "iridescenceFactor"
            | "iridescenceIor"
            | "iridescenceThicknessMinimum"
            | "iridescenceThicknessMaximum"
            | "sheenRoughnessFactor"
            | "transmissionFactor"
            | "thicknessFactor"
            | "attenuationDistance"
            | "ior"
            | "specularFactor"
            | "bumpFactor"
            | "anisotropyStrength"
            | "anisotropyRotation"
            | "index"
            | "texCoord"
            | "rotation"
    )
}

fn material_array_length(field: &str) -> Option<usize> {
    match field {
        "baseColorFactor" => Some(4),
        "emissiveFactor" | "sheenColorFactor" | "specularColorFactor" | "attenuationColor" => {
            Some(3)
        }
        "offset" => Some(2),
        _ => None,
    }
}

fn charge_texture_info(
    value: Option<&Value>,
    assignments: u64,
    texture_pixels: &[u64],
    referenced_pixels: &mut u64,
) -> Result<(), String> {
    let Some(value) = value else {
        return Ok(());
    };
    let info = value.as_object().ok_or_else(invalid)?;
    let index = info
        .get("index")
        .and_then(Value::as_u64)
        .and_then(|index| usize::try_from(index).ok())
        .filter(|index| *index < texture_pixels.len())
        .ok_or_else(invalid)?;
    let tex_coord = match info.get("texCoord") {
        Some(value) => value
            .as_u64()
            .filter(|value| *value <= 3)
            .ok_or_else(invalid)?,
        None => 0,
    };
    let mut clone_count = u64::from(tex_coord > 0);
    if let Some(extensions) = info.get("extensions") {
        let extensions = extensions.as_object().ok_or_else(invalid)?;
        for (name, body) in extensions {
            if name != "KHR_texture_transform" {
                return Err(invalid());
            }
            let transform = body.as_object().ok_or_else(invalid)?;
            if transform
                .keys()
                .any(|key| !matches!(key.as_str(), "offset" | "rotation" | "scale" | "texCoord"))
            {
                return Err(invalid());
            }
            let mut transform_clones = false;
            if let Some(value) = transform.get("texCoord") {
                let transformed = value
                    .as_u64()
                    .filter(|value| *value <= 3)
                    .ok_or_else(invalid)?;
                transform_clones |= transformed != tex_coord;
            }
            for key in ["offset", "scale"] {
                if let Some(value) = transform.get(key) {
                    validate_finite_array(value, 2)?;
                    transform_clones = true;
                }
            }
            if let Some(value) = transform.get("rotation") {
                if value.as_f64().is_none_or(|value| {
                    !value.is_finite() || value.abs() > MAX_GLTF_ABSOLUTE_RENDERER_VALUE
                }) {
                    return Err(invalid());
                }
                transform_clones = true;
            }
            clone_count += u64::from(transform_clones);
        }
    }
    if clone_count > 0 {
        *referenced_pixels = referenced_pixels
            .checked_add(
                texture_pixels[index]
                    .checked_mul(assignments)
                    .and_then(|pixels| pixels.checked_mul(clone_count))
                    .ok_or_else(invalid)?,
            )
            .filter(|pixels| *pixels <= MAX_TOTAL_TEXTURE_PIXELS)
            .ok_or_else(invalid)?;
    }
    Ok(())
}

fn validate_finite_array(value: &Value, length: usize) -> Result<(), String> {
    value
        .as_array()
        .filter(|values| {
            values.len() == length
                && values.iter().all(|value| {
                    value.as_f64().is_some_and(|value| {
                        value.is_finite() && value.abs() <= MAX_GLTF_ABSOLUTE_RENDERER_VALUE
                    })
                })
        })
        .map(|_| ())
        .ok_or_else(invalid)
}

fn invalid() -> String {
    "The model contains an invalid texture.".to_string()
}
