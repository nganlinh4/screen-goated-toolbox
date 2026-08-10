use std::fs;
use std::io::{Read as _, Seek as _, SeekFrom, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

use super::super::receipt::{OwnedComponentFile, file_matches};
use super::{
    ComponentKind, LocalAsrDelivery, LocalAsrFile, MAX_COMPONENT_FILES, WORKER_ID, owned_file,
    receipt, staging, validate_delivery, version_root,
};
#[cfg(debug_assertions)]
use super::{RUNTIME_FILES, RUNTIME_ID, RUNTIME_VERSION, validate_runtime_install};
#[cfg(debug_assertions)]
use anyhow::Context as _;
use anyhow::{Result, anyhow, bail};

#[cfg(debug_assertions)]
const ONNX_URL: &str = "https://api.nuget.org/v3-flatcontainer/microsoft.ml.onnxruntime.directml/1.24.2/microsoft.ml.onnxruntime.directml.1.24.2.nupkg";
#[cfg(debug_assertions)]
const ONNX_SIZE: u64 = 12_411_398;
#[cfg(debug_assertions)]
const ONNX_SHA256: &str = "c9b8adb96dfb5578097bea42a7d9b7ff8f300fb3c3a6f3052fe5b702628ab681";
#[cfg(debug_assertions)]
const DIRECTML_URL: &str = "https://api.nuget.org/v3-flatcontainer/microsoft.ai.directml/1.15.4/microsoft.ai.directml.1.15.4.nupkg";
#[cfg(debug_assertions)]
const DIRECTML_SIZE: u64 = 202_292_617;
#[cfg(debug_assertions)]
const DIRECTML_SHA256: &str = "4e7cb7ddce8cf837a7a75dc029209b520ca0101470fcdf275c1f49736a3615b9";
const IMAGE_FILE_MACHINE_AMD64: u16 = 0x8664;

static INSTALL_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
static INSTALL_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[cfg(debug_assertions)]
struct SourcePackage {
    url: &'static str,
    size_bytes: u64,
    sha256: &'static str,
    entries: &'static [SourceEntry],
}

#[cfg(debug_assertions)]
struct SourceEntry {
    archive_path: &'static str,
    output_path: &'static str,
}

#[cfg(debug_assertions)]
const ONNX_ENTRIES: &[SourceEntry] = &[
    SourceEntry {
        archive_path: "runtimes/win-x64/native/onnxruntime.dll",
        output_path: "bin/x64/onnxruntime.dll",
    },
    SourceEntry {
        archive_path: "runtimes/win-x64/native/onnxruntime_providers_shared.dll",
        output_path: "bin/x64/onnxruntime_providers_shared.dll",
    },
    SourceEntry {
        archive_path: "LICENSE",
        output_path: "licenses/onnxruntime-LICENSE.txt",
    },
    SourceEntry {
        archive_path: "ThirdPartyNotices.txt",
        output_path: "licenses/onnxruntime-ThirdPartyNotices.txt",
    },
];
#[cfg(debug_assertions)]
const DIRECTML_ENTRIES: &[SourceEntry] = &[
    SourceEntry {
        archive_path: "bin/x64-win/DirectML.dll",
        output_path: "bin/x64/DirectML.dll",
    },
    SourceEntry {
        archive_path: "LICENSE-CODE.txt",
        output_path: "licenses/directml-LICENSE-CODE.txt",
    },
    SourceEntry {
        archive_path: "LICENSE.txt",
        output_path: "licenses/directml-LICENSE.txt",
    },
    SourceEntry {
        archive_path: "ThirdPartyNotices.txt",
        output_path: "licenses/directml-ThirdPartyNotices.txt",
    },
];
#[cfg(debug_assertions)]
const SOURCE_PACKAGES: &[SourcePackage] = &[
    SourcePackage {
        url: ONNX_URL,
        size_bytes: ONNX_SIZE,
        sha256: ONNX_SHA256,
        entries: ONNX_ENTRIES,
    },
    SourcePackage {
        url: DIRECTML_URL,
        size_bytes: DIRECTML_SIZE,
        sha256: DIRECTML_SHA256,
        entries: DIRECTML_ENTRIES,
    },
];

pub(super) fn ensure_delivery(
    delivery: &'static LocalAsrDelivery,
    cancelled: &std::sync::atomic::AtomicBool,
    on_progress: impl Fn(u64, u64),
) -> Result<()> {
    if validate_delivery(delivery).is_ok() {
        return Ok(());
    }
    let _guard = INSTALL_LOCK
        .lock()
        .unwrap_or_else(|value| value.into_inner());
    if validate_delivery(delivery).is_ok() {
        return Ok(());
    }
    clear_invalid(ComponentKind::from_id(delivery.id)?)?;
    let _lease = super::super::acquire(delivery.id)?;
    install_delivery(delivery, cancelled, on_progress)
}

#[cfg(debug_assertions)]
pub(super) fn ensure_development_runtime(
    cancelled: &std::sync::atomic::AtomicBool,
    on_progress: impl Fn(u64, u64),
) -> Result<()> {
    if validate_runtime_install().is_ok() {
        return Ok(());
    }
    let _guard = INSTALL_LOCK
        .lock()
        .unwrap_or_else(|value| value.into_inner());
    if validate_runtime_install().is_ok() {
        return Ok(());
    }
    clear_invalid(ComponentKind::Runtime)?;
    let _lease = super::super::acquire(RUNTIME_ID)?;
    install_development_runtime(cancelled, on_progress)
}

fn clear_invalid(kind: ComponentKind) -> Result<()> {
    if kind == ComponentKind::Runtime {
        match super::super::request_remove(WORKER_ID)? {
            super::super::RemovalOutcome::Missing | super::super::RemovalOutcome::Removed => {}
            super::super::RemovalOutcome::Pending => {
                bail!("local ASR worker is active; runtime repair is pending")
            }
            super::super::RemovalOutcome::RequiredBy(dependents) => bail!(
                "local ASR worker repair is blocked by: {}",
                dependents.join(", ")
            ),
            super::super::RemovalOutcome::PreservedModified(paths) => bail!(
                "local ASR worker has {} modified managed file(s)",
                paths.len()
            ),
        }
    }
    match super::super::request_remove(kind.id())? {
        super::super::RemovalOutcome::Missing | super::super::RemovalOutcome::Removed => Ok(()),
        super::super::RemovalOutcome::Pending => {
            bail!("{} is active; repair is pending", kind.id())
        }
        super::super::RemovalOutcome::RequiredBy(dependents) => bail!(
            "{} repair is blocked by installed components: {}",
            kind.id(),
            dependents.join(", ")
        ),
        super::super::RemovalOutcome::PreservedModified(paths) => {
            bail!("{} has {} modified managed file(s)", kind.id(), paths.len())
        }
    }
}

fn install_delivery(
    delivery: &LocalAsrDelivery,
    cancelled: &std::sync::atomic::AtomicBool,
    on_progress: impl Fn(u64, u64),
) -> Result<()> {
    let (archive, staging_root) = working_paths(delivery.id, delivery.version)?;
    let result = (|| {
        download(
            delivery.download_url,
            delivery.size_bytes,
            &archive,
            cancelled,
            &on_progress,
        )?;
        verify_file(&archive, delivery.size_bytes, delivery.sha256)?;
        extract_delivery(&archive, &staging_root, delivery)?;
        publish(
            &staging_root,
            delivery.id,
            delivery.version,
            ComponentKind::from_id(delivery.id)?,
            delivery.files,
        )?;
        validate_delivery(delivery)
    })();
    let _ = fs::remove_file(&archive);
    if staging_root.exists() {
        let mut owned = delivery
            .files
            .iter()
            .map(|file| PathBuf::from(file.path))
            .collect::<Vec<_>>();
        owned.push("receipt.json".into());
        let _ = staging::cleanup_owned(&staging_root, &owned);
    }
    result
}

#[cfg(debug_assertions)]
fn install_development_runtime(
    cancelled: &std::sync::atomic::AtomicBool,
    on_progress: impl Fn(u64, u64),
) -> Result<()> {
    let (_, staging_root) = working_paths(RUNTIME_ID, RUNTIME_VERSION)?;
    let result = (|| {
        if !adopt_legacy_runtime(&staging_root)? {
            install_source_packages(&staging_root, cancelled, &on_progress)?;
        }
        publish(
            &staging_root,
            RUNTIME_ID,
            RUNTIME_VERSION,
            ComponentKind::Runtime,
            RUNTIME_FILES,
        )?;
        validate_runtime_install()
    })();
    if staging_root.exists() {
        let mut owned = RUNTIME_FILES
            .iter()
            .map(|file| PathBuf::from(file.path))
            .collect::<Vec<_>>();
        owned.extend([
            PathBuf::from("source-0.nupkg"),
            PathBuf::from("source-1.nupkg"),
            PathBuf::from("receipt.json"),
        ]);
        let _ = staging::cleanup_owned(&staging_root, &owned);
    }
    result
}

fn working_paths(id: &str, version: &str) -> Result<(PathBuf, PathBuf)> {
    let sequence = INSTALL_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let working_root = crate::paths::app_local_data_dir();
    let scratch = working_root.join("component-downloads");
    staging::ensure_directory_tree(&working_root, &scratch)?;
    let archive = scratch.join(format!("{id}-{}-{sequence}.download", std::process::id()));
    let staging_parent = working_root.join("component-staging");
    staging::ensure_directory_tree(&working_root, &staging_parent)?;
    let staging_root =
        staging_parent.join(format!("{id}-{version}-{}-{sequence}", std::process::id()));
    fs::create_dir(&staging_root)?;
    Ok((archive, staging_root))
}

fn download(
    url: &str,
    expected_size: u64,
    target: &Path,
    cancelled: &std::sync::atomic::AtomicBool,
    on_progress: &impl Fn(u64, u64),
) -> Result<()> {
    let response = crate::api::client::UREQ_DOWNLOAD_AGENT
        .get(url)
        .header("User-Agent", "ScreenGoatedToolbox")
        .call()
        .map_err(|error| anyhow!("local ASR component download failed: {error}"))?;
    if response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|size| size != expected_size)
    {
        bail!("local ASR component download size does not match this build");
    }
    let mut reader = response.into_body().into_reader();
    let mut output = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(target)?;
    let mut buffer = [0_u8; 128 * 1024];
    let mut total = 0_u64;
    loop {
        if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
            bail!("local ASR component download cancelled");
        }
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total = total
            .checked_add(read as u64)
            .filter(|total| *total <= expected_size)
            .ok_or_else(|| anyhow!("local ASR component download exceeds its limit"))?;
        output.write_all(&buffer[..read])?;
        on_progress(total, expected_size);
    }
    if total != expected_size {
        bail!("local ASR component download is incomplete");
    }
    output.flush()?;
    output.sync_all()?;
    Ok(())
}

