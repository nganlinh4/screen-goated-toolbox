use std::collections::HashSet;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

use anyhow::{Result, anyhow, bail};

use super::super::receipt::{ComponentReceipt, OwnedComponentFile, file_matches};
use super::{
    ARCHITECTURE, COMPONENT_ID, MAX_COMPONENT_FILES, VcRuntimeDelivery, delivery, owned_file,
    staging, validate_exact_tree, validate_install, version_root,
};

static INSTALL_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static INSTALL_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
const EMBEDDED_NOTICES: &[(&str, &[u8])] = &[
    (
        "licenses/REDIST.txt",
        include_bytes!("../../../component-notices/vc14-x64-runtime/REDIST.txt"),
    ),
    (
        "licenses/THIRD-PARTY-NOTICES.txt",
        include_bytes!("../../../component-notices/vc14-x64-runtime/THIRD-PARTY-NOTICES.txt"),
    ),
];

pub(super) fn install(on_progress: impl Fn(u64, u64)) -> Result<()> {
    let _guard = INSTALL_LOCK
        .lock()
        .unwrap_or_else(|value| value.into_inner());
    let delivery = delivery()?;
    if validate_install(delivery).is_ok() {
        return Ok(());
    }
    clear_invalid_install()?;
    let _install_lease = super::super::acquire(COMPONENT_ID)?;
    if adopt_legacy(delivery)? {
        return Ok(());
    }
    install_delivery(delivery, on_progress)
}

fn clear_invalid_install() -> Result<()> {
    match super::super::request_remove(COMPONENT_ID)? {
        super::super::RemovalOutcome::Missing | super::super::RemovalOutcome::Removed => Ok(()),
        super::super::RemovalOutcome::Pending => bail!("VC runtime support is currently in use"),
        super::super::RemovalOutcome::RequiredBy(dependents) => bail!(
            "VC runtime support cannot be repaired while required by installed components: {}",
            dependents.join(", ")
        ),
        super::super::RemovalOutcome::PreservedModified(paths) => bail!(
            "VC runtime support cannot be repaired because {} modified managed file(s) were preserved",
            paths.len()
        ),
    }
}

fn install_delivery(delivery: &VcRuntimeDelivery, on_progress: impl Fn(u64, u64)) -> Result<()> {
    let sequence = INSTALL_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let working_root = crate::paths::app_local_data_dir();
    let scratch = working_root.join("component-downloads");
    ensure_regular_directory(&working_root, &scratch)?;
    let archive_path = scratch.join(format!(
        "{COMPONENT_ID}-{}-{sequence}.download",
        std::process::id()
    ));
    let staging_parent = working_root.join("component-staging");
    ensure_regular_directory(&working_root, &staging_parent)?;
    let staging = staging_parent.join(format!(
        "{COMPONENT_ID}-{}-{}-{sequence}",
        delivery.version,
        std::process::id()
    ));
    std::fs::create_dir(&staging)?;
    require_regular_directory(&staging)?;

    let result = (|| {
        download_archive(delivery, &archive_path, on_progress)?;
        validate_archive(&archive_path, delivery)?;
        extract_archive(&archive_path, &staging, delivery)?;
        finish_staging(&staging, delivery)
    })();
    let _ = std::fs::remove_file(&archive_path);
    if staging.exists() {
        let owned_paths = delivery
            .files
            .iter()
            .map(|file| file.path)
            .collect::<Vec<_>>();
        let _ = staging::cleanup_owned(&staging, &owned_paths);
    }
    result
}

fn ensure_regular_directory(root: &Path, target: &Path) -> Result<()> {
    std::fs::create_dir_all(root)?;
    require_regular_directory(root)?;
    let relative = target
        .strip_prefix(root)
        .map_err(|_| anyhow!("VC runtime working directory escaped its root"))?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let std::path::Component::Normal(name) = component else {
            bail!("VC runtime working directory is unsafe");
        };
        current.push(name);
        match std::fs::create_dir(&current) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
        require_regular_directory(&current)?;
    }
    Ok(())
}

