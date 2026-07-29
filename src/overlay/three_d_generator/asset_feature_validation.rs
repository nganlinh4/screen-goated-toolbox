use std::collections::HashSet;

use serde_json::{Map, Value};

pub(super) const SAFE_EXTENSIONS: &[&str] = &[
    "EXT_materials_bump",
    "EXT_texture_webp",
    "KHR_materials_anisotropy",
    "KHR_materials_clearcoat",
    "KHR_materials_dispersion",
    "KHR_materials_emissive_strength",
    "KHR_materials_ior",
    "KHR_materials_iridescence",
    "KHR_materials_sheen",
    "KHR_materials_specular",
    "KHR_materials_transmission",
    "KHR_materials_unlit",
    "KHR_materials_volume",
    "KHR_texture_transform",
];

pub(super) fn validate(root: &Map<String, Value>) -> Result<(), String> {
    reject_unused_runtime_features(root)?;
    let used = extension_declaration(root, "extensionsUsed")?;
    let required = extension_declaration(root, "extensionsRequired")?;
    if !required.is_subset(&used) {
        return Err(invalid());
    }

    let mut seen = HashSet::new();
    let mut pending = Vec::new();
    queue_object(root, &used, &mut seen, &mut pending)?;
    while let Some(value) = pending.pop() {
        match value {
            Value::Object(object) => {
                queue_object(object, &used, &mut seen, &mut pending)?;
            }
            Value::Array(values) => pending.extend(values.iter()),
            _ => {}
        }
    }
    if seen != used {
        return Err(invalid());
    }
    Ok(())
}

fn queue_object<'a>(
    object: &'a Map<String, Value>,
    used: &HashSet<&'a str>,
    seen: &mut HashSet<&'a str>,
    pending: &mut Vec<&'a Value>,
) -> Result<(), String> {
    if let Some(extensions) = object.get("extensions") {
        let extensions = extensions.as_object().ok_or_else(invalid)?;
        for (name, body) in extensions {
            if !used.contains(name.as_str()) || !is_safe_extension(name) || !body.is_object() {
                return Err(invalid());
            }
            seen.insert(name);
            pending.push(body);
        }
    }
    for (key, child) in object {
        if key != "extensions" && key != "extras" {
            pending.push(child);
        }
    }
    Ok(())
}

fn reject_unused_runtime_features(root: &Map<String, Value>) -> Result<(), String> {
    for key in ["animations", "skins", "cameras"] {
        if root
            .get(key)
            .is_some_and(|value| value.as_array().is_none_or(|items| !items.is_empty()))
        {
            return Err(invalid());
        }
    }
    if root
        .get("nodes")
        .and_then(Value::as_array)
        .is_some_and(|nodes| {
            nodes.iter().any(|node| {
                node.as_object()
                    .is_none_or(|node| node.contains_key("skin") || node.contains_key("camera"))
            })
        })
    {
        return Err(invalid());
    }
    Ok(())
}

fn extension_declaration<'a>(
    root: &'a Map<String, Value>,
    key: &str,
) -> Result<HashSet<&'a str>, String> {
    let Some(value) = root.get(key) else {
        return Ok(HashSet::new());
    };
    let values = value.as_array().ok_or_else(invalid)?;
    if values.len() > SAFE_EXTENSIONS.len() {
        return Err(invalid());
    }
    let mut names = HashSet::with_capacity(values.len());
    for value in values {
        let name = value
            .as_str()
            .filter(|name| is_safe_extension(name))
            .ok_or_else(invalid)?;
        if !names.insert(name) {
            return Err(invalid());
        }
    }
    Ok(names)
}

fn is_safe_extension(name: &str) -> bool {
    SAFE_EXTENSIONS.contains(&name)
}

fn invalid() -> String {
    "The model result contains unsupported features.".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_declared_bounded_extensions_are_accepted() {
        let valid = serde_json::json!({
            "extensionsUsed": ["KHR_materials_unlit"],
            "materials": [{"extensions": {"KHR_materials_unlit": {}}}]
        });
        assert!(validate(valid.as_object().unwrap()).is_ok());

        let undeclared = serde_json::json!({
            "materials": [{"extensions": {"KHR_materials_unlit": {}}}]
        });
        assert!(validate(undeclared.as_object().unwrap()).is_err());

        let amplifying = serde_json::json!({
            "extensionsUsed": ["EXT_mesh_gpu_instancing"],
            "nodes": [{"extensions": {"EXT_mesh_gpu_instancing": {"attributes": {}}}}]
        });
        assert!(validate(amplifying.as_object().unwrap()).is_err());

        let malformed_body = serde_json::json!({
            "extensionsUsed": ["KHR_materials_unlit"],
            "materials": [{"extensions": {"KHR_materials_unlit": []}}]
        });
        assert!(validate(malformed_body.as_object().unwrap()).is_err());
    }

    #[test]
    fn declarations_are_unique_used_and_required_is_a_subset() {
        let duplicate = serde_json::json!({
            "extensionsUsed": ["KHR_materials_unlit", "KHR_materials_unlit"]
        });
        assert!(validate(duplicate.as_object().unwrap()).is_err());

        let unused = serde_json::json!({
            "extensionsUsed": ["KHR_materials_unlit"]
        });
        assert!(validate(unused.as_object().unwrap()).is_err());

        let missing_required = serde_json::json!({
            "extensionsRequired": ["KHR_materials_unlit"]
        });
        assert!(validate(missing_required.as_object().unwrap()).is_err());
    }

    #[test]
    fn viewer_unused_tables_and_node_references_are_rejected() {
        for root in [
            serde_json::json!({"animations": [{}]}),
            serde_json::json!({"skins": [{}]}),
            serde_json::json!({"cameras": [{}]}),
            serde_json::json!({"nodes": [{"skin": 0}]}),
            serde_json::json!({"nodes": [{"camera": 0}]}),
        ] {
            assert!(validate(root.as_object().unwrap()).is_err());
        }
        let empty = serde_json::json!({"animations": [], "skins": [], "cameras": []});
        assert!(validate(empty.as_object().unwrap()).is_ok());
    }
}
