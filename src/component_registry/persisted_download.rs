//! Pinned downloads whose received bytes survive a failed install attempt.
//!
//! Each body is written to `<owner>-<sha256>.part` with a small sidecar that
//! records the URL, the pinned size and digest, and the server's validators
//! (`ETag` or `Last-Modified`). A later attempt hashes the bytes already
//! received, asks for the rest with `Range` and `If-Range`, and starts over only
//! when the server answers with the complete (changed) representation. The
//! finished file must still match the pinned SHA-256; a mismatch deletes it.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use super::resumable_download::{self, BodyValidators, OpenedBody};

const PART_EXTENSION: &str = "part";
const RECORD_EXTENSION: &str = "part.json";

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PartRecord {
    url: String,
    size: u64,
    sha256: String,
    validators: BodyValidators,
}

/// Opens a body at an offset, honoring an optional `If-Range` validator.
pub(super) type BodyOpener<'a> = dyn FnMut(u64, Option<&str>) -> Result<OpenedBody> + 'a;

fn partials_root() -> PathBuf {
    #[cfg(test)]
    return std::env::temp_dir().join(format!("sgt-component-partials-{}", std::process::id()));
    #[cfg(not(test))]
    crate::paths::app_runtime_local_data_dir().join("component-partials")
}

/// The stable partial-download path for one pinned body of `owner`.
pub(super) fn part_path(owner: &str, sha256: &str) -> Result<PathBuf> {
    super::catalog::validate_identifier(owner)?;
    ensure!(
        sha256.len() == 64 && sha256.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "pinned download digest is invalid"
    );
    let root = partials_root();
    std::fs::create_dir_all(&root)?;
    let metadata = std::fs::symlink_metadata(&root)?;
    ensure!(
        metadata.is_dir() && !super::receipt::is_reparse_point(&metadata),
        "partial download root is unsafe"
    );
    Ok(root.join(format!(
        "{owner}-{}.{PART_EXTENSION}",
        sha256.to_ascii_lowercase()
    )))
}

/// Downloads `url` into `part`, continuing bytes kept by an earlier attempt,
/// and returns the verified file positioned at its start.
pub(super) fn download(
    part: &Path,
    url: &str,
    size: u64,
    sha256: &str,
    cancelled: &AtomicBool,
    progress: &dyn Fn(u64),
) -> Result<File> {
    ensure!(
        url.starts_with("https://") || (cfg!(test) && url.starts_with("http://127.0.0.1:")),
        "pinned component downloads must use HTTPS"
    );
    download_with(
        part,
        url,
        size,
        sha256,
        cancelled,
        progress,
        &mut |offset, if_range| resumable_download::open_body(url, offset, size, if_range),
        &|delay| resumable_download::wait_unless_cancelled(delay, cancelled),
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "the network opener and backoff wait are injected for tests"
)]
pub(super) fn download_with(
    part: &Path,
    url: &str,
    size: u64,
    sha256: &str,
    cancelled: &AtomicBool,
    progress: &dyn Fn(u64),
    open: &mut BodyOpener<'_>,
    wait: &dyn Fn(Duration) -> bool,
) -> Result<File> {
    let record_path = part.with_extension(RECORD_EXTENSION);
    let mut record = PartRecord {
        url: url.to_string(),
        size,
        sha256: sha256.to_ascii_lowercase(),
        validators: BodyValidators::default(),
    };
    let kept = read_record(&record_path).filter(|kept| {
        kept.url == record.url && kept.size == record.size && kept.sha256 == record.sha256
    });
    let mut file = open_part(part)?;
    let mut received = file.metadata()?.len();
    if kept.is_none() || received > size {
        file.set_len(0)?;
        received = 0;
    }
    if kept.is_some() && received == size {
        // A body completed by an earlier attempt or an unguarded prefetch.
        file.seek(SeekFrom::Start(0))?;
        if super::verified_cache::file_has_digest(part, &mut file, sha256)? {
            progress(size);
            file.seek(SeekFrom::Start(0))?;
            return Ok(file);
        }
        drop(file);
        discard(part);
        bail!("delivered content differs from its pinned digest");
    }
    let if_range = kept.and_then(|kept| kept.validators.if_range().map(str::to_owned));
    let mut hash = hash_received(&mut file, received, cancelled)?;
    progress(received);

    if received < size {
        let opened = open(received, if_range.as_deref())?;
        if opened.offset != received {
            // The representation changed or the range was ignored: start over.
            file.set_len(0)?;
            file.seek(SeekFrom::Start(0))?;
            hash = Sha256::new();
            received = 0;
            progress(0);
        }
        record.validators = opened.validators;
        write_record(&record_path, &record)?;
        let validator = record.validators.if_range().map(str::to_owned);
        let mut fetch = |offset: u64| -> Result<Box<dyn Read>> {
            let body = open(offset, validator.as_deref())?;
            ensure!(
                body.offset == offset,
                "download changed on the server while resuming at byte {offset}"
            );
            Ok(body.reader)
        };
        let copied = resumable_download::copy_from(
            opened.reader,
            &mut fetch,
            &mut file,
            received,
            hash,
            size,
            cancelled,
            progress,
            wait,
        );
        // Keep what arrived for the next attempt, even when this one failed.
        file.flush()?;
        file.sync_all()?;
        hash = copied?;
    }

    if !format!("{:x}", hash.finalize()).eq_ignore_ascii_case(sha256) {
        drop(file);
        discard(part);
        bail!("delivered content differs from its pinned digest");
    }
    super::verified_cache::remember(part, &file, sha256);
    file.seek(SeekFrom::Start(0))?;
    Ok(file)
}

