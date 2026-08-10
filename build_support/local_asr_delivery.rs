use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

const MANIFEST_ENV: &str = "SGT_LOCAL_ASR_DELIVERY_MANIFEST";
const DEFAULT_MANIFEST: &str = "local-runtime-bundles/sgt_local_asr/sgt_local_asr.delivery.json";
const RELEASE_PREFIX: &str =
    "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/sgt-runtime-bundles/";
const COMPONENTS: &[(&str, &[&str])] = &[
    (
        "local-asr-worker",
        &[
            "bin/x64/sgt-local-asr-worker.exe",
            "licenses/parakeet-rs-LICENSE.txt",
        ],
    ),
    (
        "onnx-directml-runtime",
        &[
            "bin/x64/onnxruntime.dll",
            "bin/x64/onnxruntime_providers_shared.dll",
            "bin/x64/DirectML.dll",
            "licenses/onnxruntime-LICENSE.txt",
            "licenses/onnxruntime-ThirdPartyNotices.txt",
            "licenses/directml-LICENSE-CODE.txt",
            "licenses/directml-LICENSE.txt",
            "licenses/directml-ThirdPartyNotices.txt",
        ],
    ),
];

pub(crate) fn generate(manifest_dir: &Path, out_dir: &Path) {
    println!("cargo:rerun-if-env-changed={MANIFEST_ENV}");
    let configured = std::env::var_os(MANIFEST_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest_dir.join(DEFAULT_MANIFEST));
    println!("cargo:rerun-if-changed={}", configured.display());
    let generated = if configured.is_file() {
        delivery_source(&configured)
    } else {
        "const LOCAL_ASR_DELIVERIES: &[LocalAsrDelivery] = &[];\n".to_string()
    };
    let output = out_dir.join("local_asr_delivery.rs");
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
    let components = value
        .get("windows")
        .and_then(Value::as_object)
        .unwrap_or_else(|| panic!("{} is missing windows delivery data", path.display()));
    assert_eq!(
        required_string(components, "architecture", path),
        "x64",
        "{} must contain Windows x64 packages",
        path.display()
    );
    let entries = components
        .get("components")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("{} is missing windows.components", path.display()));
    assert_eq!(
        entries.len(),
        COMPONENTS.len(),
        "{} must deliver both local ASR components",
        path.display()
    );

    let mut seen = HashSet::new();
    let mut source = String::from("const LOCAL_ASR_DELIVERIES: &[LocalAsrDelivery] = &[\n");
    for entry in entries {
        let entry = entry
            .as_object()
            .unwrap_or_else(|| panic!("{} contains an invalid component", path.display()));
        let id = required_string(entry, "id", path);
        let expected_files = COMPONENTS
            .iter()
            .find_map(|(candidate, files)| (*candidate == id).then_some(*files))
            .unwrap_or_else(|| panic!("{} contains unsupported component {id}", path.display()));
        assert!(seen.insert(id), "{} repeats component {id}", path.display());
        let version = required_string(entry, "version", path);
        validate_identifier(version, path);
        let sha256 = required_string(entry, "sha256", path);
        validate_sha256(sha256, path);
        let asset = required_string(entry, "asset", path);
        assert_eq!(
            asset,
            format!("{id}-{version}-{}.zip", &sha256[..16]),
            "{} asset must be versioned and content-addressed",
            path.display()
        );
        let download_url = required_string(entry, "downloadUrl", path);
        assert_eq!(
            download_url,
            format!("{RELEASE_PREFIX}{asset}"),
            "{} asset URL must use the immutable runtime-bundles release",
            path.display()
        );
        let size_bytes = required_u64(entry, "sizeBytes", path);
        let unpacked_size_bytes = required_u64(entry, "unpackedSizeBytes", path);
        let files = entry
            .get("files")
            .and_then(Value::as_array)
            .unwrap_or_else(|| panic!("{} component {id} has no files", path.display()));
        assert_eq!(
            files.len(),
            expected_files.len(),
            "{} component {id} has an unexpected file count",
            path.display()
        );
        let mut seen_files = HashSet::new();
        let mut file_total = 0_u64;
        let mut file_source = String::new();
        for file in files {
            let file = file
                .as_object()
                .unwrap_or_else(|| panic!("{} contains an invalid file", path.display()));
            let relative = required_string(file, "path", path);
            assert!(
                expected_files.contains(&relative),
                "{} component {id} contains unexpected file {relative}",
                path.display()
            );
            assert!(
                seen_files.insert(relative),
                "{} component {id} repeats file {relative}",
                path.display()
            );
            let file_size = required_u64(file, "sizeBytes", path);
            file_total = file_total
                .checked_add(file_size)
                .expect("file size overflow");
            let file_sha = required_string(file, "sha256", path);
            validate_sha256(file_sha, path);
            file_source.push_str(&format!(
                "            LocalAsrFile {{ path: {relative:?}, size_bytes: {file_size}, sha256: {file_sha:?} }},\n"
            ));
        }
        assert_eq!(
            file_total,
            unpacked_size_bytes,
            "{} component {id} unpacked total is inconsistent",
            path.display()
        );
        source.push_str(&format!(
            "    LocalAsrDelivery {{ id: {id:?}, version: {version:?}, download_url: {download_url:?}, size_bytes: {size_bytes}, sha256: {sha256:?}, unpacked_size_bytes: {unpacked_size_bytes}, files: &[\n{file_source}        ] }},\n"
        ));
    }
    assert_eq!(seen.len(), COMPONENTS.len());
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
