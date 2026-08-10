use std::collections::HashSet;
use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

const DEFAULT_MANIFEST: &str = "component-delivery/windows/qwen-runtime-v1.json";
const RELEASE_PREFIX: &str =
    "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/sgt-runtime-bundles/";
const COMPONENT_ID: &str = "qwen3-cuda-runtime";
const DEPENDENCY_ID: &str = "vc14-x64-runtime";
const MAX_ASSET_BYTES: u64 = 2_000_000_000;
const RUNTIME_ASSET_PATHS: &[&str] = &[
    "bin/x64/sgt_qwen3_runtime.dll",
    "licenses/PYTORCH-LICENSE.txt",
    "licenses/PYTORCH-NOTICE.txt",
    "licenses/PYTORCH-LICENSES-BUNDLED.txt",
    "licenses/CUDA-NOTICE.txt",
    "licenses/DNNL-LICENSE.txt",
    "licenses/DNNL-THIRD-PARTY-PROGRAMS.txt",
];
const LIBTORCH_PATHS: &[&str] = &[
    "bin/x64/asmjit.dll",
    "bin/x64/c10.dll",
    "bin/x64/c10_cuda.dll",
    "bin/x64/cublas64_12.dll",
    "bin/x64/cublasLt64_12.dll",
    "bin/x64/cudart64_12.dll",
    "bin/x64/cudnn64_9.dll",
    "bin/x64/cudnn_adv64_9.dll",
    "bin/x64/cudnn_cnn64_9.dll",
    "bin/x64/cudnn_engines_precompiled64_9.dll",
    "bin/x64/cudnn_engines_runtime_compiled64_9.dll",
    "bin/x64/cudnn_graph64_9.dll",
    "bin/x64/cudnn_heuristic64_9.dll",
    "bin/x64/cudnn_ops64_9.dll",
    "bin/x64/cufft64_11.dll",
    "bin/x64/cupti64_2025.1.0.dll",
    "bin/x64/cusolver64_11.dll",
    "bin/x64/cusparse64_12.dll",
    "bin/x64/fbgemm.dll",
    "bin/x64/libiomp5md.dll",
    "bin/x64/nvJitLink_120_0.dll",
    "bin/x64/nvrtc-builtins64_128.dll",
    "bin/x64/nvrtc64_120_0.dll",
    "bin/x64/torch_cpu.dll",
    "bin/x64/torch_cuda.dll",
    "bin/x64/uv.dll",
    "bin/x64/zlibwapi.dll",
    "metadata/build-hash",
    "metadata/build-version",
];

pub(crate) fn generate(manifest_dir: &Path, out_dir: &Path) {
    let configured = manifest_dir.join(DEFAULT_MANIFEST);
    println!("cargo:rerun-if-changed={}", configured.display());
    assert!(
        configured.is_file(),
        "missing verified Qwen runtime delivery: {}",
        configured.display()
    );
    let generated = delivery_source(&configured);
    let output = out_dir.join("qwen_runtime_delivery.rs");
    fs::write(&output, generated)
        .unwrap_or_else(|error| panic!("failed to write {}: {error}", output.display()));
}