fn extract_delivery(
    archive_path: &Path,
    staging_root: &Path,
    delivery: &LocalAsrDelivery,
) -> Result<()> {
    let mut archive = zip::ZipArchive::new(fs::File::open(archive_path)?)?;
    if archive.len() != delivery.files.len() || archive.len() > MAX_COMPONENT_FILES {
        bail!("local ASR archive has an unexpected entry count");
    }
    let mut seen = std::collections::HashSet::new();
    let mut expanded = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        if entry.is_dir() {
            bail!("local ASR archive contains an unexpected directory");
        }
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| anyhow!("local ASR archive contains an unsafe path"))?
            .to_path_buf();
        let expected = delivery
            .files
            .iter()
            .find(|file| Path::new(file.path) == relative)
            .ok_or_else(|| anyhow!("local ASR archive contains an unowned file"))?;
        if !seen.insert(relative.clone()) || entry.size() != expected.size_bytes {
            bail!("local ASR archive entry does not match its manifest");
        }
        expanded = expanded
            .checked_add(entry.size())
            .filter(|total| *total <= delivery.unpacked_size_bytes)
            .ok_or_else(|| anyhow!("local ASR archive expands beyond its limit"))?;
        let target = staging::prepare_target(staging_root, &relative)?;
        let mut output = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&target)?;
        std::io::copy(&mut entry, &mut output)?;
        output.flush()?;
        if !file_matches(&target, &owned_file(expected))? {
            bail!("extracted local ASR file failed integrity verification");
        }
        if expected.path.starts_with("bin/x64/") {
            validate_x64_pe(&target)?;
        }
    }
    if seen.len() != delivery.files.len() || expanded != delivery.unpacked_size_bytes {
        bail!("local ASR archive is incomplete");
    }
    Ok(())
}

