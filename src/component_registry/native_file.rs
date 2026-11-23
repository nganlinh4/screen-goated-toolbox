//! Opening, verifying and canonicalizing receipt-owned native files.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::os::windows::fs::OpenOptionsExt as _;
use std::path::Path;
#[cfg(not(feature = "recorder-worker"))]
use std::path::PathBuf;

use anyhow::{Context, Result, ensure};
use windows::Win32::Storage::FileSystem::{FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ};

use super::receipt::is_reparse_point;

const MAX_PE_OFFSET: u64 = 1024 * 1024;

pub(super) fn validate_x64_pe(path: &Path) -> Result<()> {
    let mut file = open_regular(path)
        .with_context(|| format!("failed to inspect native file '{}'", path.display()))?;
    let length = file.metadata()?.len();
    inspect_header(&mut file, length)
        .with_context(|| format!("invalid native file '{}'", path.display()))
}

/// The one way a receipt-owned file is opened for verification or launch:
/// readers may share it, writers are denied, and a reparse point is opened as
/// itself and refused rather than followed to a target swapped in after a
/// metadata check.
pub(crate) fn open_regular(path: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ.0)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(path)?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && !is_reparse_point(&metadata),
        "component path is not a regular file"
    );
    Ok(file)
}

/// Opens one receipt-owned file through [`open_regular`] and verifies its size
/// and SHA-256 on that same handle. The returned handle denies writers, so the
/// verified bytes stay pinned for as long as the caller holds it. `changed` is
/// the caller's error message when the file no longer matches its inventory.
pub(super) fn lock_verified(
    root: &Path,
    relative: &str,
    size_bytes: u64,
    sha256: &str,
    changed: &str,
) -> Result<File> {
    let path = super::receipt::resolve_owned_path(root, Path::new(relative))?;
    let mut file = open_regular(&path)?;
    ensure!(file.metadata()?.len() == size_bytes, "{changed}");
    let matched = super::verified_copy::stream_matches(
        &mut file,
        &mut std::io::sink(),
        size_bytes,
        sha256,
        &|| false,
        &|_| {},
    )?;
    ensure!(matched, "{changed}");
    Ok(file)
}

/// Canonicalizes a launch input and requires a regular file that is not a
/// reparse point. `what` names it in errors, for example "recorder worker".
#[cfg(not(feature = "recorder-worker"))]
pub(crate) fn canonical_regular_file(path: &Path, what: &str) -> Result<PathBuf> {
    let canonical = std::fs::canonicalize(path)
        .with_context(|| format!("canonicalize {what} '{}'", path.display()))?;
    let metadata = std::fs::symlink_metadata(&canonical)?;
    ensure!(
        metadata.is_file() && !is_reparse_point(&metadata),
        "{what} path is unsafe"
    );
    Ok(canonical)
}

/// Canonicalizes a launch directory and requires a real directory that is not
/// a reparse point.
#[cfg(not(feature = "recorder-worker"))]
pub(crate) fn canonical_regular_dir(path: &Path, what: &str) -> Result<PathBuf> {
    let canonical = std::fs::canonicalize(path)
        .with_context(|| format!("canonicalize {what} directory '{}'", path.display()))?;
    let metadata = std::fs::symlink_metadata(&canonical)?;
    ensure!(
        metadata.is_dir() && !is_reparse_point(&metadata),
        "{what} directory is unsafe"
    );
    Ok(canonical)
}

fn inspect_header(file: &mut (impl Read + Seek), length: u64) -> Result<()> {
    let mut dos = [0_u8; 64];
    file.read_exact(&mut dos)?;
    ensure!(&dos[..2] == b"MZ", "native file has no DOS signature");
    let offset = u64::from(u32::from_le_bytes(dos[60..64].try_into().unwrap()));
    ensure!(
        (64..=MAX_PE_OFFSET).contains(&offset)
            && offset.checked_add(6).is_some_and(|end| end <= length),
        "native file has an invalid PE offset"
    );
    file.seek(SeekFrom::Start(offset))?;
    let mut header = [0_u8; 6];
    file.read_exact(&mut header)?;
    ensure!(&header[..4] == b"PE\0\0", "native file has no PE signature");
    ensure!(
        u16::from_le_bytes([header[4], header[5]]) == 0x8664,
        "native file is not Windows x64"
    );
    Ok(())
}

#[cfg(test)]
#[path = "native_file_tests.rs"]
mod tests;
