use std::collections::HashSet;
use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

#[path = "../src/component_registry/screen_text_detector/protocol.rs"]
mod protocol;

const DEFAULT_MANIFEST: &str = "component-delivery/windows/screen-text-detector-v1.json";
const COMPONENT_ID: &str = "screen-text-detector";

pub(crate) fn generate(manifest_dir: &Path, out_dir: &Path) {
    let selected = crate::delivery_channel::select(manifest_dir, DEFAULT_MANIFEST);
    let configured = selected.path;
    assert!(
        configured.is_file(),
        "missing verified Screen Translate detector delivery: {}",
        configured.display()
    );
    let generated = delivery_source(&configured, selected.channel);
    let output = out_dir.join("screen_text_detector_delivery.rs");
    fs::write(&output, generated)
        .unwrap_or_else(|error| panic!("failed to write {}: {error}", output.display()));
}

fn delivery_source(path: &Path, channel: crate::delivery_channel::DeliveryChannel) -> String {
    let raw = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    let value: Value = serde_json::from_str(&raw)
        .unwrap_or_else(|error| panic!("invalid {}: {error}", path.display()));
    let root = value
        .as_object()
        .unwrap_or_else(|| panic!("{} must contain an object", path.display()));
    assert_eq!(required_u64(root, "schemaVersion", path), 1);
    assert_eq!(required_string(root, "architecture", path), "x64");
    let component = root
        .get("component")
        .and_then(Value::as_object)
        .unwrap_or_else(|| panic!("{} is missing component", path.display()));
    assert_eq!(required_string(component, "id", path), COMPONENT_ID);
    let version = required_string(component, "version", path);
    validate_identifier(version, path);
    let declared = component
        .get("protocol")
        .map(|value| value.as_str().expect("detector protocol must be a string"));
    protocol::validate(version, declared).expect("incompatible screen-text detector delivery");
    let required_files = incremental_files();
    let sha256 = required_string(component, "sha256", path);
    validate_sha256(sha256, path);
    let asset = required_string(component, "asset", path);
    assert_eq!(
        asset,
        format!("{COMPONENT_ID}-{version}-{}.zip", &sha256[..16]),
        "{} asset must be versioned and content-addressed",
        path.display()
    );
    let download_url = required_string(component, "downloadUrl", path);
    crate::delivery_channel::assert_owned_asset_url(
        channel,
        asset,
        download_url,
        "screen-text detector URL",
    );
    let size_bytes = required_u64(component, "sizeBytes", path);
    let unpacked_size_bytes = required_u64(component, "unpackedSizeBytes", path);
    let files = component
        .get("files")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("{} component has no files", path.display()));
    assert_eq!(
        files.len(),
        required_files.len(),
        "{} inventory must match package-inputs.json",
        path.display()
    );
    let mut seen = HashSet::new();
    let mut file_total = 0_u64;
    let mut file_source = String::new();
    for file in files {
        let file = file
            .as_object()
            .unwrap_or_else(|| panic!("{} contains an invalid file", path.display()));
        let relative = required_string(file, "path", path);
        assert!(
            required_files.iter().any(|required| required == relative) && seen.insert(relative),
            "{} lists an unexpected or duplicate file {relative}",
            path.display()
        );
        let file_size = required_u64(file, "sizeBytes", path);
        file_total = file_total
            .checked_add(file_size)
            .expect("file size overflow");
        let file_sha = required_string(file, "sha256", path);
        validate_sha256(file_sha, path);
        file_source.push_str(&format!(
            "        DetectorFile {{ path: {relative:?}, size_bytes: {file_size}, sha256: {file_sha:?} }},\n"
        ));
    }
    assert!(
        required_files
            .iter()
            .all(|relative| seen.contains(relative.as_str()))
    );
    assert_eq!(file_total, unpacked_size_bytes);
    format!(
        "const DETECTOR_DELIVERY: Option<DetectorDelivery> = Some(DetectorDelivery {{ version: {version:?}, asset: {asset:?}, download_url: {download_url:?}, size_bytes: {size_bytes}, sha256: {sha256:?}, unpacked_size_bytes: {unpacked_size_bytes}, files: &[\n{file_source}    ] }});\n"
    )
}

fn incremental_files() -> Vec<String> {
    let inputs: Value = serde_json::from_str(include_str!(
        "../native/screen_translate_worker/package-inputs.json"
    ))
    .expect("invalid incremental package inputs");
    let mut files = [
        "bin/x64/sgt-screen-text-detector-worker.exe",
        "licenses/THIRD-PARTY-LICENSES.json",
        "licenses/THIRD-PARTY-NOTICES.txt",
        "licenses/PACKAGE-INPUTS.json",
        "models/pp-ocr-screen-text/readers.json",
    ]
    .map(str::to_owned)
    .to_vec();
    for section in ["models", "licenses"] {
        for item in inputs[section].as_array().expect("package inventory") {
            files.push(item["path"].as_str().expect("package file").to_owned());
        }
    }
    files
}

fn required_string<'a>(value: &'a Map<String, Value>, field: &str, path: &Path) -> &'a str {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| panic!("{} is missing {field}", path.display()))
}

fn required_u64(value: &Map<String, Value>, field: &str, path: &Path) -> u64 {
    value
        .get(field)
        .and_then(Value::as_u64)
        .filter(|value| *value > 0)
        .unwrap_or_else(|| panic!("{} has invalid {field}", path.display()))
}

fn validate_identifier(value: &str, path: &Path) {
    assert!(
        value.len() <= 80
            && value.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'.' | b'-' | b'_')
            }),
        "{} contains an invalid identifier",
        path.display()
    );
}

fn validate_sha256(value: &str, path: &Path) {
    assert!(
        value.len() == 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "{} contains an invalid SHA-256",
        path.display()
    );
}
