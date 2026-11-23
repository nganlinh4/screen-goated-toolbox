pub(super) use super::super::native_file::validate_x64_pe;
use crate::component_registry::verified_archive::{self, FileSpec, Policy};
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

use super::super::persisted_download;
use super::super::receipt::{OwnedComponentFile, file_matches};
use super::{
    LocalAsrDelivery, LocalAsrFile, MAX_COMPONENT_FILES, owned_file, receipt, staging,
    validate_delivery,
};
use anyhow::{Result, anyhow, bail};

static INSTALL_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
static INSTALL_SEQUENCE: AtomicU64 = AtomicU64::new(0);

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
    let mut lease = super::super::acquire(delivery.id)?;
    install_delivery(delivery, cancelled, on_progress, &mut lease)
}

fn install_delivery(
    delivery: &LocalAsrDelivery,
    cancelled: &std::sync::atomic::AtomicBool,
    on_progress: impl Fn(u64, u64),
    lease: &mut super::super::ComponentLease,
) -> Result<()> {
    let staging_root = staging_path(delivery.id, delivery.version)?;
    let result = (|| {
        let archive =
            persisted_download::fetch(delivery.id, &pinned_body(delivery), cancelled, &on_progress)
                .map_err(|error| anyhow!("local ASR component download failed: {error:#}"))?;
        verify_file(&archive, delivery.size_bytes, delivery.sha256)?;
        extract_delivery(&archive, &staging_root, delivery, cancelled)?;
        if cancelled.load(std::sync::atomic::Ordering::Acquire) {
            bail!("local ASR installation was cancelled");
        }
        publish(
            &staging_root,
            delivery.id,
            delivery.version,
            delivery.files,
            lease,
        )?;
        validate_delivery(delivery)?;
        // A failed install keeps the received archive for the next attempt.
        persisted_download::discard_leftovers(delivery.id);
        Ok(())
    })();
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

fn staging_path(id: &str, version: &str) -> Result<PathBuf> {
    let sequence = INSTALL_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let working_root = crate::paths::app_runtime_local_data_dir();
    let staging_parent = working_root.join("component-staging");
    staging::ensure_directory_tree(&working_root, &staging_parent)?;
    let staging_root =
        staging_parent.join(format!("{id}-{version}-{}-{sequence}", std::process::id()));
    fs::create_dir(&staging_root)?;
    Ok(staging_root)
}

/// Transfers the archive of a delivery that is not installed yet while holding
/// only its per-component transfer guard, so callers run it before taking the
/// registry mutation guard.
pub(super) fn prefetch(
    delivery: &LocalAsrDelivery,
    cancelled: &AtomicBool,
    on_progress: &impl Fn(u64, u64),
) -> Result<()> {
    if validate_delivery(delivery).is_ok() {
        return Ok(());
    }
    persisted_download::prefetch(
        delivery.id,
        &[pinned_body(delivery)],
        cancelled,
        on_progress,
    )
    .map_err(|error| anyhow!("local ASR component download failed: {error:#}"))
}

fn pinned_body(delivery: &LocalAsrDelivery) -> persisted_download::PinnedBody<'_> {
    persisted_download::PinnedBody {
        url: delivery.download_url,
        size: delivery.size_bytes,
        sha256: delivery.sha256,
    }
}

fn extract_delivery(
    archive_path: &Path,
    staging_root: &Path,
    delivery: &LocalAsrDelivery,
    cancelled: &AtomicBool,
) -> Result<()> {
    let files = delivery
        .files
        .iter()
        .map(|file| FileSpec::same(Path::new(file.path), file.size_bytes, file.sha256))
        .collect::<Vec<_>>();
    verified_archive::extract(
        std::fs::File::open(archive_path)?,
        &files,
        Policy::files(MAX_COMPONENT_FILES, delivery.unpacked_size_bytes),
        cancelled,
        |index, member| {
            let expected = &delivery.files[index];
            let (target, _directories) =
                staging::prepare_target(staging_root, Path::new(expected.path))?;
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
            if expected.path.starts_with("bin/x64/") {
                validate_x64_pe(&target)?;
            }
            Ok(())
        },
    )
}

