use std::io::{Read as _, Seek as _, Write as _};
use std::path::{Path, PathBuf};

use super::DeliveryRecord;

mod platform;

pub(super) struct ReceiptReservation {
    _file: std::fs::File,
    identity: String,
}

impl ReceiptReservation {
    pub(super) fn identity(&self) -> &str {
        &self.identity
    }

    pub(super) fn commit(self) -> Result<(), String> {
        Ok(())
    }
}

pub(super) struct PublishedArtifactGuard {
    _file: std::fs::File,
    artifact: crate::overlay::generation_history::DeliveryArtifactIdentity,
}

impl PublishedArtifactGuard {
    pub(super) fn artifact(&self) -> &crate::overlay::generation_history::DeliveryArtifactIdentity {
        &self.artifact
    }
}

pub(super) fn reserve_path(output_path: &Path, dispatch_id: &str) -> Result<PathBuf, String> {
    match std::fs::symlink_metadata(output_path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err("Creation output destination is unavailable.".to_string()),
        Ok(_) => return Err("Creation output destination is already in use.".to_string()),
    }
    let parent = output_path
        .parent()
        .ok_or_else(|| "Creation delivery destination is invalid.".to_string())?;
    for _ in 0..8 {
        let mut nonce = [0_u8; 16];
        getrandom::fill(&mut nonce)
            .map_err(|_| "Creation publication receipt is unavailable.".to_string())?;
        let nonce = nonce
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let path = parent.join(format!(".sgt-{dispatch_id}-{nonce}.publishing"));
        match std::fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(path),
            Ok(_) => {}
            Err(_) => return Err("Creation publication receipt is unavailable.".to_string()),
        }
    }
    Err("Creation publication receipt is unavailable.".to_string())
}

pub(super) fn new_claim() -> Result<String, String> {
    let mut claim = [0_u8; 32];
    getrandom::fill(&mut claim)
        .map_err(|_| "Creation publication receipt is unavailable.".to_string())?;
    Ok(claim.iter().map(|byte| format!("{byte:02x}")).collect())
}

pub(super) fn output_identity(path: &Path) -> Result<String, String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Creation delivery destination is invalid.".to_string())?;
    let name = path
        .file_name()
        .ok_or_else(|| "Creation delivery destination is invalid.".to_string())?;
    let parent = std::fs::canonicalize(parent)
        .map_err(|_| "Creation delivery destination is unavailable.".to_string())?;
    let identity = parent.join(name).to_string_lossy().to_string();
    #[cfg(windows)]
    {
        Ok(identity.to_ascii_lowercase())
    }
    #[cfg(not(windows))]
    {
        Ok(identity)
    }
}

pub(super) fn validate_paths(record: &DeliveryRecord) -> Result<(), String> {
    let output = Path::new(&record.output_path);
    let name = record.output_name.as_str();
    let identity = output_identity(output)?;
    let expected = output_identity(&crate::overlay::creation_output::assigned_path(
        output
            .parent()
            .ok_or_else(|| "Creation delivery destination is invalid.".to_string())?,
        name,
    )?)?;
    if identity != expected {
        return Err("Creation delivery destination is invalid.".to_string());
    }
    crate::overlay::creation_output::validate_staging_path(
        &record.dispatch_id,
        name,
        Path::new(&record.staging_path),
    )?;
    validate_publication_path(record)?;
    if record
        .publication_file_identity
        .as_deref()
        .is_some_and(|identity| !valid_file_identity(identity))
        || (record.stage != super::DeliveryStage::Validated
            && record.publication_file_identity.is_none())
    {
        return Err("Creation publication receipt identity is invalid.".to_string());
    }
    if record.publication_claim.len() != 64
        || !record
            .publication_claim
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("Creation publication receipt claim is invalid.".to_string());
    }
    Ok(())
}

fn valid_file_identity(identity: &str) -> bool {
    crate::overlay::creation_file_identity::valid(identity)
}

