use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

use super::super::receipt::{ComponentReceipt, RECEIPT_NAME, file_matches, is_reparse_point};
use super::{MAX_MODEL_FILES, ModelDelivery, RemovalOutcome, owned_file, staging};

const MAX_SCRATCH_ENTRIES: usize = 4_096;

pub(super) fn staging_root(delivery: &ModelDelivery) -> Result<PathBuf> {
    let runtime_root = super::state_root();
    Ok(runtime_root
        .join("component-staging")
        .join(format!("{}-{}", delivery.id, delivery.version)))
}

pub(super) fn cleanup_stale_downloads(
    id: &str,
    _mutation: &super::super::RegistryMutationGuard,
) -> Result<()> {
    for path in owned_downloads(id)? {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::NotFound
                ) => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

pub(super) fn finish_removal(
    id: &str,
    outcome: RemovalOutcome,
    _mutation: &super::super::RegistryMutationGuard,
) -> Result<RemovalOutcome> {
    if matches!(
        outcome,
        RemovalOutcome::RequiredBy(_) | RemovalOutcome::Pending
    ) {
        return Ok(outcome);
    }
    let Some(delivery) = super::catalog()
        .models
        .iter()
        .find(|delivery| delivery.id == id)
    else {
        return Ok(outcome);
    };
    let (staging_existed, mut auxiliary_preserved) = remove_staging(delivery)?;
    let downloads = owned_downloads(id)?;
    let downloads_existed = !downloads.is_empty();
    for path in downloads {
        if let Err(error) = std::fs::remove_file(&path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            auxiliary_preserved.push(path);
        }
    }
    let primary_removed = matches!(&outcome, RemovalOutcome::Removed);
    let mut preserved = match outcome {
        RemovalOutcome::PreservedModified(paths) => paths,
        _ => Vec::new(),
    };
    preserved.extend(auxiliary_preserved);
    preserved.sort();
    preserved.dedup();
    if !preserved.is_empty() {
        return Ok(RemovalOutcome::PreservedModified(preserved));
    }
    if staging_existed || downloads_existed || primary_removed {
        Ok(RemovalOutcome::Removed)
    } else {
        Ok(RemovalOutcome::Missing)
    }
}

fn remove_staging(delivery: &ModelDelivery) -> Result<(bool, Vec<PathBuf>)> {
    let root = staging_root(delivery)?;
    let metadata = match std::fs::symlink_metadata(&root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok((false, Vec::new()));
        }
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        return Ok((true, vec![root]));
    }
    let mut actual = Vec::new();
    staging::collect_regular_files(&root, &root, &mut actual, MAX_MODEL_FILES + 1)?;
    let mut preserved = Vec::new();
    for relative in actual {
        let path = super::super::receipt::resolve_owned_path(&root, &relative)?;
        let removable = if relative == Path::new(RECEIPT_NAME) {
            ComponentReceipt::read(&path).is_ok_and(|receipt| receipt_matches(delivery, &receipt))
        } else {
            delivery
                .files
                .iter()
                .find(|file| file.path == relative)
                .is_some_and(|file| file_matches(&path, &owned_file(file)).unwrap_or(false))
        };
        if removable {
            std::fs::remove_file(&path)?;
            staging::remove_empty_parents(path.parent(), &root)?;
        } else {
            preserved.push(path);
        }
    }
    match std::fs::remove_dir(&root) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::DirectoryNotEmpty => {
            for entry in std::fs::read_dir(&root)? {
                let path = entry?.path();
                if !preserved.contains(&path) {
                    preserved.push(path);
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok((true, preserved))
}

fn receipt_matches(delivery: &ModelDelivery, receipt: &ComponentReceipt) -> bool {
    receipt.schema_version == 1
        && receipt.id == delivery.id
        && receipt.version == delivery.version
        && receipt.architecture == super::ARCHITECTURE
        && receipt.dependencies.is_empty()
        && receipt.files.len() == delivery.files.len()
        && receipt
            .files
            .iter()
            .zip(&delivery.files)
            .all(|(owned, file)| {
                owned.path == file.path
                    && owned.size_bytes == file.size_bytes
                    && owned.sha256.eq_ignore_ascii_case(&file.sha256)
            })
}

fn owned_downloads(id: &str) -> Result<Vec<PathBuf>> {
    let root = super::state_root().join("component-downloads");
    let metadata = match std::fs::symlink_metadata(&root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        bail!("model download scratch root is unsafe");
    }
    let mut owned = Vec::new();
    for (index, entry) in std::fs::read_dir(root)?.enumerate() {
        if index >= MAX_SCRATCH_ENTRIES {
            bail!("model download scratch contains too many entries");
        }
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !is_owned_download_name(name, id) {
            continue;
        }
        let metadata = std::fs::symlink_metadata(&path)?;
        if !metadata.is_file() || is_reparse_point(&metadata) {
            continue;
        }
        owned.push(path);
    }
    Ok(owned)
}

fn is_owned_download_name(name: &str, id: &str) -> bool {
    let Some(rest) = name
        .strip_prefix(id)
        .and_then(|rest| rest.strip_prefix('-'))
    else {
        return false;
    };
    let Some((stem, extension)) = rest.rsplit_once('.') else {
        return false;
    };
    if !matches!(extension, "download" | "entry") {
        return false;
    }
    let mut fields = stem.split('-');
    matches!(
        (fields.next(), fields.next(), fields.next()),
        (Some(pid), Some(sequence), None)
            if !pid.is_empty()
                && !sequence.is_empty()
                && pid.bytes().all(|byte| byte.is_ascii_digit())
                && sequence.bytes().all(|byte| byte.is_ascii_digit())
    )
}

#[cfg(all(test, not(feature = "recorder-worker")))]
pub(super) fn owned_download_name_for_test(name: &str, id: &str) -> bool {
    is_owned_download_name(name, id)
}

#[cfg(all(test, not(feature = "recorder-worker")))]
pub(super) fn remove_staging_for_test(delivery: &ModelDelivery) -> Result<(bool, Vec<PathBuf>)> {
    remove_staging(delivery)
}