#[cfg(debug_assertions)]
fn adopt_legacy_runtime(staging_root: &Path) -> Result<bool> {
    let legacy = crate::paths::app_local_data_dir().join("bin/x64");
    for expected in RUNTIME_FILES {
        let source = legacy.join(
            Path::new(expected.path)
                .file_name()
                .expect("runtime file has a name"),
        );
        if !source.is_file() || !file_matches(&source, &owned_file(expected))? {
            return Ok(false);
        }
    }
    for expected in RUNTIME_FILES {
        let source = legacy.join(
            Path::new(expected.path)
                .file_name()
                .expect("runtime file has a name"),
        );
        let target = staging::prepare_target(staging_root, Path::new(expected.path))?;
        fs::copy(source, &target)?;
        validate_x64_pe(&target)?;
    }
    Ok(true)
}

#[cfg(debug_assertions)]
fn install_source_packages(
    staging_root: &Path,
    cancelled: &std::sync::atomic::AtomicBool,
    on_progress: &impl Fn(u64, u64),
) -> Result<()> {
    let total_download: u64 = SOURCE_PACKAGES
        .iter()
        .map(|package| package.size_bytes)
        .sum();
    let mut completed = 0_u64;
    for (index, package) in SOURCE_PACKAGES.iter().enumerate() {
        let archive = staging_root.join(format!("source-{index}.nupkg"));
        download(
            package.url,
            package.size_bytes,
            &archive,
            cancelled,
            &|done, _| on_progress(completed + done, total_download),
        )?;
        verify_file(&archive, package.size_bytes, package.sha256)?;
        let mut zip = zip::ZipArchive::new(fs::File::open(&archive)?)?;
        for source in package.entries {
            let expected = RUNTIME_FILES
                .iter()
                .find(|file| file.path == source.output_path)
                .expect("source entry maps to runtime contract");
            let mut entry = zip
                .by_name(source.archive_path)
                .with_context(|| format!("source package is missing {}", source.archive_path))?;
            if entry.size() != expected.size_bytes {
                bail!("source package runtime entry has an unexpected size");
            }
            let target = staging::prepare_target(staging_root, Path::new(source.output_path))?;
            let mut output = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&target)?;
            std::io::copy(&mut entry, &mut output)?;
            output.flush()?;
            if !file_matches(&target, &owned_file(expected))? {
                bail!("source package runtime entry failed integrity verification");
            }
            if expected.path.starts_with("bin/x64/") {
                validate_x64_pe(&target)?;
            }
        }
        drop(zip);
        fs::remove_file(&archive)?;
        completed += package.size_bytes;
    }
    Ok(())
}