fn download_archive(
    delivery: &VcRuntimeDelivery,
    target: &Path,
    on_progress: impl Fn(u64, u64),
) -> Result<()> {
    let response = crate::api::client::UREQ_DOWNLOAD_AGENT
        .get(delivery.download_url)
        .header("User-Agent", "ScreenGoatedToolbox")
        .call()
        .map_err(|error| anyhow!("VC runtime download failed: {error}"))?;
    if response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|size| size != delivery.size_bytes)
    {
        bail!("VC runtime download size does not match this build");
    }
    let mut reader = response.into_body().into_reader();
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)?;
    let mut buffer = [0_u8; 128 * 1024];
    let mut downloaded = 0_u64;
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        downloaded = downloaded
            .checked_add(read as u64)
            .ok_or_else(|| anyhow!("VC runtime download is too large"))?;
        if downloaded > delivery.size_bytes {
            bail!("VC runtime download is larger than this build allows");
        }
        output.write_all(&buffer[..read])?;
        on_progress(downloaded, delivery.size_bytes);
    }
    if downloaded != delivery.size_bytes {
        bail!("VC runtime download is incomplete");
    }
    output.flush()?;
    output.sync_all()?;
    Ok(())
}

fn validate_archive(path: &Path, delivery: &VcRuntimeDelivery) -> Result<()> {
    let owned = OwnedComponentFile {
        path: PathBuf::from(delivery.asset),
        size_bytes: delivery.size_bytes,
        sha256: delivery.sha256.to_string(),
    };
    if !file_matches(path, &owned)? {
        bail!("VC runtime archive checksum mismatch");
    }
    Ok(())
}

fn extract_archive(path: &Path, staging: &Path, delivery: &VcRuntimeDelivery) -> Result<()> {
    let file = std::fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    if archive.len() != delivery.files.len() || archive.len() > MAX_COMPONENT_FILES {
        bail!("VC runtime archive has an unexpected entry count");
    }
    let mut extracted = HashSet::new();
    let mut extracted_bytes = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        if entry.is_dir() {
            bail!("VC runtime archive contains an unexpected directory entry");
        }
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| anyhow!("VC runtime archive contains an unsafe path"))?
            .to_path_buf();
        let expected = delivery
            .files
            .iter()
            .find(|file| Path::new(file.path) == relative)
            .ok_or_else(|| anyhow!("VC runtime archive contains an unowned file"))?;
        if !extracted.insert(relative.clone()) || entry.size() != expected.size_bytes {
            bail!("VC runtime archive entry does not match its manifest");
        }
        extracted_bytes = extracted_bytes
            .checked_add(entry.size())
            .ok_or_else(|| anyhow!("VC runtime archive expands beyond its limit"))?;
        if extracted_bytes > delivery.unpacked_size_bytes {
            bail!("VC runtime archive expands beyond its declared size");
        }
        let target = staging::prepare_target(staging, &relative)?;
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)?;
        std::io::copy(&mut entry, &mut output)?;
        output.flush()?;
        if !file_matches(&target, &owned_file(expected))? {
            bail!("extracted VC runtime file failed integrity verification");
        }
        if relative
            .extension()
            .is_some_and(|extension| extension.to_string_lossy().eq_ignore_ascii_case("dll"))
        {
            staging::validate_x64_pe(&target)?;
        }
    }
    if extracted.len() != delivery.files.len() || extracted_bytes != delivery.unpacked_size_bytes {
        bail!("VC runtime archive is incomplete");
    }
    Ok(())
}

fn legacy_bin_dir() -> PathBuf {
    crate::paths::app_runtime_local_data_dir()
        .join("bin")
        .join("x64")
}

fn adopt_legacy(delivery: &VcRuntimeDelivery) -> Result<bool> {
    let legacy = legacy_bin_dir();
    if !legacy.is_dir() {
        return Ok(false);
    }
    require_regular_directory(&legacy)?;
    let sequence = INSTALL_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let working_root = crate::paths::app_local_data_dir();
    let staging_parent = working_root.join("component-staging");
    ensure_regular_directory(&working_root, &staging_parent)?;
    let staging = staging_parent.join(format!(
        "{COMPONENT_ID}-adopt-{}-{sequence}",
        std::process::id()
    ));
    std::fs::create_dir(&staging)?;
    require_regular_directory(&staging)?;
    let result = adopt_from(&legacy, &staging, delivery);
    match result {
        Ok(true) => {
            finish_staging(&staging, delivery)?;
            cleanup_verified_legacy(&legacy, delivery);
            Ok(true)
        }
        Ok(false) => {
            let owned = delivery
                .files
                .iter()
                .map(|file| file.path)
                .collect::<Vec<_>>();
            let _ = staging::cleanup_owned(&staging, &owned);
            Ok(false)
        }
        Err(error) => {
            let owned = delivery
                .files
                .iter()
                .map(|file| file.path)
                .collect::<Vec<_>>();
            let _ = staging::cleanup_owned(&staging, &owned);
            Err(error)
        }
    }
}