fn publish(
    staging_root: &Path,
    id: &str,
    version: &str,
    files: &[LocalAsrFile],
    lease: &mut super::super::ComponentLease,
) -> Result<()> {
    let receipt = receipt(id, version, files)?;
    super::super::write_receipt(staging_root, &receipt)?;
    super::validate_exact_tree(staging_root, files)?;
    super::super::publication::publish_verified(id, version, &receipt.files, staging_root, lease)?;
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

#[cfg(test)]
mod tests {
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
        fs::create_dir_all(&stage).unwrap();
        let mut bytes = vec![0_u8; 70];
        bytes[..2].copy_from_slice(b"MZ");
        bytes[60..64].copy_from_slice(&64_u32.to_le_bytes());
        bytes[64..68].copy_from_slice(b"PE\0\0");
        bytes[68..70].copy_from_slice(&0x8664_u16.to_le_bytes());
        let files = [LocalAsrFile {
            path: "bin/x64/worker.dll",
            size_bytes: bytes.len() as u64,
            sha256: Box::leak(format!("{:x}", Sha256::digest(&bytes)).into_boxed_str()),
        }];
        let delivery = LocalAsrDelivery {
            id: "native-runtime",
            version: "test",
            download_url: "https://example.invalid/archive.zip",
            size_bytes: 1,
            sha256: "0",
            unpacked_size_bytes: bytes.len() as u64,
            files: Box::leak(Box::new(files)),
        };
        let archive_path = root.join("fixture.zip");
        let mut archive = zip::ZipWriter::new(fs::File::create(&archive_path).unwrap());
        archive
            .start_file(
                "bin/x64/worker.dll",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        archive.write_all(&bytes).unwrap();
        drop(archive.finish().unwrap());
        extract_delivery(&archive_path, &stage, &delivery, &AtomicBool::new(false)).unwrap();
        assert_eq!(fs::read(stage.join("bin/x64/worker.dll")).unwrap(), bytes);
        fs::remove_file(stage.join("bin/x64/worker.dll")).unwrap();
        fs::remove_dir(stage.join("bin/x64")).unwrap();
        fs::remove_dir(stage.join("bin")).unwrap();
        fs::remove_dir(stage).unwrap();
        fs::remove_file(archive_path).unwrap();
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn rejects_archive_links_even_when_their_names_are_owned() {
        use sha2::{Digest as _, Sha256};
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "sgt-archive-link-test-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir(&root).unwrap();
        let archive_path = root.join("fixture.zip");
        let mut writer = zip::ZipWriter::new(std::fs::File::create(&archive_path).unwrap());
        writer
            .add_symlink(
                "bin/x64/onnxruntime.dll",
                "outside",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        writer.finish().unwrap();
        let files = [LocalAsrFile {
            path: "bin/x64/onnxruntime.dll",
            size_bytes: 7,
            sha256: Box::leak(format!("{:x}", Sha256::digest(b"outside")).into_boxed_str()),
        }];
        let delivery = LocalAsrDelivery {
            id: "onnx-directml-runtime",
            version: "test",
            download_url: "https://example.invalid",
            size_bytes: 1,
            sha256: "0",
            unpacked_size_bytes: 7,
            files: Box::leak(Box::new(files)),
        };
        let staging = root.join("staging");
        std::fs::create_dir_all(&staging).unwrap();
        let error = extract_delivery(&archive_path, &staging, &delivery, &AtomicBool::new(false))
            .unwrap_err();
        assert!(error.to_string().contains("non-regular"), "{error}");
        assert!(!staging.join("bin/x64/onnxruntime.dll").exists());
        std::fs::remove_dir(staging).unwrap();
        std::fs::remove_file(archive_path).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
