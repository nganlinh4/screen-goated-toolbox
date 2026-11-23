use crate::component_registry::verified_archive::{self, FileSpec, Policy};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

use anyhow::{Result, anyhow, bail};

use super::super::persisted_download;
use super::super::receipt::{ComponentReceipt, OwnedComponentFile, file_matches};
use super::{
    ARCHITECTURE, COMPONENT_ID, MAX_COMPONENT_FILES, VcRuntimeDelivery, delivery, owned_file,
    staging, validate_exact_tree, validate_install,
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

pub(super) fn install(cancelled: &AtomicBool, on_progress: impl Fn(u64, u64)) -> Result<()> {
    let _guard = INSTALL_LOCK
        .lock()
        .unwrap_or_else(|value| value.into_inner());
    let delivery = delivery()?;
    if validate_install(delivery).is_ok() {
        return Ok(());
    }
    let mut install_lease = super::super::acquire(COMPONENT_ID)?;
    ensure_not_cancelled(cancelled)?;
    if adopt_legacy(delivery, cancelled, &mut install_lease)? {
        return Ok(());
    }
    install_delivery(delivery, cancelled, on_progress, &mut install_lease)
}

fn install_delivery(
    delivery: &VcRuntimeDelivery,
    cancelled: &AtomicBool,
    on_progress: impl Fn(u64, u64),
    install_lease: &mut super::super::ComponentLease,
) -> Result<()> {
    let sequence = INSTALL_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let working_root = crate::paths::app_runtime_local_data_dir();
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
        let archive_path = persisted_download::fetch(
            COMPONENT_ID,
            &pinned_body(delivery),
            cancelled,
            &on_progress,
        )
        .map_err(|error| anyhow!("VC runtime download failed: {error:#}"))?;
        validate_archive(&archive_path, delivery)?;
        extract_archive(&archive_path, &staging, delivery, cancelled)?;
        ensure_not_cancelled(cancelled)?;
        finish_staging(&staging, delivery, install_lease)?;
        // A failed install keeps the received archive for the next attempt.
        persisted_download::discard_leftovers(COMPONENT_ID);
        Ok(())
    })();
    if staging.exists() {
        let mut owned_paths = delivery
            .files
            .iter()
            .map(|file| file.path)
            .collect::<Vec<_>>();
        owned_paths.push(super::super::receipt::RECEIPT_NAME);
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

/// Transfers the archive of a fresh install while holding only this
/// component's transfer guard, so the caller runs it before the registry
/// mutation guard. Installs that can adopt the legacy runtime skip it.
pub(super) fn prefetch(
    delivery: &VcRuntimeDelivery,
    cancelled: &AtomicBool,
    on_progress: &impl Fn(u64, u64),
) -> Result<()> {
    if validate_install(delivery).is_ok()
        || (crate::paths::runtime_state_root().is_none() && legacy_bin_dir().is_dir())
    {
        return Ok(());
    }
    persisted_download::prefetch(
        COMPONENT_ID,
        &[pinned_body(delivery)],
        cancelled,
        on_progress,
    )
    .map_err(|error| anyhow!("VC runtime download failed: {error:#}"))
}

fn pinned_body(delivery: &VcRuntimeDelivery) -> persisted_download::PinnedBody<'_> {
    persisted_download::PinnedBody {
        url: delivery.download_url,
        size: delivery.size_bytes,
        sha256: delivery.sha256,
    }
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

fn extract_archive(
    path: &Path,
    staging: &Path,
    delivery: &VcRuntimeDelivery,
    cancelled: &AtomicBool,
) -> Result<()> {
    let files = delivery
        .files
        .iter()
        .map(|file| FileSpec::same(Path::new(file.path), file.size_bytes, file.sha256))
        .collect::<Vec<_>>();
    verified_archive::extract(
        std::fs::File::open(path)?,
        &files,
        Policy::files(MAX_COMPONENT_FILES, delivery.unpacked_size_bytes),
        cancelled,
        |index, member| {
            let expected = &delivery.files[index];
            let (target, _directories) =
                staging::prepare_target(staging, Path::new(expected.path))?;
            let mut output = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&target)?;
            member.copy_to(&mut output, &|_| {})?;
            output.flush()?;
            drop(output);
            if !file_matches(&target, &owned_file(expected))? {
                bail!("extracted component file failed integrity verification");
            }
            if target
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("dll"))
            {
                staging::validate_x64_pe(&target)?;
            }
            Ok(())
        },
    )
}