fn validate_publication_path(record: &DeliveryRecord) -> Result<(), String> {
    let output_parent = Path::new(&record.output_path)
        .parent()
        .ok_or_else(|| "Creation publication receipt is invalid.".to_string())?;
    let publication = Path::new(&record.publication_path);
    if publication.parent() != Some(output_parent) {
        return Err("Creation publication receipt is invalid.".to_string());
    }
    let name = publication
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "Creation publication receipt is invalid.".to_string())?;
    let prefix = format!(".sgt-{}-", record.dispatch_id);
    let nonce = name
        .strip_prefix(&prefix)
        .and_then(|value| value.strip_suffix(".publishing"))
        .ok_or_else(|| "Creation publication receipt is invalid.".to_string())?;
    if nonce.len() != 32 || !nonce.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Creation publication receipt is invalid.".to_string());
    }
    Ok(())
}

pub(super) fn prepare(
    record: &DeliveryRecord,
) -> Result<crate::overlay::generation_history::DeliveryArtifactIdentity, String> {
    validate_paths(record)?;
    let staging = validated_staging(record)?;
    let final_path = Path::new(&record.output_path);
    match std::fs::symlink_metadata(final_path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err("Creation output destination is unavailable.".to_string()),
        Ok(_) => return Err("Creation output destination is already in use.".to_string()),
    }
    let temporary = Path::new(&record.publication_path);
    let mut target = open_owned_receipt(record, true, false)?;
    if let Ok(copied) =
        crate::overlay::generation_history::inspect_delivery_artifact(&temporary.to_string_lossy())
        && copied.size_bytes == record.artifact_size_bytes
        && copied.sha256 == record.artifact_sha256
        && platform::identity(&target)? == saved_file_identity(record)?
    {
        return Ok(copied);
    }
    let mut source = std::fs::File::open(&record.staging_path)
        .map_err(|_| "Creation staging result is unavailable.".to_string())?;
    target
        .set_len(0)
        .and_then(|()| target.rewind())
        .map_err(|_| "Creation publication receipt is unavailable.".to_string())?;
    let copied = std::io::copy(
        &mut (&mut source).take(record.artifact_size_bytes.saturating_add(1)),
        &mut target,
    )
    .map_err(|_| "Creation result could not be published.".to_string())?;
    if copied != record.artifact_size_bytes {
        return Err("Creation staging result changed during publication.".to_string());
    }
    target
        .flush()
        .and_then(|()| target.sync_all())
        .map_err(|_| "Creation result could not be published.".to_string())?;
    drop(target);
    let copied = crate::overlay::generation_history::inspect_delivery_artifact(
        &temporary.to_string_lossy(),
    )?;
    if copied.size_bytes != record.artifact_size_bytes
        || copied.sha256 != record.artifact_sha256
        || file_identity_path(temporary)? != saved_file_identity(record)?
    {
        return Err("Creation publication copy could not be verified.".to_string());
    }
    Ok(staging)
}