fn adopt_from(legacy: &Path, staging_root: &Path, delivery: &VcRuntimeDelivery) -> Result<bool> {
    for file in delivery.files {
        if let Some(name) = Path::new(file.path)
            .strip_prefix("bin/x64")
            .ok()
            .and_then(Path::file_name)
        {
            let source = legacy.join(name);
            if !file_matches(&source, &owned_file(file)).unwrap_or(false) {
                return Ok(false);
            }
            staging::validate_x64_pe(&source)?;
        } else if embedded_notice(file.path).is_none() {
            return Ok(false);
        }
    }
    for file in delivery.files {
        let target = staging::prepare_target(staging_root, Path::new(file.path))?;
        if let Some(name) = Path::new(file.path)
            .strip_prefix("bin/x64")
            .ok()
            .and_then(Path::file_name)
        {
            let source = legacy.join(name);
            std::fs::hard_link(&source, &target).or_else(|_| {
                std::fs::copy(&source, &target)?;
                Ok::<(), std::io::Error>(())
            })?;
        } else {
            std::fs::write(
                &target,
                embedded_notice(file.path).expect("notice path validated above"),
            )?;
        }
        if !file_matches(&target, &owned_file(file))? {
            bail!("adopted VC runtime file changed during staging");
        }
    }
    Ok(true)
}

fn embedded_notice(path: &str) -> Option<&'static [u8]> {
    EMBEDDED_NOTICES
        .iter()
        .find_map(|(candidate, bytes)| (*candidate == path).then_some(*bytes))
}

fn cleanup_verified_legacy(legacy: &Path, delivery: &VcRuntimeDelivery) {
    for file in delivery.files {
        let Ok(relative) = Path::new(file.path).strip_prefix("bin/x64") else {
            continue;
        };
        let Some(name) = relative.file_name() else {
            continue;
        };
        let path = legacy.join(name);
        if file_matches(&path, &owned_file(file)).unwrap_or(false)
            && staging::validate_x64_pe(&path).is_ok()
        {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn finish_staging(staging_root: &Path, delivery: &VcRuntimeDelivery) -> Result<()> {
    super::super::write_receipt(staging_root, &receipt(delivery))?;
    validate_exact_tree(staging_root, delivery.files)?;
    let target = version_root(delivery)?;
    let parent = super::super::ensure_component_parent(COMPONENT_ID)?;
    if target.parent() != Some(parent.as_path()) {
        bail!("VC runtime install target escaped its component directory");
    }
    if target.exists() {
        bail!("VC runtime install target already exists");
    }
    std::fs::rename(staging_root, &target)?;
    super::super::validate_version_root(COMPONENT_ID, delivery.version)?;
    validate_install(delivery)
}

#[cfg(all(test, not(feature = "recorder-worker")))]
pub(super) fn adopt_from_for_test(
    legacy: &Path,
    staging_root: &Path,
    delivery: &VcRuntimeDelivery,
) -> Result<bool> {
    adopt_from(legacy, staging_root, delivery)
}

fn receipt(delivery: &VcRuntimeDelivery) -> ComponentReceipt {
    ComponentReceipt {
        schema_version: 1,
        id: COMPONENT_ID.to_string(),
        version: delivery.version.to_string(),
        architecture: ARCHITECTURE.to_string(),
        dependencies: Vec::new(),
        files: delivery.files.iter().map(owned_file).collect(),
    }
}

fn require_regular_directory(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_dir() || super::super::receipt::is_reparse_point(&metadata) {
        bail!("VC runtime working directory is unsafe");
    }
    Ok(())
}
