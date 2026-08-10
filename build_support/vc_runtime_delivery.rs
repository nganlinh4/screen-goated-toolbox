use std::collections::HashSet;
use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

const DEFAULT_MANIFEST: &str = "component-delivery/windows/vc-runtime-v1.json";
const RELEASE_PREFIX: &str =
    "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/sgt-runtime-bundles/";
const COMPONENT_ID: &str = "vc14-x64-runtime";
const EXPECTED_FILES: &[&str] = &[
    "bin/x64/concrt140.dll",
    "bin/x64/msvcp140.dll",
    "bin/x64/msvcp140_1.dll",
    "bin/x64/msvcp140_2.dll",
    "bin/x64/msvcp140_atomic_wait.dll",
    "bin/x64/msvcp140_codecvt_ids.dll",
    "bin/x64/vccorlib140.dll",
    "bin/x64/vcruntime140.dll",
    "bin/x64/vcruntime140_1.dll",
    "bin/x64/vcruntime140_threads.dll",
    "licenses/REDIST.txt",
    "licenses/THIRD-PARTY-NOTICES.txt",
];

pub(crate) fn generate(manifest_dir: &Path, out_dir: &Path) {
    let configured = manifest_dir.join(DEFAULT_MANIFEST);
    println!("cargo:rerun-if-changed={}", configured.display());
    for file in EXPECTED_FILES
        .iter()
        .filter(|file| file.starts_with("bin/x64/"))
    {
        let source = manifest_dir.join("src/embed_dlls/x64").join(
            Path::new(file)
                .file_name()
                .expect("VC runtime file has a name"),
        );
        println!("cargo:rerun-if-changed={}", source.display());
    }

    assert!(
        configured.is_file(),
        "missing verified VC runtime delivery: {}",
        configured.display()
    );
    let generated = delivery_source(&configured);
    let output = out_dir.join("vc_runtime_delivery.rs");
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
    let version = required_string(&value, "version", path);
    validate_identifier(version, "version", path);
    let windows = value
        .get("windows")
        .and_then(Value::as_object)
        .unwrap_or_else(|| panic!("{} is missing windows delivery data", path.display()));
    assert_eq!(
        required_object_string(windows, "architecture", path),
        "x64",
        "{} must contain only Windows x64 VC runtime data",
        path.display()
    );
    let components = windows
        .get("components")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("{} is missing windows.components", path.display()));
    assert_eq!(
        components.len(),
        1,
        "{} must deliver exactly one VC runtime component",
        path.display()
    );
    let component = components[0]
        .as_object()
        .unwrap_or_else(|| panic!("{} contains an invalid component", path.display()));
    assert_eq!(
        required_object_string(component, "id", path),
        COMPONENT_ID,
        "{} contains the wrong component id",
        path.display()
    );
    let asset = required_object_string(component, "asset", path);
    let url = required_object_string(component, "downloadUrl", path);
    let sha256 = required_object_string(component, "sha256", path);
    validate_sha256(sha256, "archive sha256", path);
    let expected_asset = format!("{COMPONENT_ID}-{version}-{}.zip", &sha256[..16]);
    assert_eq!(
        asset,
        expected_asset,
        "{} asset must be versioned and content-addressed",
        path.display()
    );
    assert_eq!(
        url,
        format!("{RELEASE_PREFIX}{asset}"),
        "{} must use the immutable runtime-bundles asset URL",
        path.display()
    );
    let size_bytes = required_positive_u64(component, "sizeBytes", path);
    let unpacked_size_bytes = required_positive_u64(component, "unpackedSizeBytes", path);
    let files = component
        .get("files")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("{} has no component files", path.display()));
    assert_eq!(
        files.len(),
        EXPECTED_FILES.len(),
        "{} has an unexpected VC runtime file count",
        path.display()
    );

    let mut seen = HashSet::new();
    let mut total = 0_u64;
    let mut file_source = String::new();
    for file in files {
        let file = file
            .as_object()
            .unwrap_or_else(|| panic!("{} contains an invalid file", path.display()));
        let file_path = required_object_string(file, "path", path);
        assert!(
            EXPECTED_FILES.contains(&file_path) && seen.insert(file_path),
            "{} contains an unexpected or repeated file {file_path}",
            path.display()
        );
        let file_size = required_positive_u64(file, "sizeBytes", path);
        total = total
            .checked_add(file_size)
            .unwrap_or_else(|| panic!("{} component size overflow", path.display()));
        let file_sha256 = required_object_string(file, "sha256", path);
        validate_sha256(file_sha256, "file sha256", path);
        file_source.push_str(&format!(
            "        VcRuntimeFile {{ path: {file_path:?}, size_bytes: {file_size}, sha256: {file_sha256:?} }},\n"
        ));
    }
    assert_eq!(
        total,
        unpacked_size_bytes,
        "{} unpacked size does not match its files",
        path.display()
    );
    format!(
        "const VC_RUNTIME_DELIVERY: Option<VcRuntimeDelivery> = Some(VcRuntimeDelivery {{\n    version: {version:?},\n    asset: {asset:?},\n    download_url: {url:?},\n    size_bytes: {size_bytes},\n    sha256: {sha256:?},\n    unpacked_size_bytes: {unpacked_size_bytes},\n    files: &[\n{file_source}    ],\n}});\n"
    )
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
