use std::fs;
use std::io::{Read, Seek, SeekFrom};
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

const REQUIRED_RUNTIME_DLLS: [&str; 3] = [ONNX_DLL, ONNX_SHARED_DLL, DIRECTML_DLL];
const DOS_HEADER_LEN: usize = 64;
const PE_PREFIX_LEN: u64 = 6;
const PE_OFFSET_FIELD: std::ops::Range<usize> = 0x3c..0x40;
const IMAGE_FILE_MACHINE_AMD64: u16 = 0x8664;
const IMAGE_FILE_MACHINE_ARM64: u16 = 0xaa64;

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
    REQUIRED_RUNTIME_DLLS
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
    let missing: Vec<&str> = REQUIRED_RUNTIME_DLLS
        .into_iter()
        .filter(|name| !bin_dir.join(name).exists())
        .collect();

    if !missing.is_empty() {
        return Some(format!(
            "Local AI runtime is incomplete. Missing: {}",
            missing.join(", ")
        ));
    }

    for name in REQUIRED_RUNTIME_DLLS {
        if let Err(detail) = validate_runtime_dll(&bin_dir.join(name), name) {
            return Some(format!(
                "Local AI runtime is invalid. {detail} Reinstall required."
            ));
        }
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

fn validate_runtime_dll(path: &Path, name: &str) -> Result<(), String> {
    let metadata =
        fs::metadata(path).map_err(|error| format!("{name} cannot be inspected: {error}."))?;
    if !metadata.is_file() {
        return Err(format!("{name} is not a regular file."));
    }
    if metadata.len() == 0 {
        return Err(format!("{name} is empty."));
    }
    if metadata.len() < DOS_HEADER_LEN as u64 {
        return Err(format!(
            "{name} is truncated ({} bytes; DOS header needs {DOS_HEADER_LEN}).",
            metadata.len()
        ));
    }

    let mut file =
        fs::File::open(path).map_err(|error| format!("{name} cannot be read: {error}."))?;
    let mut dos_header = [0_u8; DOS_HEADER_LEN];
    file.read_exact(&mut dos_header)
        .map_err(|error| format!("{name} DOS header cannot be read: {error}."))?;
    if &dos_header[..2] != b"MZ" {
        return Err(format!("{name} has an invalid DOS MZ signature."));
    }

    let pe_offset = u32::from_le_bytes(
        dos_header[PE_OFFSET_FIELD]
            .try_into()
            .expect("PE offset field has a fixed length"),
    ) as u64;
    if pe_offset < DOS_HEADER_LEN as u64
        || pe_offset
            .checked_add(PE_PREFIX_LEN)
            .is_none_or(|end| end > metadata.len())
    {
        return Err(format!(
            "{name} has an out-of-range PE header offset {pe_offset} for a {}-byte file.",
            metadata.len()
        ));
    }

    file.seek(SeekFrom::Start(pe_offset))
        .map_err(|error| format!("{name} PE header cannot be reached: {error}."))?;
    let mut pe_prefix = [0_u8; PE_PREFIX_LEN as usize];
    file.read_exact(&mut pe_prefix)
        .map_err(|error| format!("{name} PE header cannot be read: {error}."))?;
    if &pe_prefix[..4] != b"PE\0\0" {
        return Err(format!("{name} has an invalid PE signature."));
    }

    let actual_machine = u16::from_le_bytes([pe_prefix[4], pe_prefix[5]]);
    let expected_machine = expected_pe_machine();
    if actual_machine != expected_machine {
        return Err(format!(
            "{name} architecture mismatch: found {} (0x{actual_machine:04x}), expected {} (0x{expected_machine:04x}).",
            pe_machine_name(actual_machine),
            pe_machine_name(expected_machine)
        ));
    }
    Ok(())
}

fn expected_pe_machine() -> u16 {
    match runtime_arch() {
        crate::runtime_support::RuntimeArch::X64 => IMAGE_FILE_MACHINE_AMD64,
        crate::runtime_support::RuntimeArch::Arm64 => IMAGE_FILE_MACHINE_ARM64,
    }
}

fn pe_machine_name(machine: u16) -> &'static str {
    match machine {
        IMAGE_FILE_MACHINE_AMD64 => "x64",
        IMAGE_FILE_MACHINE_ARM64 => "arm64",
        0x014c => "x86",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

    struct RuntimeFixture {
        path: PathBuf,
    }

    impl RuntimeFixture {
        fn valid() -> Self {
            let id = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("sgt-ai-runtime-pe-{}-{id}", std::process::id()));
            fs::create_dir_all(&path).expect("create runtime fixture");
            let fixture = Self { path };
            let pe = minimal_pe(expected_pe_machine());
            for name in REQUIRED_RUNTIME_DLLS {
                fs::write(fixture.path.join(name), &pe).expect("write runtime DLL fixture");
            }
            fs::write(
                runtime_marker_path(&fixture.path),
                expected_runtime_marker_contents(),
            )
            .expect("write runtime marker fixture");
            fixture
        }

        fn replace(&self, name: &str, bytes: &[u8]) {
            fs::write(self.path.join(name), bytes).expect("replace runtime DLL fixture");
        }

        fn issue(&self) -> Option<String> {
            runtime_health_issue(&self.path)
        }
    }

    impl Drop for RuntimeFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn minimal_pe(machine: u16) -> Vec<u8> {
        let pe_offset = 0x80_usize;
        let mut bytes = vec![0_u8; pe_offset + PE_PREFIX_LEN as usize];
        bytes[..2].copy_from_slice(b"MZ");
        bytes[PE_OFFSET_FIELD].copy_from_slice(&(pe_offset as u32).to_le_bytes());
        bytes[pe_offset..pe_offset + 4].copy_from_slice(b"PE\0\0");
        bytes[pe_offset + 4..pe_offset + 6].copy_from_slice(&machine.to_le_bytes());
        bytes
    }

    #[test]
    fn valid_minimal_pe_runtime_and_existing_marker_are_accepted() {
        let fixture = RuntimeFixture::valid();
        assert_eq!(fixture.issue(), None);
    }

    #[test]
    fn zero_length_runtime_dll_is_rejected_specifically() {
        let fixture = RuntimeFixture::valid();
        fixture.replace(ONNX_DLL, &[]);
        let issue = fixture.issue().expect("zero-length DLL must fail");
        assert!(issue.contains(ONNX_DLL));
        assert!(issue.contains("empty"));
    }

    #[test]
    fn truncated_runtime_dll_is_rejected_specifically() {
        let fixture = RuntimeFixture::valid();
        fixture.replace(ONNX_SHARED_DLL, b"MZ");
        let issue = fixture.issue().expect("truncated DLL must fail");
        assert!(issue.contains(ONNX_SHARED_DLL));
        assert!(issue.contains("truncated"));
    }

    #[test]
    fn wrong_architecture_runtime_dll_is_rejected_specifically() {
        let fixture = RuntimeFixture::valid();
        let wrong_machine = match runtime_arch() {
            crate::runtime_support::RuntimeArch::X64 => IMAGE_FILE_MACHINE_ARM64,
            crate::runtime_support::RuntimeArch::Arm64 => IMAGE_FILE_MACHINE_AMD64,
        };
        fixture.replace(DIRECTML_DLL, &minimal_pe(wrong_machine));
        let issue = fixture.issue().expect("wrong-architecture DLL must fail");
        assert!(issue.contains(DIRECTML_DLL));
        assert!(issue.contains("architecture mismatch"));
    }

    #[test]
    fn malformed_pe_boundaries_and_signatures_are_diagnosed() {
        let fixture = RuntimeFixture::valid();

        let mut bytes = minimal_pe(expected_pe_machine());
        bytes[0] = b'N';
        fixture.replace(ONNX_DLL, &bytes);
        assert!(
            fixture
                .issue()
                .is_some_and(|issue| issue.contains("DOS MZ signature"))
        );

        let mut bytes = minimal_pe(expected_pe_machine());
        bytes[PE_OFFSET_FIELD].copy_from_slice(&u32::MAX.to_le_bytes());
        fixture.replace(ONNX_DLL, &bytes);
        assert!(
            fixture
                .issue()
                .is_some_and(|issue| issue.contains("PE header offset"))
        );

        let mut bytes = minimal_pe(expected_pe_machine());
        bytes[0x80] = b'Q';
        fixture.replace(ONNX_DLL, &bytes);
        assert!(
            fixture
                .issue()
                .is_some_and(|issue| issue.contains("PE signature"))
        );
    }
}
