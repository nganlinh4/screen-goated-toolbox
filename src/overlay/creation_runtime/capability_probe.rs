use std::path::PathBuf;
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde_json::Value;

#[derive(Clone, Eq, PartialEq)]
struct RuntimeBinaryKey {
    path: PathBuf,
    size_bytes: u64,
    modified_nanos: u128,
}

#[derive(Clone)]
struct RuntimeCapabilities {
    binary: RuntimeBinaryKey,
    fast_optional_instruction: bool,
    quality_optional_instruction: bool,
}

pub(crate) fn supports_optional_3d_instruction(mode: &str) -> bool {
    if !matches!(mode, "fast" | "quality") {
        return false;
    }
    let Some(binary) = runtime_binary_key() else {
        return false;
    };
    static CACHE: OnceLock<Mutex<Option<RuntimeCapabilities>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(None));
    if let Some(capabilities) = cache
        .lock()
        .unwrap_or_else(|value| value.into_inner())
        .as_ref()
        .filter(|capabilities| capabilities.binary == binary)
        .cloned()
    {
        return optional_instruction_for(&capabilities, mode);
    }

    let capabilities = query_runtime_capabilities(binary.clone()).unwrap_or(RuntimeCapabilities {
        binary,
        fast_optional_instruction: false,
        quality_optional_instruction: false,
    });
    let supported = optional_instruction_for(&capabilities, mode);
    *cache.lock().unwrap_or_else(|value| value.into_inner()) = Some(capabilities);
    supported
}

fn optional_instruction_for(capabilities: &RuntimeCapabilities, mode: &str) -> bool {
    match mode {
        "fast" => capabilities.fast_optional_instruction,
        "quality" => capabilities.quality_optional_instruction,
        _ => false,
    }
}

fn runtime_binary_key() -> Option<RuntimeBinaryKey> {
    let path = super::shared_runtime_path()?;
    let metadata = path.metadata().ok()?;
    if !metadata.is_file() {
        return None;
    }
    let modified_nanos = metadata
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    Some(RuntimeBinaryKey {
        path,
        size_bytes: metadata.len(),
        modified_nanos,
    })
}

fn query_runtime_capabilities(binary: RuntimeBinaryKey) -> Option<RuntimeCapabilities> {
    let delivery = super::runtime_delivery()?;
    let mut command = Command::new(&binary.path);
    command.arg("--capabilities");
    let output = super::process_query::run(&mut command, None, Duration::from_secs(5), 64 * 1024)?;
    if !output.status.success() || output.truncated {
        return None;
    }
    parse_runtime_capabilities(binary, &output.bytes, delivery.version, delivery.features)
}

fn parse_runtime_capabilities(
    binary: RuntimeBinaryKey,
    bytes: &[u8],
    expected_version: &str,
    expected_features: &[&str],
) -> Option<RuntimeCapabilities> {
    let root = std::str::from_utf8(bytes)
        .ok()?
        .lines()
        .rev()
        .find_map(|line| serde_json::from_str::<Value>(line).ok())?;
    let root_object = root.as_object()?;
    if !has_exact_keys(root_object, &["ok", "result"]) {
        return None;
    }
    if root.get("ok").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let result = root.get("result")?.as_object()?;
    if !has_exact_keys(
        result,
        &["contractVersion", "runtimeVersion", "features", "tools"],
    ) {
        return None;
    }
    if result.get("contractVersion").and_then(Value::as_u64) != Some(1)
        || result.get("runtimeVersion").and_then(Value::as_str) != Some(expected_version)
    {
        return None;
    }
    let features = result.get("features")?.as_array()?;
    let feature_names = features
        .iter()
        .map(Value::as_str)
        .collect::<Option<Vec<_>>>()?;
    let feature_set = feature_names
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    let expected_set = expected_features
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    if feature_set.len() != feature_names.len() || feature_set != expected_set {
        return None;
    }
    let tools = result.get("tools")?.as_object()?;
    if !has_exact_keys(tools, &["image_to_3d"]) {
        return None;
    }
    let tool = tools.get("image_to_3d")?.as_object()?;
    if !has_exact_keys(tool, &["generationModes"]) {
        return None;
    }
    let modes = tool.get("generationModes")?.as_object()?;
    if !has_exact_keys(modes, &["fast", "quality"]) {
        return None;
    }
    let optional_instruction = |mode: &str| {
        let descriptor = modes.get(mode)?.as_object()?;
        has_exact_keys(descriptor, &["optionalInstruction"])
            .then(|| descriptor.get("optionalInstruction")?.as_bool())
            .flatten()
    };
    Some(RuntimeCapabilities {
        binary,
        fast_optional_instruction: optional_instruction("fast")?,
        quality_optional_instruction: optional_instruction("quality")?,
    })
}

