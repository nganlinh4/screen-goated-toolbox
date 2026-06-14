use std::fs;
use std::path::{Path, PathBuf};

pub(super) const ONNX_RUNTIME_VERSION: &str = "1.24.2";
pub(super) const DIRECTML_VERSION: &str = "1.15.4";
pub(super) const ONNX_PACKAGE_URL: &str = "https://api.nuget.org/v3-flatcontainer/microsoft.ml.onnxruntime.directml/1.24.2/microsoft.ml.onnxruntime.directml.1.24.2.nupkg";
pub(super) const DIRECTML_PACKAGE_URL: &str = "https://api.nuget.org/v3-flatcontainer/microsoft.ai.directml/1.15.4/microsoft.ai.directml.1.15.4.nupkg";
pub(super) const ONNX_ARCHIVE_NAME: &str = "onnxruntime-directml-1.24.2.nupkg";
pub(super) const DIRECTML_ARCHIVE_NAME: &str = "directml-1.15.4.nupkg";
pub(super) const ONNX_DLL: &str = "onnxruntime.dll";
pub(super) const ONNX_SHARED_DLL: &str = "onnxruntime_providers_shared.dll";
pub(super) const DIRECTML_DLL: &str = "DirectML.dll";
pub(super) const RUNTIME_VERSION_MARKER: &str = "ai-runtime-version.txt";

#[derive(Clone, Copy)]
pub(super) struct RuntimePackage {
    pub(super) url: &'static str,
    pub(super) archive_name: &'static str,
    pub(super) label: &'static str,
    pub(super) progress_start: f32,
    pub(super) progress_end: f32,
}

const ONNX_X64_ENTRIES: &[super::super::remote_zip::RequestedZipEntry] = &[
    super::super::remote_zip::RequestedZipEntry {
        source_path: "runtimes/win-x64/native/onnxruntime.dll",
        dest_name: ONNX_DLL,
    },
    super::super::remote_zip::RequestedZipEntry {
        source_path: "runtimes/win-x64/native/onnxruntime_providers_shared.dll",
        dest_name: ONNX_SHARED_DLL,
    },
];

const ONNX_ARM64_ENTRIES: &[super::super::remote_zip::RequestedZipEntry] = &[
    super::super::remote_zip::RequestedZipEntry {
        source_path: "runtimes/win-arm64/native/onnxruntime.dll",
        dest_name: ONNX_DLL,
    },
    super::super::remote_zip::RequestedZipEntry {
        source_path: "runtimes/win-arm64/native/onnxruntime_providers_shared.dll",
        dest_name: ONNX_SHARED_DLL,
    },
];

const DIRECTML_X64_ENTRIES: &[super::super::remote_zip::RequestedZipEntry] =
    &[super::super::remote_zip::RequestedZipEntry {
        source_path: "bin/x64-win/DirectML.dll",
        dest_name: DIRECTML_DLL,
    }];

const DIRECTML_ARM64_ENTRIES: &[super::super::remote_zip::RequestedZipEntry] =
    &[super::super::remote_zip::RequestedZipEntry {
        source_path: "bin/arm64-win/DirectML.dll",
        dest_name: DIRECTML_DLL,
    }];

pub(super) const PACKAGES: &[RuntimePackage] = &[
    RuntimePackage {
        url: ONNX_PACKAGE_URL,
        archive_name: ONNX_ARCHIVE_NAME,
        label: "Downloading ONNX Runtime",
        progress_start: 0.0,
        progress_end: 48.0,
    },
    RuntimePackage {
        url: DIRECTML_PACKAGE_URL,
        archive_name: DIRECTML_ARCHIVE_NAME,
        label: "Downloading DirectML",
        progress_start: 50.0,
        progress_end: 98.0,
    },
];

pub(super) fn runtime_arch() -> crate::runtime_support::RuntimeArch {
    crate::runtime_support::tool_download_arch()
}

pub(super) fn package_entries(
    package: RuntimePackage,
) -> &'static [super::super::remote_zip::RequestedZipEntry] {
    match (runtime_arch(), package.archive_name) {
        (crate::runtime_support::RuntimeArch::Arm64, ONNX_ARCHIVE_NAME) => ONNX_ARM64_ENTRIES,
        (crate::runtime_support::RuntimeArch::Arm64, DIRECTML_ARCHIVE_NAME) => {
            DIRECTML_ARM64_ENTRIES
        }
        (_, ONNX_ARCHIVE_NAME) => ONNX_X64_ENTRIES,
        (_, DIRECTML_ARCHIVE_NAME) => DIRECTML_X64_ENTRIES,
        _ => unreachable!("unknown runtime package archive name"),
    }
}

pub(super) fn package_name(package: RuntimePackage) -> &'static str {
    package.label.trim_start_matches("Downloading ")
}

pub(super) fn core_runtime_present(bin_dir: &Path) -> bool {
    runtime_health_issue(bin_dir).is_none()
}

pub(super) fn runtime_bytes(bin_dir: &Path) -> u64 {
    [ONNX_DLL, ONNX_SHARED_DLL, DIRECTML_DLL]
        .into_iter()
        .filter_map(|name| fs::metadata(bin_dir.join(name)).ok())
        .map(|meta| meta.len())
        .sum()
}

pub(super) fn runtime_marker_path(bin_dir: &Path) -> PathBuf {
    bin_dir.join(RUNTIME_VERSION_MARKER)
}

pub(super) fn expected_runtime_marker_contents() -> String {
    format!(
        "onnxruntime={}\ndirectml={}\n",
        ONNX_RUNTIME_VERSION, DIRECTML_VERSION
    )
}

pub(super) fn has_runtime_artifacts(bin_dir: &Path) -> bool {
    [
        ONNX_DLL,
        ONNX_SHARED_DLL,
        DIRECTML_DLL,
        RUNTIME_VERSION_MARKER,
    ]
    .into_iter()
    .any(|name| bin_dir.join(name).exists())
}

pub(super) fn runtime_health_issue(bin_dir: &Path) -> Option<String> {
    let missing: Vec<&str> = [ONNX_DLL, ONNX_SHARED_DLL, DIRECTML_DLL]
        .into_iter()
        .filter(|name| !bin_dir.join(name).exists())
        .collect();

    if !missing.is_empty() {
        return Some(format!(
            "Local AI runtime is incomplete. Missing: {}",
            missing.join(", ")
        ));
    }

    let marker_path = runtime_marker_path(bin_dir);
    let expected = expected_runtime_marker_contents();
    match fs::read_to_string(&marker_path) {
        Ok(contents) if contents == expected => None,
        Ok(_) => Some("Local AI runtime is outdated. Reinstall required.".to_string()),
        Err(_) => {
            Some("Local AI runtime version marker is missing. Reinstall required.".to_string())
        }
    }
}