pub(super) fn publish_prepared(record: &DeliveryRecord) -> Result<PublishedArtifactGuard, String> {
    validate_paths(record)?;
    validated_staging(record)?;
    let final_path = Path::new(&record.output_path);
    let temporary = Path::new(&record.publication_path);
    match std::fs::symlink_metadata(temporary) {
        Ok(metadata) if metadata.file_type().is_file() && !is_reparse_point(&metadata) => {
            let owned = open_owned_receipt(record, false, true)?;
            let copied = crate::overlay::generation_history::inspect_delivery_artifact(
                &temporary.to_string_lossy(),
            )?;
            if copied.size_bytes != record.artifact_size_bytes
                || copied.sha256 != record.artifact_sha256
            {
                return Err("Creation publication receipt changed unexpectedly.".to_string());
            }
            match std::fs::symlink_metadata(final_path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err("Creation output destination is unavailable.".to_string()),
                Ok(_) => {
                    return Err("Creation output destination is already in use.".to_string());
                }
            }
            platform::rename_no_replace(&owned, temporary, final_path)?;
            drop(owned);
            verify_published(record)
        }
        Ok(_) => Err("Creation publication receipt is invalid.".to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => verify_published(record),
        Err(_) => Err("Creation publication receipt is unavailable.".to_string()),
    }
}

fn validated_staging(
    record: &DeliveryRecord,
) -> Result<crate::overlay::generation_history::DeliveryArtifactIdentity, String> {
    let staging =
        crate::overlay::generation_history::inspect_delivery_artifact(&record.staging_path)?;
    if staging.size_bytes != record.artifact_size_bytes || staging.sha256 != record.artifact_sha256
    {
        return Err("Validated creation result changed before publication.".to_string());
    }
    Ok(staging)
}

pub(super) fn verify_published(record: &DeliveryRecord) -> Result<PublishedArtifactGuard, String> {
    let file = open_owned_output(record, false)?;
    let published =
        crate::overlay::generation_history::inspect_delivery_artifact(&record.output_path)?;
    if published.size_bytes != record.artifact_size_bytes
        || published.sha256 != record.artifact_sha256
        || file_identity_path(Path::new(&record.output_path))? != saved_file_identity(record)?
    {
        return Err("Published creation result could not be verified.".to_string());
    }
    Ok(PublishedArtifactGuard {
        _file: file,
        artifact: published,
    })
}

pub(super) fn cleanup_receipt(record: &DeliveryRecord) -> Result<(), String> {
    validate_publication_path(record)?;
    let path = Path::new(&record.publication_path);
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() && !is_reparse_point(&metadata) => {
            let owned = open_owned_receipt(record, false, true)?;
            platform::delete(&owned, path)
        }
        Ok(_) => Err("Creation publication cleanup refused an unsafe entry.".to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("Creation publication cleanup could not inspect its entry.".to_string()),
    }
}

pub(super) fn create_receipt(record: &DeliveryRecord) -> Result<ReceiptReservation, String> {
    validate_paths(record)?;
    let final_path = Path::new(&record.output_path);
    match std::fs::symlink_metadata(final_path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err("Creation output destination is unavailable.".to_string()),
        Ok(_) => return Err("Creation output destination is already in use.".to_string()),
    }
    let publication = Path::new(&record.publication_path);
    let mut file = match std::fs::symlink_metadata(publication) {
        Ok(metadata) if metadata.file_type().is_file() && !is_reparse_point(&metadata) => {
            let mut file = platform::open_locked(publication, false, false)?;
            verify_claim(&mut file, &record.publication_claim)?;
            file
        }
        Ok(_) => return Err("Creation publication receipt is invalid.".to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            cleanup_initializers(record)?;
            let initializer = initializer_path(record)?;
            let mut file = platform::create_locked(&initializer)?;
            file.write_all(claim_bytes(&record.publication_claim).as_bytes())
                .and_then(|()| file.flush())
                .and_then(|()| file.sync_all())
                .map_err(|_| "Creation publication receipt is unavailable.".to_string())?;
            if let Err(error) = platform::rename_no_replace(&file, &initializer, publication) {
                let _ = platform::delete(&file, &initializer);
                return Err(error);
            }
            file
        }
        Err(_) => return Err("Creation publication receipt is unavailable.".to_string()),
    };
    verify_claim(&mut file, &record.publication_claim)?;
    let identity = platform::identity(&file)?;
    Ok(ReceiptReservation {
        _file: file,
        identity,
    })
}

fn claim_bytes(claim: &str) -> String {
    format!("SGT-RECEIPT-1:{claim}\n")
}

fn verify_claim(file: &mut std::fs::File, claim: &str) -> Result<(), String> {
    file.rewind()
        .map_err(|_| "Creation publication receipt is unavailable.".to_string())?;
    let expected = claim_bytes(claim);
    let mut bytes = Vec::with_capacity(expected.len().saturating_add(1));
    file.take(expected.len() as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Creation publication receipt is unavailable.".to_string())?;
    if bytes != expected.as_bytes() {
        return Err("Creation publication receipt ownership is invalid.".to_string());
    }
    Ok(())
}

fn initializer_path(record: &DeliveryRecord) -> Result<PathBuf, String> {
    let parent = Path::new(&record.publication_path)
        .parent()
        .ok_or_else(|| "Creation publication receipt is invalid.".to_string())?;
    Ok(parent.join(format!(
        ".sgt-{}-receipt-{}.tmp",
        record.dispatch_id, record.publication_claim
    )))
}

fn cleanup_initializers(record: &DeliveryRecord) -> Result<(), String> {
    let path = initializer_path(record)?;
    match std::fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("Creation publication receipt is unavailable.".to_string()),
        Ok(metadata) if metadata.file_type().is_file() && !is_reparse_point(&metadata) => {
            let file = lock_owned_path(&path, None, false, true)?;
            platform::delete(&file, &path)
        }
        Ok(_) => Err("Creation publication receipt initializer is unsafe.".to_string()),
    }
}

pub(super) fn cancel_pre_publication(record: &DeliveryRecord) -> Result<(), String> {
    if !record.stage.is_pre_publication() {
        return Err("Creation publication can no longer be cancelled.".to_string());
    }
    let receipt = Path::new(&record.publication_path);
    match std::fs::symlink_metadata(receipt) {
        Ok(_) => {
            if record.stage == super::DeliveryStage::PublicationPrepared {
                let artifact = crate::overlay::generation_history::inspect_delivery_artifact(
                    &record.publication_path,
                )?;
                if artifact.size_bytes != record.artifact_size_bytes
                    || artifact.sha256 != record.artifact_sha256
                {
                    return Err("Creation publication receipt changed unexpectedly.".to_string());
                }
            }
            cleanup_receipt(record)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if record.stage == super::DeliveryStage::Validated {
                return Ok(());
            }
            match std::fs::symlink_metadata(&record.output_path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                Err(_) => {
                    return Err("Cancelled creation result is unavailable.".to_string());
                }
                Ok(_) => {}
            }
            let owned = open_owned_output(record, true)?;
            let published =
                crate::overlay::generation_history::inspect_delivery_artifact(&record.output_path)?;
            if published.size_bytes != record.artifact_size_bytes
                || published.sha256 != record.artifact_sha256
            {
                return Err("Published creation result could not be verified.".to_string());
            }
            platform::delete(&owned, Path::new(&record.output_path))
                .map_err(|_| "Cancelled creation result could not be removed.".to_string())
        }
        Err(_) => Err("Creation publication cleanup could not inspect its entry.".to_string()),
    }
}

fn open_owned_receipt(
    record: &DeliveryRecord,
    write: bool,
    delete: bool,
) -> Result<std::fs::File, String> {
    let path = Path::new(&record.publication_path);
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| "Creation publication receipt is unavailable.".to_string())?;
    if !metadata.file_type().is_file() || is_reparse_point(&metadata) {
        return Err("Creation publication receipt is invalid.".to_string());
    }
    let file = platform::open_locked(path, write, delete)?;
    if platform::identity(&file)? != saved_file_identity(record)? {
        return Err("Creation publication receipt ownership changed.".to_string());
    }
    Ok(file)
}

