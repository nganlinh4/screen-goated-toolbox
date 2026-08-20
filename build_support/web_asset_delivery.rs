use std::collections::HashSet;
use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

const DEFAULT_MANIFEST: &str = "component-delivery/windows/web-assets-v1.json";
const EXPECTED_COMPONENTS: &[(&str, &str)] = &[
    ("creation-3d-web", "Creation3d"),
    ("prompt-dj-web", "PromptDj"),
    ("tts-playground-web", "TtsPlayground"),
];
const EXPECTED_FILES: &[&str] = &["assets/index.css", "assets/index.js", "index.html"];

pub(crate) fn generate(manifest_dir: &Path, out_dir: &Path) {
    let selected = crate::delivery_channel::select(manifest_dir, DEFAULT_MANIFEST);
    let configured = selected.path;

    assert!(
        configured.is_file(),
        "missing verified web-asset delivery: {}",
        configured.display()
    );
    let generated = delivery_source(&configured, selected.channel);
    let output = out_dir.join("web_asset_delivery.rs");
    fs::write(&output, generated)
        .unwrap_or_else(|error| panic!("failed to write {}: {error}", output.display()));
}

fn delivery_source(path: &Path, channel: crate::delivery_channel::DeliveryChannel) -> String {
    let raw = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    let value: Value = serde_json::from_str(&raw)
        .unwrap_or_else(|error| panic!("invalid {}: {error}", path.display()));
    assert_eq!(
        value.get("schemaVersion").and_then(Value::as_u64),
        Some(1),
        "{} uses an unsupported delivery schema",
        path.display()
    );
    let version = required_string(&value, "version", path);
    validate_identifier(version, "version", path);
    let windows = value
        .get("windows")
        .and_then(Value::as_object)
        .unwrap_or_else(|| panic!("{} is missing windows delivery data", path.display()));
    assert_eq!(
        required_object_string(windows, "architecture", path),
        "x64",
        "{} must contain only Windows x64 web packs",
        path.display()
    );
    let components = windows
        .get("components")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("{} is missing windows.components", path.display()));
    assert_eq!(
        components.len(),
        EXPECTED_COMPONENTS.len(),
        "{} must deliver every supported optional web pack exactly once",
        path.display()
    );

    let mut seen_components = HashSet::new();
    let mut generated = String::from("const WEB_ASSET_DELIVERIES: &[WebAssetDelivery] = &[\n");
    for component in components {
        let component = component
            .as_object()
            .unwrap_or_else(|| panic!("{} contains an invalid component entry", path.display()));
        let id = required_object_string(component, "id", path);
        let variant = EXPECTED_COMPONENTS
            .iter()
            .find_map(|(expected, variant)| (*expected == id).then_some(*variant))
            .unwrap_or_else(|| panic!("{} contains unsupported component {id}", path.display()));
        assert!(
            seen_components.insert(id),
            "{} repeats component {id}",
            path.display()
        );
        let asset = required_object_string(component, "asset", path);
        let url = required_object_string(component, "downloadUrl", path);
        let sha256 = required_object_string(component, "sha256", path);
        validate_sha256(sha256, "component sha256", path);
        let expected_asset = format!("{id}-{version}-{}.zip", &sha256[..16]);
        assert_eq!(
            asset,
            expected_asset,
            "{} component asset must be versioned and content-addressed",
            path.display()
        );
        crate::delivery_channel::assert_candidate_asset_url(
            channel,
            asset,
            url,
            "web component URL",
        );
        let size_bytes = required_positive_u64(component, "sizeBytes", path);
        let unpacked_size_bytes = required_positive_u64(component, "unpackedSizeBytes", path);
        let files = component
            .get("files")
            .and_then(Value::as_array)
            .unwrap_or_else(|| panic!("{} component {id} has no files", path.display()));
        assert_eq!(
            files.len(),
            EXPECTED_FILES.len(),
            "{} component {id} has an unexpected file count",
            path.display()
        );

        let mut seen_files = HashSet::new();
        let mut file_total = 0_u64;
        let mut file_source = String::new();
        for file in files {
            let file = file.as_object().unwrap_or_else(|| {
                panic!("{} component {id} contains an invalid file", path.display())
            });
            let file_path = required_object_string(file, "path", path);
            validate_relative_path(file_path, path);
            assert!(
                EXPECTED_FILES.contains(&file_path),
                "{} component {id} contains unexpected file {file_path}",
                path.display()
            );
            assert!(
                seen_files.insert(file_path),
                "{} component {id} repeats file {file_path}",
                path.display()
            );
            let file_size = required_positive_u64(file, "sizeBytes", path);
            file_total = file_total
                .checked_add(file_size)
                .unwrap_or_else(|| panic!("{} component size overflow", path.display()));
            let file_sha256 = required_object_string(file, "sha256", path);
            validate_sha256(file_sha256, "file sha256", path);
            file_source.push_str(&format!(
                "            WebAssetFile {{ path: {file_path:?}, size_bytes: {file_size}, sha256: {file_sha256:?} }},\n"
            ));
        }
        assert_eq!(
            file_total,
            unpacked_size_bytes,
            "{} component {id} unpacked size does not match its files",
            path.display()
        );
        generated.push_str(&format!(
            "    WebAssetDelivery {{\n        component: WebAssetComponent::{variant},\n        version: {version:?},\n        asset: {asset:?},\n        download_url: {url:?},\n        size_bytes: {size_bytes},\n        sha256: {sha256:?},\n        unpacked_size_bytes: {unpacked_size_bytes},\n        files: &[\n{file_source}        ],\n    }},\n"
        ));
    }
    assert_eq!(
        seen_components.len(),
        EXPECTED_COMPONENTS.len(),
        "{} is missing a supported component",
        path.display()
    );
    generated.push_str("];\n");
    generated
}

fn required_string<'a>(value: &'a Value, field: &str, path: &Path) -> &'a str {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| panic!("{} is missing {field}", path.display()))
}

fn required_object_string<'a>(value: &'a Map<String, Value>, field: &str, path: &Path) -> &'a str {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| panic!("{} is missing {field}", path.display()))
}

fn required_positive_u64(value: &Map<String, Value>, field: &str, path: &Path) -> u64 {
    value
        .get(field)
        .and_then(Value::as_u64)
        .filter(|value| *value > 0)
        .unwrap_or_else(|| panic!("{} has invalid {field}", path.display()))
}

fn validate_identifier(value: &str, label: &str, path: &Path) {
    assert!(
        value.len() <= 80
            && value.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'.' | b'-' | b'_')
            }),
        "{} has invalid {label}",
        path.display()
    );
}

fn validate_sha256(value: &str, label: &str, path: &Path) {
    assert!(
        value.len() == 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "{} has invalid {label}",
        path.display()
    );
}

fn validate_relative_path(value: &str, path: &Path) {
    assert!(
        !value.is_empty()
            && value.len() <= 512
            && !value.contains('\\')
            && value
                .split('/')
                .all(|component| !component.is_empty() && component != "." && component != ".."),
        "{} contains an unsafe component file path",
        path.display()
    );
}
