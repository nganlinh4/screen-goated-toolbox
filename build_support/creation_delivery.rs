use std::collections::HashSet;
use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

const DEFAULT_MANIFEST: &str = "component-delivery/windows/creation-v1.json";
const EXPECTED_FEATURES: &[&str] = &["image_to_3d", "image_to_svg", "image_creator"];
const EXPECTED_FILES: &[&str] = &[
    "bin/sgt_creation_runtime.exe",
    "web/assets/index.css",
    "web/assets/index.js",
    "web/index.html",
];

pub(crate) fn generate(manifest_dir: &Path, out_dir: &Path) {
    let selected = crate::delivery_channel::select(manifest_dir, DEFAULT_MANIFEST);
    assert!(
        selected.path.is_file(),
        "missing verified Windows Creation delivery: {}",
        selected.path.display()
    );
    let generated = delivery_source(&selected.path, selected.channel);
    let output = out_dir.join("creation_delivery.rs");
    fs::write(&output, generated)
        .unwrap_or_else(|error| panic!("failed to write {}: {error}", output.display()));
}

fn delivery_source(path: &Path, channel: crate::delivery_channel::DeliveryChannel) -> String {
    let raw = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    let value: Value = serde_json::from_str(&raw)
        .unwrap_or_else(|error| panic!("invalid {}: {error}", path.display()));
    assert_eq!(value.get("schemaVersion").and_then(Value::as_u64), Some(1));
    let host_version = required_string(&value, "hostVersion", path);
    let package_version = std::env::var("CARGO_PKG_VERSION").unwrap();
    assert_eq!(host_version, package_version, "Creation host pin is stale");
    let version = required_string(&value, "version", path);
    let runtime_version = required_string(&value, "runtimeVersion", path);
    validate_identifier(version, path);
    validate_identifier(runtime_version, path);
    let features = value.get("features").and_then(Value::as_array).unwrap();
    let feature_set = features
        .iter()
        .map(|item| item.as_str().unwrap())
        .collect::<HashSet<_>>();
    assert_eq!(feature_set, EXPECTED_FEATURES.iter().copied().collect());

    let windows = value.get("windows").and_then(Value::as_object).unwrap();
    assert_eq!(required(windows, "architecture", path), "x64");
    let asset = required(windows, "asset", path);
    let url = required(windows, "downloadUrl", path);
    let sha = required(windows, "sha256", path);
    validate_sha(sha, path);
    assert_eq!(
        asset,
        format!("creation-windows-{version}-{}.zip", &sha[..16])
    );
    crate::delivery_channel::assert_candidate_asset_url(channel, asset, url, "Creation URL");
    let size = positive(windows, "sizeBytes", path);
    let unpacked = positive(windows, "unpackedSizeBytes", path);
    let files = windows.get("files").and_then(Value::as_array).unwrap();
    assert_eq!(files.len(), EXPECTED_FILES.len());
    let mut seen = HashSet::new();
    let mut total = 0_u64;
    let mut file_source = String::new();
    for file in files {
        let file = file.as_object().unwrap();
        let relative = required(file, "path", path);
        assert!(EXPECTED_FILES.contains(&relative) && seen.insert(relative));
        let file_size = positive(file, "sizeBytes", path);
        total = total.checked_add(file_size).unwrap();
        let file_sha = required(file, "sha256", path);
        validate_sha(file_sha, path);
        file_source.push_str(&format!(
            "    CreationFile {{ path: {relative:?}, size_bytes: {file_size}, sha256: {file_sha:?} }},\n"
        ));
    }
    assert_eq!(total, unpacked);
    format!(
        "const CREATION_DELIVERY: Option<CreationDelivery> = Some(CreationDelivery {{\n\
         version: {version:?}, runtime_version: {runtime_version:?}, features: &{EXPECTED_FEATURES:?},\n\
         asset: {asset:?}, download_url: {url:?}, size_bytes: {size}, sha256: {sha:?},\n\
         unpacked_size_bytes: {unpacked}, files: &[\n{file_source}],\n}});\n"
    )
}

fn required_string<'a>(value: &'a Value, key: &str, path: &Path) -> &'a str {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| panic!("{} is missing {key}", path.display()))
}

fn required<'a>(value: &'a Map<String, Value>, key: &str, path: &Path) -> &'a str {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| panic!("{} is missing {key}", path.display()))
}

fn positive(value: &Map<String, Value>, key: &str, path: &Path) -> u64 {
    value
        .get(key)
        .and_then(Value::as_u64)
        .filter(|value| *value > 0)
        .unwrap_or_else(|| panic!("{} has invalid {key}", path.display()))
}

fn validate_identifier(value: &str, path: &Path) {
    assert!(
        value.len() <= 80
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')),
        "{} has invalid identifier",
        path.display()
    );
}

fn validate_sha(value: &str, path: &Path) {
    assert!(
        value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "{} has invalid SHA-256",
        path.display()
    );
}
