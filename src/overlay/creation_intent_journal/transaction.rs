//! Durable intent for replacing a journal while retaining its prior version.

use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use super::{MAX_FILE_BYTES, RecordError, TEMP_SEQUENCE, storage};

const MAX_MARKER_BYTES: u64 = 16 * 1024;
const UNAVAILABLE: &str = "Creation recovery state requires reconciliation.";

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct Stamp {
    identity: String,
    size: u64,
    sha256: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Transaction {
    version: u32,
    temporary: String,
    backup: String,
    old: Option<Stamp>,
    new: Stamp,
}

fn marker_path(path: &Path) -> Result<PathBuf, String> {
    let parent = path.parent().ok_or(UNAVAILABLE)?;
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or(UNAVAILABLE)?;
    Ok(parent.join(format!(".{name}.transaction")))
}

fn stamp(path: &Path) -> Result<Option<Stamp>, String> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(UNAVAILABLE.to_string()),
    };
    if !metadata.file_type().is_file() || metadata.len() > MAX_FILE_BYTES {
        return Err(UNAVAILABLE.to_string());
    }
    let mut file = std::fs::File::open(path).map_err(|_| UNAVAILABLE.to_string())?;
    let identity = crate::overlay::creation_file_identity::from_file(&file)?;
    let before = file.metadata().map_err(|_| UNAVAILABLE.to_string())?;
    let mut hasher = Sha256::new();
    let mut bytes = [0_u8; 16 * 1024];
    let mut size = 0_u64;
    loop {
        let count = file.read(&mut bytes).map_err(|_| UNAVAILABLE.to_string())?;
        if count == 0 {
            break;
        }
        size = size.checked_add(count as u64).ok_or(UNAVAILABLE)?;
        if size > MAX_FILE_BYTES {
            return Err(UNAVAILABLE.to_string());
        }
        hasher.update(&bytes[..count]);
    }
    let after = file.metadata().map_err(|_| UNAVAILABLE.to_string())?;
    if size != before.len()
        || size != after.len()
        || crate::overlay::creation_file_identity::from_path(path)
            .ok()
            .as_deref()
            != Some(identity.as_str())
    {
        return Err(UNAVAILABLE.to_string());
    }
    Ok(Some(Stamp {
        identity,
        size,
        sha256: format!("{:x}", hasher.finalize()),
    }))
}

fn valid_stamp(stamp: &Stamp) -> bool {
    crate::overlay::creation_file_identity::valid(&stamp.identity)
        && stamp.size <= MAX_FILE_BYTES
        && stamp.sha256.len() == 64
        && stamp.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(windows)]
fn lock_exact(
    path: &Path,
    expected_identity: Option<&str>,
    delete: bool,
) -> Result<std::fs::File, String> {
    use std::os::windows::fs::{MetadataExt as _, OpenOptionsExt as _};
    use windows::Win32::Storage::FileSystem::{
        DELETE, FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_READ, FILE_SHARE_READ,
    };
    let access = FILE_GENERIC_READ.0 | if delete { DELETE.0 } else { 0 };
    let file = std::fs::OpenOptions::new()
        .read(true)
        .access_mode(access)
        .share_mode(FILE_SHARE_READ.0)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(path)
        .map_err(|_| UNAVAILABLE.to_string())?;
    let metadata = file.metadata().map_err(|_| UNAVAILABLE.to_string())?;
    if !metadata.is_file() || metadata.file_attributes() & 0x400 != 0 {
        return Err(UNAVAILABLE.to_string());
    }
    if let Some(expected) = expected_identity
        && crate::overlay::creation_file_identity::from_file(&file)? != expected
    {
        return Err(UNAVAILABLE.to_string());
    }
    Ok(file)
}

#[cfg(not(windows))]
fn lock_exact(
    path: &Path,
    expected_identity: Option<&str>,
    delete: bool,
) -> Result<std::fs::File, String> {
    crate::overlay::creation_delivery::publication::lock_owned_path(
        path,
        expected_identity,
        false,
        delete,
    )
}