fn has_exact_keys(object: &serde_json::Map<String, Value>, expected: &[&str]) -> bool {
    object.len() == expected.len() && expected.iter().all(|key| object.contains_key(*key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_capabilities_require_matching_version_and_feature_set() {
        let binary = RuntimeBinaryKey {
            path: PathBuf::from("runtime.exe"),
            size_bytes: 1,
            modified_nanos: 1,
        };
        let valid = br#"{"ok":true,"result":{"contractVersion":1,"runtimeVersion":"1.2.3","features":["image_to_3d"],"tools":{"image_to_3d":{"generationModes":{"fast":{"optionalInstruction":true},"quality":{"optionalInstruction":false}}}}}}"#;
        let parsed =
            parse_runtime_capabilities(binary.clone(), valid, "1.2.3", &["image_to_3d"]).unwrap();
        assert!(parsed.fast_optional_instruction);
        assert!(!parsed.quality_optional_instruction);

        assert!(
            parse_runtime_capabilities(binary.clone(), valid, "1.2.4", &["image_to_3d"],).is_none()
        );
        assert!(
            parse_runtime_capabilities(binary, valid, "1.2.3", &["image_to_3d", "image_to_svg"],)
                .is_none()
        );
    }

    #[test]
    fn unknown_or_malformed_capability_schema_fails_closed() {
        let binary = RuntimeBinaryKey {
            path: PathBuf::from("runtime.exe"),
            size_bytes: 1,
            modified_nanos: 1,
        };
        let features = &["image_to_3d"];
        for invalid in [
            br#"{"ok":true,"extra":1,"result":{"contractVersion":1,"runtimeVersion":"1.2.3","features":["image_to_3d"],"tools":{"image_to_3d":{"generationModes":{"fast":{"optionalInstruction":true},"quality":{"optionalInstruction":false}}}}}}"#.as_slice(),
            br#"{"ok":true,"result":{"contractVersion":1,"runtimeVersion":"1.2.3","features":["image_to_3d"],"tools":{"image_to_3d":{"generationModes":{"fast":{"optionalInstruction":true},"quality":{"optionalInstruction":false},"future":{"optionalInstruction":true}}}}}}"#.as_slice(),
            br#"{"ok":true,"result":{"contractVersion":1,"runtimeVersion":"1.2.3","features":["image_to_3d"],"tools":{"image_to_3d":{"generationModes":{"fast":{"optionalInstruction":"yes"},"quality":{"optionalInstruction":false}}}}}}"#.as_slice(),
            br#"{"ok":true,"result":{"contractVersion":1,"runtimeVersion":"1.2.3","features":["image_to_3d"],"tools":{"image_to_3d":{"generationModes":{"fast":{"optionalInstruction":true}}}}}}"#.as_slice(),
            br#"{"ok":true,"result":{"contractVersion":1,"runtimeVersion":"1.2.3","features":["image_to_3d","image_to_svg"],"tools":{"image_to_3d":{"generationModes":{"fast":{"optionalInstruction":true},"quality":{"optionalInstruction":false}}}}}}"#.as_slice(),
        ] {
            assert!(
                parse_runtime_capabilities(
                    binary.clone(),
                    invalid,
                    "1.2.3",
                    features,
                )
                .is_none()
            );
        }
    }
}