fn ensure_not_cancelled(cancelled: &AtomicBool) -> Result<()> {
    if cancelled.load(Ordering::Acquire) {
        bail!("VC runtime installation was cancelled");
    }
    Ok(())
}

fn legacy_bin_dir() -> PathBuf {
    crate::paths::app_runtime_local_data_dir()
        .join("bin")
        .join("x64")
}

fn adopt_legacy(
    delivery: &VcRuntimeDelivery,
    cancelled: &AtomicBool,
    install_lease: &mut super::super::ComponentLease,
) -> Result<bool> {
    if crate::paths::runtime_state_root().is_some() {
        return Ok(false);
    }
    let legacy = legacy_bin_dir();
    if !legacy.is_dir() {
        return Ok(false);
    }
    require_regular_directory(&legacy)?;
    let sequence = INSTALL_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let working_root = crate::paths::app_runtime_local_data_dir();
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
            ensure_not_cancelled(cancelled)?;
            finish_staging(&staging, delivery, install_lease)?;
            cleanup_verified_legacy(&legacy, delivery);
            Ok(true)
        }
        Ok(false) => {
            let mut owned = delivery
                .files
                .iter()
                .map(|file| file.path)
                .collect::<Vec<_>>();
            owned.push(super::super::receipt::RECEIPT_NAME);
            let _ = staging::cleanup_owned(&staging, &owned);
            Ok(false)
        }
        Err(error) => {
            let mut owned = delivery
                .files
                .iter()
                .map(|file| file.path)
                .collect::<Vec<_>>();
            owned.push(super::super::receipt::RECEIPT_NAME);
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
        let (target, _directories) = staging::prepare_target(staging_root, Path::new(file.path))?;
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
    if crate::paths::runtime_state_root().is_some() {
        return;
    }
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

fn finish_staging(
    staging_root: &Path,
    delivery: &VcRuntimeDelivery,
    install_lease: &mut super::super::ComponentLease,
) -> Result<()> {
    super::super::write_receipt(staging_root, &receipt(delivery))?;
    validate_exact_tree(staging_root, delivery.files)?;
    super::super::publication::publish_verified(
        COMPONENT_ID,
        delivery.version,
        &receipt(delivery).files,
        staging_root,
        install_lease,
    )?;
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

#[cfg(test)]
mod tests {
    use super::super::VcRuntimeFile;
    use super::*;

    #[test]
    fn archive_extraction_finishes_writing_before_native_inspection() {
        use sha2::{Digest as _, Sha256};
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "sgt-native-archive-{}-{unique}",
            std::process::id()
        ));
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        let mut bytes = vec![0_u8; 70];
        bytes[..2].copy_from_slice(b"MZ");
        bytes[60..64].copy_from_slice(&64_u32.to_le_bytes());
        bytes[64..68].copy_from_slice(b"PE\0\0");
        bytes[68..70].copy_from_slice(&0x8664_u16.to_le_bytes());
        let files = [VcRuntimeFile {
            path: "bin/x64/runtime.dll",
            size_bytes: bytes.len() as u64,
            sha256: Box::leak(format!("{:x}", Sha256::digest(&bytes)).into_boxed_str()),
        }];
        let delivery = VcRuntimeDelivery {
            version: "test",
            asset: "fixture.zip",
            download_url: "https://example.invalid/archive.zip",
            size_bytes: 1,
            sha256: "0",
            unpacked_size_bytes: bytes.len() as u64,
            files: Box::leak(Box::new(files)),
        };
        let archive_path = root.join("fixture.zip");
        let mut archive = zip::ZipWriter::new(std::fs::File::create(&archive_path).unwrap());
        archive
            .start_file(
                "bin/x64/runtime.dll",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        archive.write_all(&bytes).unwrap();
        drop(archive.finish().unwrap());
        extract_archive(&archive_path, &stage, &delivery, &AtomicBool::new(false)).unwrap();
        assert_eq!(
            std::fs::read(stage.join("bin/x64/runtime.dll")).unwrap(),
            bytes
        );
        std::fs::remove_file(stage.join("bin/x64/runtime.dll")).unwrap();
        std::fs::remove_dir(stage.join("bin/x64")).unwrap();
        std::fs::remove_dir(stage.join("bin")).unwrap();
        std::fs::remove_dir(stage).unwrap();
        std::fs::remove_file(archive_path).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