/// One pinned body of a component.
pub(super) struct PinnedBody<'a> {
    pub(super) url: &'a str,
    pub(super) size: u64,
    pub(super) sha256: &'a str,
}

/// Transfers `bodies` of `owner` into their partial files without holding the
/// registry mutation guard. The installer then takes that guard only to
/// extract and publish, and finds every body complete and already verified.
pub(super) fn prefetch(
    owner: &str,
    bodies: &[PinnedBody<'_>],
    cancelled: &AtomicBool,
    progress: &dyn Fn(u64, u64),
) -> Result<()> {
    let total = bodies
        .iter()
        .try_fold(0_u64, |total, body| total.checked_add(body.size))
        .ok_or_else(|| anyhow::anyhow!("download size overflow"))?;
    let _transfer = super::mutation::acquire_transfer_guard_cancellable(owner, cancelled)?;
    let mut completed = 0_u64;
    for body in bodies {
        let part = part_path(owner, body.sha256)?;
        drop(download(
            &part,
            body.url,
            body.size,
            body.sha256,
            cancelled,
            &|done| progress(completed.saturating_add(done), total),
        )?);
        completed = completed.saturating_add(body.size);
    }
    Ok(())
}

/// Downloads (or finishes) one pinned body of `owner` into its persisted
/// partial file and returns that verified file's path. The installer reads it
/// under the registry guard and discards it once the component is published;
/// a failed install keeps it for the next attempt.
pub(super) fn fetch(
    owner: &str,
    body: &PinnedBody<'_>,
    cancelled: &AtomicBool,
    progress: &dyn Fn(u64, u64),
) -> Result<PathBuf> {
    let part = part_path(owner, body.sha256)?;
    drop(download(
        &part,
        body.url,
        body.size,
        body.sha256,
        cancelled,
        &|done| progress(done, body.size),
    )?);
    Ok(part)
}

/// Removes a partial or consumed download and its record.
pub(super) fn discard(part: &Path) {
    for path in [part.to_path_buf(), part.with_extension(RECORD_EXTENSION)] {
        if let Err(error) = std::fs::remove_file(&path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            crate::log_info!(
                "[Components] partial download preserved at {}: {error}",
                path.display()
            );
        }
    }
}

/// Removes every partial download of `owner` once it was published or
/// removed, so a superseded version's leftover part does not stay behind.
/// Parts that cannot be removed (an in-flight transfer) are logged.
pub(super) fn discard_leftovers(owner: &str) {
    match discard_owned(owner) {
        Ok(preserved) => {
            for path in preserved {
                crate::log_info!(
                    "[Components] partial download preserved at {}",
                    path.display()
                );
            }
        }
        Err(error) => {
            crate::log_info!("[Components] partial downloads of {owner} preserved: {error:#}")
        }
    }
}

/// Removes every partial download of `owner`, returning paths that could not
/// be removed. Used when the component itself is removed.
pub(super) fn discard_owned(owner: &str) -> Result<Vec<PathBuf>> {
    super::catalog::validate_identifier(owner)?;
    let root = partials_root();
    let entries = match std::fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let prefix = format!("{owner}-");
    let mut preserved = Vec::new();
    for entry in entries {
        let path = entry?.path();
        let owned = path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_prefix(&prefix))
            .and_then(|rest| {
                rest.strip_suffix(&format!(".{RECORD_EXTENSION}"))
                    .or_else(|| rest.strip_suffix(&format!(".{PART_EXTENSION}")))
            })
            .is_some_and(|digest| {
                digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
            });
        if owned
            && let Err(error) = std::fs::remove_file(&path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            preserved.push(path);
        }
    }
    Ok(preserved)
}

fn open_part(part: &Path) -> Result<File> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        use windows::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;
        options
            .share_mode(0)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0);
    }
    let file = options.open(part)?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && !super::receipt::is_reparse_point(&metadata),
        "partial download is not a regular file"
    );
    Ok(file)
}

fn hash_received(file: &mut File, received: u64, cancelled: &AtomicBool) -> Result<Sha256> {
    let mut hash = Sha256::new();
    file.seek(SeekFrom::Start(0))?;
    let mut remaining = received;
    let mut buffer = vec![0_u8; 256 * 1024];
    while remaining > 0 {
        ensure!(!cancelled.load(Ordering::Acquire), "download cancelled");
        let wanted = usize::try_from(remaining.min(buffer.len() as u64))?;
        let read = file.read(&mut buffer[..wanted])?;
        ensure!(read > 0, "partial download ended early");
        hash.update(&buffer[..read]);
        remaining -= read as u64;
    }
    file.seek(SeekFrom::Start(received))?;
    Ok(hash)
}

fn read_record(path: &Path) -> Option<PartRecord> {
    let bytes = std::fs::read(path).ok()?;
    (bytes.len() <= 64 * 1024)
        .then(|| serde_json::from_slice(&bytes).ok())
        .flatten()
}

fn write_record(path: &Path, record: &PartRecord) -> Result<()> {
    crate::atomic_json::write_json_atomic(path, record).map_err(Into::into)
}

#[cfg(test)]
#[path = "persisted_download_tests.rs"]
mod tests;
