use std::fs;
use std::path::Path;

use serde_json::Value;

const DEFAULT_MANIFEST: &str = "component-delivery/creation-runtime-v1.json";

pub(crate) fn generate(manifest_dir: &Path, out_dir: &Path) {
    let configured = manifest_dir.join(DEFAULT_MANIFEST);
    println!("cargo:rerun-if-changed={}", configured.display());

    assert!(
        configured.is_file(),
        "missing verified creation-runtime delivery: {}",
        configured.display()
    );
    let generated = delivery_source(&configured);

    let output = out_dir.join("creation_runtime_delivery.rs");
    fs::write(&output, generated)
        .unwrap_or_else(|error| panic!("failed to write {}: {error}", output.display()));
}

fn delivery_source(path: &Path) -> String {
    let raw = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    let value: Value = serde_json::from_str(&raw)
        .unwrap_or_else(|error| panic!("invalid {}: {error}", path.display()));
    let windows = value
        .get("windows")
        .and_then(Value::as_object)
        .unwrap_or_else(|| panic!("{} is missing windows delivery data", path.display()));

    let version = required_string(&value, "version", path);
    let schema_version = value
        .get("schemaVersion")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| panic!("{} has invalid schemaVersion", path.display()));
    assert_eq!(
        schema_version,
        1,
        "{} uses an unsupported delivery schema",
        path.display()
    );
    let features = value
        .get("features")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("{} is missing product features", path.display()))
        .iter()
        .map(|feature| {
            feature
                .as_str()
                .filter(|feature| {
                    !feature.is_empty()
                        && feature.bytes().all(|byte| {
                            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'
                        })
                })
                .unwrap_or_else(|| panic!("{} has an invalid product feature", path.display()))
        })
        .collect::<Vec<_>>();
    assert!(
        !features.is_empty(),
        "{} has no product features",
        path.display()
    );
    let unique_features = features
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(
        unique_features.len(),
        features.len(),
        "{} repeats a product feature",
        path.display()
    );
    assert_eq!(
        unique_features,
        std::collections::HashSet::from(["image_to_3d"]),
        "{} must deliver only the active Image to 3D capability",
        path.display()
    );
    let asset = required_object_string(windows, "asset", path);
    let url = required_object_string(windows, "downloadUrl", path);
    let sha256 = required_object_string(windows, "sha256", path);
    let size = windows
        .get("sizeBytes")
        .and_then(Value::as_u64)
        .filter(|value| *value > 0)
        .unwrap_or_else(|| panic!("{} has invalid windows.sizeBytes", path.display()));

    assert!(
        !asset.contains(['/', '\\']) && !asset.is_empty(),
        "{} has an unsafe windows.asset",
        path.display()
    );
    assert!(
        url.starts_with("https://"),
        "{} windows.downloadUrl must use HTTPS",
        path.display()
    );
    assert!(
        sha256.len() == 64 && sha256.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "{} has an invalid windows.sha256",
        path.display()
    );

    format!(
        "const RUNTIME_DELIVERY: Option<RuntimeDelivery> = Some(RuntimeDelivery {{\n\
         \x20   version: {version:?},\n\
         \x20   features: &{features:?},\n\
         \x20   asset: {asset:?},\n\
         \x20   download_url: {url:?},\n\
         \x20   size_bytes: {size},\n\
         \x20   sha256: {sha256:?},\n\
         }});\n"
    )
}

fn required_string<'a>(value: &'a Value, field: &str, path: &Path) -> &'a str {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| panic!("{} is missing {field}", path.display()))
}

fn required_object_string<'a>(
    value: &'a serde_json::Map<String, Value>,
    field: &str,
    path: &Path,
) -> &'a str {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| panic!("{} is missing windows.{field}", path.display()))
}