fn publish(
    staging_root: &Path,
    id: &str,
    version: &str,
    kind: ComponentKind,
    files: &[LocalAsrFile],
) -> Result<()> {
    super::super::write_receipt(staging_root, &receipt(kind, version, files))?;
    super::validate_exact_tree(staging_root, files)?;
    let target = version_root(id, version)?;
    let parent = super::super::ensure_component_parent(id)?;
    if target.parent() != Some(parent.as_path()) || target.exists() {
        bail!("local ASR component target is invalid or already exists");
    }
    fs::rename(staging_root, &target)?;
    super::super::validate_version_root(id, version)?;
    Ok(())
}

fn verify_file(path: &Path, size_bytes: u64, sha256: &str) -> Result<()> {
    let expected = OwnedComponentFile {
        path: "archive".into(),
        size_bytes,
        sha256: sha256.to_string(),
    };
    if !file_matches(path, &expected)? {
        bail!("local ASR archive checksum mismatch");
    }
    Ok(())
}

pub(super) fn validate_x64_pe(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || super::super::receipt::is_reparse_point(&metadata) {
        bail!("local ASR executable is not a regular file");
    }
    let mut file = fs::File::open(path)?;
    let mut dos = [0_u8; 64];
    file.read_exact(&mut dos)?;
    if &dos[..2] != b"MZ" {
        bail!("local ASR executable has no DOS signature");
    }
    let pe_offset = u32::from_le_bytes(dos[0x3c..0x40].try_into().expect("PE offset")) as u64;
    if pe_offset < 64
        || pe_offset
            .checked_add(6)
            .is_none_or(|end| end > metadata.len())
    {
        bail!("local ASR executable has an invalid PE offset");
    }
    file.seek(SeekFrom::Start(pe_offset))?;
    let mut pe = [0_u8; 6];
    file.read_exact(&mut pe)?;
    if &pe[..4] != b"PE\0\0" || u16::from_le_bytes([pe[4], pe[5]]) != IMAGE_FILE_MACHINE_AMD64 {
        bail!("local ASR executable is not an x64 PE file");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_contract_matches_runtime_inventory() {
        let mut outputs = SOURCE_PACKAGES
            .iter()
            .flat_map(|package| package.entries)
            .map(|entry| entry.output_path)
            .collect::<Vec<_>>();
        outputs.sort_unstable();
        let mut expected = RUNTIME_FILES
            .iter()
            .map(|file| file.path)
            .collect::<Vec<_>>();
        expected.sort_unstable();
        assert_eq!(outputs, expected);
        assert_eq!(super::super::ARCHITECTURE, "x64");
    }
}
