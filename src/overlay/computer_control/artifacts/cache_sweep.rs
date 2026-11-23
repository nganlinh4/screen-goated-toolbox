//! Startup retention for the artifact cache directory.
//!
//! Live cache files are kernel delete-on-close and vanish with their session,
//! so a file left here is an orphan from an older build or a crash. The sweep
//! removes orphans past an age limit, then the oldest beyond a byte cap. It
//! never follows reparse points and never deletes a file that any process still
//! holds open (that is how a concurrent session's live cache is recognised).

use std::fs::{File, OpenOptions};
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use windows::Win32::Foundation::HANDLE;
use windows::Win32::Storage::FileSystem::{
    DELETE, FILE_ATTRIBUTE_REPARSE_POINT, FILE_DISPOSITION_INFO, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_READ_ATTRIBUTES, FileDispositionInfo, SetFileInformationByHandle,
};

const ERROR_SHARING_VIOLATION: i32 = 32;
const MAX_SCANNED_ENTRIES: usize = 4096;

#[derive(Clone, Copy)]
pub(super) struct Policy {
    pub(super) max_age: Duration,
    pub(super) max_bytes: u64,
}

/// Orphans older than a day are removed; younger ones only beyond 64 MiB.
pub(super) const STARTUP_POLICY: Policy = Policy {
    max_age: Duration::from_secs(24 * 60 * 60),
    max_bytes: 64 * 1024 * 1024,
};

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Report {
    pub(super) removed: usize,
    pub(super) in_use: usize,
    pub(super) kept: usize,
}

pub(super) fn cache_dir() -> PathBuf {
    crate::paths::app_temp_dir().join("cc-artifacts")
}

/// Sweep once per process, off the caller's thread.
pub(super) fn sweep_once_in_background() {
    static STARTED: AtomicBool = AtomicBool::new(false);
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("cc-artifact-sweep".into())
        .spawn(|| {
            let report = sweep(&cache_dir(), SystemTime::now(), STARTUP_POLICY);
            if report.removed > 0 {
                crate::log_info!(
                    "[ComputerControl] removed {} orphaned artifact cache files",
                    report.removed
                );
            }
        });
    if spawned.is_err() {
        STARTED.store(false, Ordering::SeqCst);
    }
}

pub(super) fn sweep(dir: &Path, now: SystemTime, policy: Policy) -> Report {
    let mut report = Report::default();
    if !is_plain_directory(dir) {
        return report;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return report;
    };
    let mut candidates = Vec::new();
    for entry in entries.take(MAX_SCANNED_ENTRIES).flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !(name.starts_with("art_") && name.ends_with(".txt")) {
            continue;
        }
        // DirEntry metadata does not traverse reparse points on Windows.
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if !is_plain_file(&metadata) {
            continue;
        }
        let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        candidates.push((entry.path(), modified, metadata.len()));
    }
    candidates.sort_by_key(|(_, modified, _)| *modified);
    let mut total = candidates
        .iter()
        .fold(0u64, |sum, (_, _, len)| sum.saturating_add(*len));
    for (path, modified, len) in candidates {
        let expired = now
            .duration_since(modified)
            .is_ok_and(|age| age > policy.max_age);
        if !expired && total <= policy.max_bytes {
            report.kept += 1;
            continue;
        }
        match remove_unowned(&path) {
            Ok(true) => {
                report.removed += 1;
                total = total.saturating_sub(len);
            }
            Ok(false) => report.in_use += 1,
            Err(_) => report.kept += 1,
        }
    }
    report
}

/// Delete through an exclusive handle: any open handle elsewhere (a live
/// session's cache file) makes the open fail, and the file is kept.
fn remove_unowned(path: &Path) -> std::io::Result<bool> {
    let file = match OpenOptions::new()
        .access_mode(DELETE.0 | FILE_READ_ATTRIBUTES.0)
        .share_mode(0)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.raw_os_error() == Some(ERROR_SHARING_VIOLATION) => return Ok(false),
        Err(error) => return Err(error),
    };
    if !is_plain_file(&file.metadata()?) {
        return Err(std::io::Error::other(
            "artifact cache entry is not a plain file",
        ));
    }
    delete_by_handle(&file)?;
    Ok(true)
}

fn delete_by_handle(file: &File) -> std::io::Result<()> {
    let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
    // SAFETY: the handle is open with DELETE access for the duration of the call
    // and the buffer is a correctly sized FILE_DISPOSITION_INFO.
    unsafe {
        SetFileInformationByHandle(
            HANDLE(file.as_raw_handle()),
            FileDispositionInfo,
            std::ptr::from_ref(&disposition).cast(),
            std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
        )
    }
    .map_err(std::io::Error::other)
}

fn is_plain_directory(dir: &Path) -> bool {
    std::fs::symlink_metadata(dir).is_ok_and(|metadata| {
        metadata.is_dir() && metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 == 0
    })
}

fn is_plain_file(metadata: &std::fs::Metadata) -> bool {
    metadata.is_file() && metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 == 0
}

#[cfg(test)]
#[path = "cache_sweep_tests.rs"]
mod tests;
