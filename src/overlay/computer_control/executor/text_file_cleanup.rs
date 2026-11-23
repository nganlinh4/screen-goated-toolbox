//! Identity-checked deletion; recovery bytes also require an unchanged version.

use super::*;
use windows::Win32::Storage::FileSystem::{
    FILE_DISPOSITION_INFO, FileDispositionInfo, SetFileInformationByHandle,
};

pub(super) fn remove_backup(
    path: &Path,
    expected: FileIdentity,
    bytes: &[u8],
    max_bytes: u64,
) -> std::io::Result<()> {
    remove_owned(path, expected, Some((bytes, max_bytes)))
}

pub(super) fn remove_if_identity(path: &Path, expected: FileIdentity) -> std::io::Result<()> {
    remove_owned(path, expected, None)
}

fn remove_owned(
    path: &Path,
    expected: FileIdentity,
    bytes: Option<(&[u8], u64)>,
) -> std::io::Result<()> {
    let mut file = OpenOptions::new()
        .access_mode(FILE_GENERIC_READ.0 | DELETE.0)
        .share_mode(FILE_SHARE_READ.0)
        .open(path)?;
    let actual = file_identity(&file).map_err(transaction_io_error)?;
    if actual != expected {
        return Err(std::io::Error::other(
            "file identity changed before cleanup",
        ));
    }
    if let Some((expected_bytes, maximum)) = bytes {
        let actual = read_file_bounded(&mut file, maximum).map_err(transaction_io_error)?;
        if actual != expected_bytes {
            return Err(std::io::Error::other("backup bytes changed before cleanup"));
        }
    }
    let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
    unsafe {
        SetFileInformationByHandle(
            HANDLE(file.as_raw_handle()),
            FileDispositionInfo,
            std::ptr::from_ref(&disposition).cast(),
            std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
        )
        .map_err(std::io::Error::other)?;
    }
    drop(file);
    Ok(())
}