fn delivery_source(path: &Path) -> String {
    let value: Value = serde_json::from_str(
        &fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display())),
    )
    .unwrap_or_else(|error| panic!("invalid {}: {error}", path.display()));
    let root = value.as_object().expect("Qwen3 delivery must be an object");
    assert_eq!(required_u64(root, "schemaVersion", path), 1);
    let version = required_string(root, "version", path);
    validate_identifier(version, path);
    let windows = required_object(root, "windows", path);
    assert_eq!(required_string(windows, "architecture", path), "x64");
    let components = required_array(windows, "components", path);
    assert_eq!(
        components.len(),
        1,
        "{} must have one component",
        path.display()
    );
    let component = components[0]
        .as_object()
        .expect("component must be an object");
    assert_eq!(required_string(component, "id", path), COMPONENT_ID);
    assert_eq!(
        required_array(component, "dependencies", path),
        &[Value::String(DEPENDENCY_ID.to_string())]
    );

    let assets = required_array(component, "assets", path);
    assert_eq!(
        assets.len(),
        3,
        "{} must contain three Qwen3 packs",
        path.display()
    );
    let mut asset_source = String::new();
    let mut asset_names = HashSet::new();
    for (index, asset) in assets.iter().enumerate() {
        let asset = asset.as_object().expect("asset must be an object");
        let name = required_string(asset, "asset", path);
        let url = required_string(asset, "downloadUrl", path);
        let size = required_positive_u64(asset, "sizeBytes", path);
        assert!(
            size < MAX_ASSET_BYTES,
            "{} contains an oversized asset",
            path.display()
        );
        let sha = required_string(asset, "sha256", path);
        validate_sha256(sha, path);
        assert!(
            asset_names.insert(name),
            "{} repeats an asset",
            path.display()
        );
        let prefix = if index == 0 {
            format!("{COMPONENT_ID}-{version}-")
        } else {
            format!("qwen3-cuda-libtorch-{version}-part{index}-")
        };
        assert_eq!(name, format!("{prefix}{}.zip", &sha[..16]));
        assert_eq!(url, format!("{RELEASE_PREFIX}{name}"));
        asset_source.push_str(&format!(
            "        QwenRuntimeArchive {{ url: {url:?}, size_bytes: {size}, sha256: {sha:?} }},\n"
        ));
    }

    let unpacked_size = required_positive_u64(component, "unpackedSizeBytes", path);
    let files = required_array(component, "files", path);
    assert_eq!(
        files.len(),
        RUNTIME_ASSET_PATHS.len() + LIBTORCH_PATHS.len(),
        "{} has wrong Qwen3 file count",
        path.display()
    );
    let mut paths = HashSet::new();
    let mut runtime_paths = HashSet::new();
    let mut libtorch_paths = HashSet::new();
    let mut used_archives = HashSet::new();
    let mut total = 0_u64;
    let mut file_source = String::new();
    for file in files {
        let file = file.as_object().expect("file must be an object");
        let archive_index = required_u64(file, "archiveIndex", path) as usize;
        assert!(
            archive_index < assets.len(),
            "{} has an invalid archive index",
            path.display()
        );
        let archive_path = required_string(file, "archivePath", path);
        let target_path = required_string(file, "path", path);
        validate_relative_path(archive_path, path);
        validate_relative_path(target_path, path);
        assert_eq!(archive_path, target_path);
        assert!(
            paths.insert(target_path),
            "{} repeats target path",
            path.display()
        );
        if archive_index == 0 {
            assert!(RUNTIME_ASSET_PATHS.contains(&target_path));
            runtime_paths.insert(target_path);
        } else {
            assert!(LIBTORCH_PATHS.contains(&target_path));
            libtorch_paths.insert(target_path);
        }
        used_archives.insert(archive_index);
        let size = required_positive_u64(file, "sizeBytes", path);
        total = total
            .checked_add(size)
            .expect("Qwen3 unpacked size overflow");
        let sha = required_string(file, "sha256", path);
        validate_sha256(sha, path);
        file_source.push_str(&format!(
            "        QwenRuntimeFile {{ archive_index: {archive_index}, archive_path: {archive_path:?}, path: {target_path:?}, size_bytes: {size}, sha256: {sha:?} }},\n"
        ));
    }
    assert_eq!(used_archives, (0..assets.len()).collect());
    assert_eq!(runtime_paths, RUNTIME_ASSET_PATHS.iter().copied().collect());
    assert_eq!(libtorch_paths, LIBTORCH_PATHS.iter().copied().collect());
    assert_eq!(total, unpacked_size);
    format!(
        "const QWEN_RUNTIME_DELIVERY: Option<QwenRuntimeDelivery> = Some(QwenRuntimeDelivery {{\n    version: {version:?},\n    archives: &[\n{asset_source}    ],\n    unpacked_size_bytes: {unpacked_size},\n    files: &[\n{file_source}    ],\n}});\n"
    )
}

fn required_object<'a>(
    value: &'a Map<String, Value>,
    field: &str,
    path: &Path,
) -> &'a Map<String, Value> {
    value
        .get(field)
        .and_then(Value::as_object)
        .unwrap_or_else(|| panic!("{} is missing {field}", path.display()))
}

fn required_array<'a>(value: &'a Map<String, Value>, field: &str, path: &Path) -> &'a Vec<Value> {
    value
        .get(field)
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("{} is missing {field}", path.display()))
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
        .unwrap_or_else(|| panic!("{} is missing {field}", path.display()))
}

fn required_positive_u64(value: &Map<String, Value>, field: &str, path: &Path) -> u64 {
    let result = required_u64(value, field, path);
    assert!(result > 0, "{} has invalid {field}", path.display());
    result
}

fn validate_identifier(value: &str, path: &Path) {
    assert!(
        value.len() <= 80
            && value.bytes().all(|byte| byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'.' | b'-' | b'_')),
        "{} has invalid version",
        path.display()
    );
}

fn validate_relative_path(value: &str, path: &Path) {
    assert!(
        !value.is_empty()
            && !value.contains('\0')
            && !value.contains('\\')
            && value
                .split('/')
                .all(|part| !part.is_empty() && part != "." && part != ".."),
        "{} has unsafe file path",
        path.display()
    );
}

fn validate_sha256(value: &str, path: &Path) {
    assert!(
        value.len() == 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "{} has invalid checksum",
        path.display()
    );
}
