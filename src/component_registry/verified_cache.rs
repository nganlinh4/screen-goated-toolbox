//! Process-lifetime record of component files whose SHA-256 already matched.
//!
//! Install checks and status probes re-validate inventories of many gigabytes.
//! A file whose identity (volume and file index), size, write time and creation
//! time are unchanged since it was hashed against the same expected digest is
//! not hashed again. Any difference misses and the file is hashed again, and a
//! failed or racing hash clears the entry. The record lives only in memory, so
//! nothing on disk can vouch for a file, and loads that pin files with
//! write-denied handles (`native_file::lock_verified`) always hash.

use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use anyhow::Result;

const MAX_ENTRIES: usize = 16_384;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FileStamp {
    volume: u32,
    index: u64,
    size: u64,
    written: u64,
    created: u64,
}

struct VerifiedFile {
    stamp: FileStamp,
    sha256: String,
}

static VERIFIED: LazyLock<Mutex<HashMap<PathBuf, VerifiedFile>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[cfg(test)]
thread_local! {
    static HASH_PASSES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Whether `file`, opened at `path`, has the SHA-256 `sha256`. The file is
/// hashed from its current position unless this process already verified the
/// same unchanged file against the same digest.
pub(super) fn file_has_digest(path: &Path, file: &mut File, sha256: &str) -> Result<bool> {
    let sha256 = sha256.to_ascii_lowercase();
    let before = stamp(file);
    if let Some(before) = before
        && is_recorded(path, before, &sha256)
    {
        return Ok(true);
    }
    let matches = hash_file(file)? == sha256;
    match (matches, before, stamp(file)) {
        (true, Some(before), Some(after)) if before == after => record(path, before, sha256),
        _ => forget(path),
    }
    Ok(matches)
}

/// Records that `file`, opened at `path`, was just verified to have `sha256`
/// by a caller that hashed its bytes while writing them.
pub(super) fn remember(path: &Path, file: &File, sha256: &str) {
    match stamp(file) {
        Some(stamp) => record(path, stamp, sha256.to_ascii_lowercase()),
        None => forget(path),
    }
}

/// Drops any record for `path`, for callers that observed it no longer match.
pub(super) fn forget(path: &Path) {
    if let Ok(mut verified) = VERIFIED.lock() {
        verified.remove(path);
    }
}

fn is_recorded(path: &Path, stamp: FileStamp, sha256: &str) -> bool {
    let Ok(mut verified) = VERIFIED.lock() else {
        return false;
    };
    match verified.get(path) {
        Some(entry) if entry.stamp == stamp && entry.sha256 == sha256 => true,
        Some(_) => {
            verified.remove(path);
            false
        }
        None => false,
    }
}

fn record(path: &Path, stamp: FileStamp, sha256: String) {
    let Ok(mut verified) = VERIFIED.lock() else {
        return;
    };
    if verified.len() >= MAX_ENTRIES && !verified.contains_key(path) {
        verified.clear();
    }
    verified.insert(path.to_path_buf(), VerifiedFile { stamp, sha256 });
}

fn hash_file(file: &mut File) -> Result<String> {
    #[cfg(test)]
    HASH_PASSES.with(|passes| passes.set(passes.get() + 1));
    Ok(crate::crypto::sha256_reader(file)?)
}

fn stamp(file: &File) -> Option<FileStamp> {
    use std::os::windows::io::AsRawHandle as _;
    use windows::Win32::Foundation::{FILETIME, HANDLE};
    use windows::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
    };
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    unsafe { GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut info) }.ok()?;
    let wide = |high: u32, low: u32| (u64::from(high) << 32) | u64::from(low);
    let time = |value: FILETIME| wide(value.dwHighDateTime, value.dwLowDateTime);
    Some(FileStamp {
        volume: info.dwVolumeSerialNumber,
        index: wide(info.nFileIndexHigh, info.nFileIndexLow),
        size: wide(info.nFileSizeHigh, info.nFileSizeLow),
        written: time(info.ftLastWriteTime),
        created: time(info.ftCreationTime),
    })
}

#[cfg(test)]
pub(super) fn hash_passes() -> usize {
    HASH_PASSES.with(std::cell::Cell::get)
}

#[cfg(test)]
#[path = "verified_cache_tests.rs"]
mod tests;
