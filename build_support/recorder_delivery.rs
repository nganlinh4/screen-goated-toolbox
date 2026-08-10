use std::collections::HashSet;
use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

const DEFAULT_MANIFEST: &str = "component-delivery/windows/recorder-v1.json";
const RELEASE_PREFIX: &str =
    "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/sgt-runtime-bundles/";
const WORKER_FILES: &[&str] = &[
    "bin/x64/sgt-recorder-worker.exe",
    "licenses/THIRD-PARTY-LICENSES.json",
    "licenses/THIRD-PARTY-NOTICES.txt",
];
const MAX_WEB_FILES: usize = 512;

pub(crate) fn generate(manifest_dir: &Path, out_dir: &Path) {
    let configured = manifest_dir.join(DEFAULT_MANIFEST);
    println!("cargo:rerun-if-changed={}", configured.display());
    assert!(
        configured.is_file(),
        "missing verified recorder delivery: {}",
        configured.display()
    );
    let generated = delivery_source(&configured);
    let output = out_dir.join("recorder_delivery.rs");
    fs::write(&output, generated)
        .unwrap_or_else(|error| panic!("failed to write {}: {error}", output.display()));
}

fn delivery_source(path: &Path) -> String {
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
    assert_eq!(
        value.get("architecture").and_then(Value::as_str),
        Some("x64"),
        "{} must contain x64 packages",
        path.display()
    );
    let components = value
        .get("components")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("{} is missing components", path.display()));
    assert_eq!(
        components.len(),
        2,
        "{} must deliver both recorder components",
        path.display()
    );

    let mut seen = HashSet::new();
    let mut source = String::from("const RECORDER_DELIVERIES: &[RecorderDelivery] = &[\n");
    for component in components {
        let component = component
            .as_object()
            .unwrap_or_else(|| panic!("{} contains an invalid component", path.display()));
        let id = required_string(component, "id", path);
        assert!(
            matches!(id, "recorder-web" | "recorder-worker") && seen.insert(id),
            "{} contains an unsupported or repeated component {id}",
            path.display()
        );
        let version = required_string(component, "version", path);
        validate_identifier(version, path);
        let digest = required_string(component, "sha256", path);
        validate_sha256(digest, path);
        let asset = required_string(component, "asset", path);
        assert_eq!(
            asset,
            format!("{id}-{version}-{}.zip", &digest[..16]),
            "{} has a non-content-addressed recorder asset",
            path.display()
        );
        let download_url = required_string(component, "downloadUrl", path);
        assert_eq!(
            download_url,
            format!("{RELEASE_PREFIX}{asset}"),
            "{} recorder URL must use the immutable runtime-bundles release",
            path.display()
        );
        let size_bytes = required_u64(component, "sizeBytes", path);
        let unpacked_size_bytes = required_u64(component, "unpackedSizeBytes", path);
        let files = component
            .get("files")
            .and_then(Value::as_array)
            .unwrap_or_else(|| panic!("{} component {id} has no files", path.display()));
        assert!(
            !files.is_empty() && files.len() <= MAX_WEB_FILES,
            "{} component {id} has an invalid file count",
            path.display()
        );
        if id == "recorder-worker" {
            assert_eq!(
                files.len(),
                WORKER_FILES.len(),
                "{} recorder worker has an unexpected inventory",
                path.display()
            );
        }

        let mut seen_files = HashSet::new();
        let mut file_total = 0_u64;
        let mut file_source = String::new();
        for file in files {
            let file = file
                .as_object()
                .unwrap_or_else(|| panic!("{} contains an invalid file", path.display()));
            let relative = required_string(file, "path", path);
            validate_relative_path(relative, path);
            assert!(
                seen_files.insert(relative),
                "{} component {id} repeats file {relative}",
                path.display()
            );
            if id == "recorder-worker" {
                assert!(
                    WORKER_FILES.contains(&relative),
                    "{} recorder worker contains unexpected file {relative}",
                    path.display()
                );
            }
            let file_size = required_u64(file, "sizeBytes", path);
            file_total = file_total
                .checked_add(file_size)
                .expect("file size overflow");
            let file_digest = required_string(file, "sha256", path);
            validate_sha256(file_digest, path);
            file_source.push_str(&format!(
                "            RecorderFile {{ path: {relative:?}, size_bytes: {file_size}, sha256: {file_digest:?} }},\n"
            ));
        }
        if id == "recorder-web" {
            for required in ["index.html", "assets/index.js", "assets/index.css"] {
                assert!(
                    seen_files.contains(required),
                    "{} recorder web package is missing {required}",
                    path.display()
                );
            }
        }
        assert_eq!(
            file_total,
            unpacked_size_bytes,
            "{} component {id} unpacked total is inconsistent",
            path.display()
        );
        source.push_str(&format!(
            "    RecorderDelivery {{ id: {id:?}, version: {version:?}, asset: {asset:?}, download_url: {download_url:?}, size_bytes: {size_bytes}, sha256: {digest:?}, unpacked_size_bytes: {unpacked_size_bytes}, files: &[\n{file_source}        ] }},\n"
        ));
    }
    assert_eq!(seen.len(), 2);
    source.push_str("];\n");
    source
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

fn validate_relative_path(value: &str, path: &Path) {
    assert!(
        !value.is_empty()
            && value.len() <= 512
            && !value.contains('\\')
            && value
                .split('/')
                .all(|part| !part.is_empty() && part != "." && part != ".."),
        "{} contains an unsafe recorder path",
        path.display()
    );
}
