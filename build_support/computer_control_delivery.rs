use std::collections::HashSet;
use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

const DEFAULT_MANIFEST: &str = "component-delivery/windows/computer-control-v1.json";
const RELEASE_PREFIX: &str =
    "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/sgt-runtime-bundles/";
const COMPONENT_ID: &str = "computer-control-engine";
const REQUIRED_FILES: &[&str] = &[
    "bin/x64/sgt-computer-control-engine.exe",
    "licenses/THIRD-PARTY-LICENSES.json",
    "licenses/THIRD-PARTY-NOTICES.txt",
];

pub(crate) fn generate(manifest_dir: &Path, out_dir: &Path) {
    let configured = manifest_dir.join(DEFAULT_MANIFEST);
    println!("cargo:rerun-if-changed={}", configured.display());
    assert!(
        configured.is_file(),
        "missing verified Computer Control delivery: {}",
        configured.display()
    );
    let generated = delivery_source(&configured);
    let output = out_dir.join("computer_control_delivery.rs");
    fs::write(&output, generated)
        .unwrap_or_else(|error| panic!("failed to write {}: {error}", output.display()));
}

fn delivery_source(path: &Path) -> String {
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
    assert_eq!(download_url, format!("{RELEASE_PREFIX}{asset}"));
    let size_bytes = required_u64(component, "sizeBytes", path);
    let unpacked_size_bytes = required_u64(component, "unpackedSizeBytes", path);
    let files = component
        .get("files")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("{} component has no files", path.display()));
    assert_eq!(files.len(), REQUIRED_FILES.len());
    let mut seen = HashSet::new();
    let mut file_total = 0_u64;
    let mut file_source = String::new();
    for file in files {
        let file = file
            .as_object()
            .unwrap_or_else(|| panic!("{} contains an invalid file", path.display()));
        let relative = required_string(file, "path", path);
        assert!(REQUIRED_FILES.contains(&relative) && seen.insert(relative));
        let file_size = required_u64(file, "sizeBytes", path);
        file_total = file_total
            .checked_add(file_size)
            .expect("file size overflow");
        let file_sha = required_string(file, "sha256", path);
        validate_sha256(file_sha, path);
        file_source.push_str(&format!(
            "        EngineFile {{ path: {relative:?}, size_bytes: {file_size}, sha256: {file_sha:?} }},\n"
        ));
    }
    assert_eq!(seen.len(), REQUIRED_FILES.len());
    assert_eq!(file_total, unpacked_size_bytes);
    format!(
        "const ENGINE_DELIVERY: Option<EngineDelivery> = Some(EngineDelivery {{ version: {version:?}, asset: {asset:?}, download_url: {download_url:?}, size_bytes: {size_bytes}, sha256: {sha256:?}, unpacked_size_bytes: {unpacked_size_bytes}, files: &[\n{file_source}    ] }});\n"
    )
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