fn members(path: &Path, transaction: &Transaction) -> Result<(PathBuf, PathBuf), String> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or(UNAVAILABLE)?;
    let prefix = format!(".{name}.");
    let identity = transaction
        .temporary
        .strip_prefix(&prefix)
        .and_then(|value| value.strip_suffix(".tmp"))
        .and_then(|value| value.split_once('-'))
        .filter(|(pid, sequence)| {
            !pid.is_empty()
                && !sequence.is_empty()
                && pid.bytes().all(|byte| byte.is_ascii_digit())
                && sequence.bytes().all(|byte| byte.is_ascii_digit())
        })
        .ok_or(UNAVAILABLE)?;
    let _ = identity;
    if transaction.version != 1
        || transaction.backup != format!("{}.backup", transaction.temporary)
        || !valid_stamp(&transaction.new)
        || transaction
            .old
            .as_ref()
            .is_some_and(|value| !valid_stamp(value))
    {
        return Err(UNAVAILABLE.to_string());
    }
    let parent = path.parent().ok_or(UNAVAILABLE)?;
    Ok((
        parent.join(&transaction.temporary),
        parent.join(&transaction.backup),
    ))
}

fn remove_exact(path: &Path, expected: Option<&Stamp>) -> Result<(), String> {
    match (stamp(path)?, expected) {
        (None, _) => Ok(()),
        (Some(actual), Some(expected)) if &actual == expected => {
            let file = lock_exact(path, Some(&expected.identity), true)?;
            if stamp(path)?.as_ref() != Some(expected) {
                return Err(UNAVAILABLE.to_string());
            }
            crate::overlay::creation_delivery::publication::delete_owned(&file, path)
        }
        _ => Err(UNAVAILABLE.to_string()),
    }
}

pub(super) fn reconcile(path: &Path) -> Result<(), String> {
    reconcile_using(path, || {})
}

fn reconcile_using(path: &Path, after_current_locked: impl FnOnce()) -> Result<(), String> {
    let marker = marker_path(path)?;
    let marker_metadata = match std::fs::symlink_metadata(&marker) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(UNAVAILABLE.to_string()),
    };
    if !marker_metadata.file_type().is_file() || marker_metadata.len() > MAX_MARKER_BYTES {
        return Err(UNAVAILABLE.to_string());
    }
    let file = lock_exact(&marker, None, true)?;
    let transaction: Transaction = serde_json::from_reader((&file).take(MAX_MARKER_BYTES + 1))
        .map_err(|_| UNAVAILABLE.to_string())?;
    let (temporary, backup) = members(path, &transaction)?;
    let current = stamp(path)?;
    // Keep the classified journal in place until every dependent cleanup and
    // the marker deletion finish; otherwise its backup could become the only
    // valid copy after a concurrent replacement.
    let mut current_guard = match &current {
        Some(current) => {
            let guard = lock_exact(path, Some(&current.identity), false)?;
            if stamp(path)?.as_ref() != Some(current) {
                return Err(UNAVAILABLE.to_string());
            }
            Some(guard)
        }
        None => None,
    };
    after_current_locked();
    let saved = stamp(&backup)?;
    let pending = stamp(&temporary)?;
    if current.as_ref() == Some(&transaction.new) {
        if pending.is_some() || (saved.is_some() && saved != transaction.old) {
            return Err(UNAVAILABLE.to_string());
        }
        remove_exact(&backup, transaction.old.as_ref())?;
    } else if transaction.old.is_some() && current == transaction.old {
        if saved.is_some() || (pending.is_some() && pending.as_ref() != Some(&transaction.new)) {
            return Err(UNAVAILABLE.to_string());
        }
        remove_exact(&temporary, Some(&transaction.new))?;
    } else if current.is_none()
        && transaction.old.is_some()
        && saved == transaction.old
        && pending.as_ref() == Some(&transaction.new)
    {
        let backup_file = lock_exact(
            &backup,
            transaction
                .old
                .as_ref()
                .map(|stamp| stamp.identity.as_str()),
            true,
        )?;
        if stamp(&backup)? != transaction.old {
            return Err(UNAVAILABLE.to_string());
        }
        crate::overlay::creation_delivery::publication::rename_owned_no_replace(
            &backup_file,
            &backup,
            path,
        )?;
        if stamp(path)? != transaction.old {
            return Err(UNAVAILABLE.to_string());
        }
        current_guard = Some(backup_file);
        remove_exact(&temporary, Some(&transaction.new))?;
    } else {
        return Err(UNAVAILABLE.to_string());
    }
    let result = crate::overlay::creation_delivery::publication::unlink_owned(file, &marker);
    drop(current_guard);
    result
}

pub(super) fn write(path: &Path, bytes: &[u8]) -> Result<(), RecordError> {
    write_using(path, bytes, publish_bound)
}

/// File identities frozen when the transaction marker was written.
struct Publication<'a> {
    old: Option<&'a str>,
    new: &'a str,
}