fn open_owned_output(record: &DeliveryRecord, delete: bool) -> Result<std::fs::File, String> {
    let path = Path::new(&record.output_path);
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| "Published creation result is unavailable.".to_string())?;
    if !metadata.file_type().is_file() || is_reparse_point(&metadata) {
        return Err("Published creation result is invalid.".to_string());
    }
    let file = platform::open_locked(path, false, delete)?;
    if platform::identity(&file)? != saved_file_identity(record)? {
        return Err("Published creation result ownership changed.".to_string());
    }
    Ok(file)
}

fn saved_file_identity(record: &DeliveryRecord) -> Result<&str, String> {
    record
        .publication_file_identity
        .as_deref()
        .ok_or_else(|| "Creation publication receipt ownership is missing.".to_string())
}

fn file_identity_path(path: &Path) -> Result<String, String> {
    let file = platform::open_locked(path, false, false)?;
    platform::identity(&file)
}

pub(crate) fn lock_owned_path(
    path: &Path,
    expected_identity: Option<&str>,
    write: bool,
    delete: bool,
) -> Result<std::fs::File, String> {
    let metadata =
        std::fs::symlink_metadata(path).map_err(|_| "Owned file is unavailable.".to_string())?;
    if !metadata.file_type().is_file() || is_reparse_point(&metadata) {
        return Err("Owned file is invalid.".to_string());
    }
    let file = platform::open_locked(path, write, delete)?;
    if let Some(expected) = expected_identity {
        let actual = platform::identity(&file)?;
        if actual != expected {
            return Err("Owned file identity changed.".to_string());
        }
    }
    Ok(file)
}

pub(crate) fn rename_owned_no_replace(
    file: &std::fs::File,
    source: &Path,
    destination: &Path,
) -> Result<(), String> {
    platform::rename_no_replace(file, source, destination)
}

pub(crate) fn delete_owned(file: &std::fs::File, path: &Path) -> Result<(), String> {
    platform::delete(file, path)
}

#[cfg(windows)]
fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt as _;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}