/// Publishes by file identity instead of by name. The exact stamped preimage
/// moves to the named backup and the exact staged file then takes the journal
/// name, both without replacement, so a file that a same-user process placed
/// at the journal name is never displaced. Each intermediate state is one the
/// durable marker already reconciles.
fn publish_bound(
    path: &Path,
    temporary: &Path,
    backup: &Path,
    publication: &Publication<'_>,
) -> std::io::Result<()> {
    use crate::overlay::creation_delivery::publication::rename_owned_no_replace;
    let unchanged = |_| std::io::Error::from_raw_os_error(storage::UNABLE_TO_REMOVE_REPLACED);
    let staged = lock_exact(temporary, Some(publication.new), true).map_err(unchanged)?;
    if let Some(old) = publication.old {
        let previous = lock_exact(path, Some(old), true).map_err(unchanged)?;
        rename_owned_no_replace(&previous, path, backup).map_err(unchanged)?;
        drop(previous);
        return rename_owned_no_replace(&staged, temporary, path)
            .map_err(|_| std::io::Error::from_raw_os_error(storage::UNABLE_TO_MOVE_REPLACEMENT));
    }
    rename_owned_no_replace(&staged, temporary, path)
        .map_err(|_| std::io::Error::from(std::io::ErrorKind::AlreadyExists))
}

fn write_using(
    path: &Path,
    bytes: &[u8],
    mut publish: impl FnMut(&Path, &Path, &Path, &Publication<'_>) -> std::io::Result<()>,
) -> Result<(), RecordError> {
    let parent = path.parent().ok_or(UNAVAILABLE)?;
    std::fs::create_dir_all(parent).map_err(|_| UNAVAILABLE.to_string())?;
    reconcile(path)?;
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or(UNAVAILABLE)?;
    let old = stamp(path)?;
    let (temporary, mut file) = storage::allocate_temporary(parent, name, || {
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    })
    .map_err(|_| UNAVAILABLE.to_string())?;
    if file
        .write_all(bytes)
        .and_then(|()| file.flush())
        .and_then(|()| file.sync_all())
        .is_err()
    {
        drop(file);
        let _ = std::fs::remove_file(&temporary);
        return Err(UNAVAILABLE.into());
    }
    drop(file);
    let new = stamp(&temporary)?.ok_or(UNAVAILABLE)?;
    let temporary_name = temporary
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or(UNAVAILABLE)?
        .to_string();
    let backup_name = format!("{temporary_name}.backup");
    let backup = parent.join(&backup_name);
    if stamp(&backup)?.is_some() {
        return Err(UNAVAILABLE.into());
    }
    let transaction = Transaction {
        version: 1,
        temporary: temporary_name,
        backup: backup_name,
        old,
        new,
    };
    let encoded = serde_json::to_vec(&transaction).map_err(|_| UNAVAILABLE.to_string())?;
    let marker = marker_path(path)?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&marker)
        .map_err(|_| UNAVAILABLE.to_string())?;
    file.write_all(&encoded)
        .and_then(|()| file.flush())
        .and_then(|()| file.sync_all())
        .map_err(|_| UNAVAILABLE.to_string())?;
    drop(file);
    let publication = Publication {
        old: transaction
            .old
            .as_ref()
            .map(|stamp| stamp.identity.as_str()),
        new: &transaction.new.identity,
    };
    if let Err(error) = storage::retry_replace(|| publish(path, &temporary, &backup, &publication))
    {
        let message = format!("Creation recovery state could not be saved: {error}");
        return Err(if matches!(error.raw_os_error(), Some(1176 | 1177)) {
            RecordError::uncertain(message)
        } else {
            message.into()
        });
    }
    #[cfg(test)]
    if std::env::var_os("SGT_TEST_INTENT_PUBLICATION_UNCERTAIN").is_some() {
        return Err(RecordError::uncertain(UNAVAILABLE.to_string()));
    }
    if verify_published(path, &temporary, &backup, &transaction).is_err() {
        return Err(RecordError::uncertain(UNAVAILABLE.to_string()));
    }
    // Publication succeeded. A cleanup failure must not turn an accepted
    // request into a reported failure: callers may otherwise remove its inputs.
    // The durable marker keeps cleanup and any uncertainty visible on restart.
    if reconcile(path).is_err() {
        crate::log_info!("Creation recovery cleanup deferred until reconciliation.");
    }
    Ok(())
}

fn verify_published(
    path: &Path,
    temporary: &Path,
    backup: &Path,
    transaction: &Transaction,
) -> Result<(), String> {
    let _current = lock_exact(path, Some(&transaction.new.identity), false)?;
    if stamp(path)?.as_ref() != Some(&transaction.new)
        || stamp(temporary)?.is_some()
        || stamp(backup)? != transaction.old
    {
        return Err(UNAVAILABLE.to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
