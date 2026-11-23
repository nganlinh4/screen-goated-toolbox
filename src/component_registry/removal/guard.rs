use std::io::Read as _;
use std::path::Path;

use anyhow::{Result, bail};
use sha2::{Digest as _, Sha256};

use super::super::receipt::{ComponentReceipt, OwnedComponentFile, is_reparse_point};

pub(super) fn is_io_error(error: &anyhow::Error) -> bool {
    error
        .chain()
        .any(|cause| cause.is::<std::io::Error>() || cause.is::<windows::core::Error>())
}

pub(super) fn is_directory_not_empty(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|error| error.kind() == std::io::ErrorKind::DirectoryNotEmpty)
            || cause
                .downcast_ref::<windows::core::Error>()
                .is_some_and(|error| error.code().0 as u32 == 0x8007_0091)
    })
}

#[cfg(windows)]
pub(super) fn lock_directory(path: &Path) -> Result<Vec<std::fs::File>> {
    use std::os::windows::fs::OpenOptionsExt as _;
    use windows::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_READ,
        FILE_SHARE_READ, FILE_SHARE_WRITE,
    };
    if !path.is_absolute() {
        bail!("component path is not absolute");
    }
    let mut paths = path.ancestors().collect::<Vec<_>>();
    paths.reverse();
    let mut guards = Vec::with_capacity(paths.len());
    for directory in paths {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .access_mode(FILE_GENERIC_READ.0)
            .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0 | FILE_FLAG_BACKUP_SEMANTICS.0)
            .open(directory)?;
        let metadata = file.metadata()?;
        if !metadata.is_dir() || is_reparse_point(&metadata) {
            bail!("component path contains an unsafe directory");
        }
        guards.push(file);
    }
    Ok(guards)
}

#[cfg(not(windows))]
pub(super) fn lock_directory(_path: &Path) -> Result<Vec<std::fs::File>> {
    bail!("safe component removal is unavailable on this platform")
}

pub(super) fn read_receipt(path: &Path) -> Result<(ComponentReceipt, std::fs::File, Vec<u8>)> {
    let mut file = open_file(path, false)?;
    let (receipt, body) = ComponentReceipt::read_with_body(&mut file)?;
    Ok((receipt, file, body))
}

pub(super) enum Expected<'a> {
    Receipt(&'a [u8]),
    Owned(&'a OwnedComponentFile),
}

pub(super) fn delete_file(path: &Path, expected: Expected<'_>) -> Result<bool> {
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(true),
        Err(error) => return Err(error.into()),
        Ok(_) => {}
    }
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("component file has no parent"))?;
    let _guards = lock_directory(parent)?;
    let mut file = match open_file(path, true) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(true),
        Err(error) => return Err(error.into()),
    };
    let metadata = file.metadata()?;
    if !metadata.is_file() || is_reparse_point(&metadata) {
        return Ok(false);
    }
    match expected {
        Expected::Receipt(expected) => {
            if metadata.len() != expected.len() as u64 {
                return Ok(false);
            }
            let mut body = Vec::new();
            file.by_ref()
                .take(expected.len() as u64 + 1)
                .read_to_end(&mut body)?;
            if body != expected {
                return Ok(false);
            }
        }
        Expected::Owned(expected) => {
            if metadata.len() != expected.size_bytes {
                return Ok(false);
            }
            let mut digest = Sha256::new();
            let mut buffer = [0_u8; 128 * 1024];
            let mut count = 0_u64;
            loop {
                let read = file.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                count = count
                    .checked_add(read as u64)
                    .ok_or_else(|| anyhow::anyhow!("component file is too large"))?;
                digest.update(&buffer[..read]);
            }
            if count != expected.size_bytes
                || !format!("{:x}", digest.finalize()).eq_ignore_ascii_case(&expected.sha256)
            {
                return Ok(false);
            }
        }
    }
    mark_for_deletion(&file)?;
    Ok(true)
}

#[cfg(windows)]
fn open_file(path: &Path, delete: bool) -> std::io::Result<std::fs::File> {
    use std::os::windows::fs::OpenOptionsExt as _;
    use windows::Win32::Storage::FileSystem::{
        DELETE, FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_READ, FILE_SHARE_READ,
    };
    std::fs::OpenOptions::new()
        .read(true)
        .access_mode(FILE_GENERIC_READ.0 | if delete { DELETE.0 } else { 0 })
        .share_mode(FILE_SHARE_READ.0)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(path)
}

#[cfg(not(windows))]
fn open_file(_path: &Path, _delete: bool) -> std::io::Result<std::fs::File> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "safe removal unavailable",
    ))
}

#[cfg(windows)]
pub(super) fn delete_directory(path: &Path) -> Result<()> {
    use std::os::windows::fs::OpenOptionsExt as _;
    use windows::Win32::Storage::FileSystem::{
        DELETE, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_READ,
        FILE_SHARE_READ,
    };
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("component directory has no parent"))?;
    let _guards = lock_directory(parent)?;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .access_mode(FILE_GENERIC_READ.0 | DELETE.0)
        .share_mode(FILE_SHARE_READ.0)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0 | FILE_FLAG_BACKUP_SEMANTICS.0)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        bail!("component directory is unsafe");
    }
    mark_for_deletion(&file)
}

#[cfg(not(windows))]
pub(super) fn delete_directory(_path: &Path) -> Result<()> {
    bail!("safe component removal is unavailable on this platform")
}

#[cfg(windows)]
fn mark_for_deletion(file: &std::fs::File) -> Result<()> {
    use std::os::windows::io::AsRawHandle as _;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        FILE_DISPOSITION_INFO, FileDispositionInfo, SetFileInformationByHandle,
    };
    let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
    // SAFETY: the file handle is live and the structure has the required size.
    unsafe {
        SetFileInformationByHandle(
            HANDLE(file.as_raw_handle()),
            FileDispositionInfo,
            std::ptr::from_ref(&disposition).cast(),
            std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
        )?;
    }
    Ok(())
}

#[cfg(not(windows))]
fn mark_for_deletion(_file: &std::fs::File) -> Result<()> {
    bail!("safe component removal is unavailable on this platform")
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn large_receipt_is_readable_and_locked_through_removal() {
        let root = std::env::temp_dir().join(format!(
            "sgt-large-receipt-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let receipt = ComponentReceipt {
            schema_version: 1,
            id: "test-large-inventory".into(),
            version: "1.0.0".into(),
            architecture: "x64".into(),
            dependencies: Vec::new(),
            files: (0..60_000)
                .map(|index| OwnedComponentFile {
                    path: format!("runtime/packages/module{index}/source.py").into(),
                    size_bytes: 1,
                    sha256: "a".repeat(64),
                })
                .collect(),
        };
        crate::component_registry::write_receipt(&root, &receipt).unwrap();
        let path = root.join("receipt.json");
        assert!(std::fs::metadata(&path).unwrap().len() > 2 * 1024 * 1024);
        assert_eq!(ComponentReceipt::read(&path).unwrap().files.len(), 60_000);
        let (actual, handle, body) = read_receipt(&path).unwrap();
        assert_eq!(actual.files.len(), 60_000);
        assert_eq!(body, std::fs::read(&path).unwrap());
        assert!(std::fs::OpenOptions::new().write(true).open(&path).is_err());
        assert!(std::fs::remove_file(&path).is_err());
        drop(handle);
        assert!(delete_file(&path, Expected::Receipt(&body)).unwrap());
        std::fs::remove_dir(root).unwrap();
    }

    #[test]
    fn locked_ancestors_cannot_be_swapped_while_deleting_a_file() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("sgt-removal-guard-{}-{unique}", std::process::id()));
        let managed = root.join("managed");
        let renamed = root.join("renamed");
        let file = managed.join("data");
        std::fs::create_dir_all(&managed).unwrap();
        std::fs::write(&file, b"managed").unwrap();
        let guards = lock_directory(&managed).unwrap();
        assert!(std::fs::rename(&managed, &renamed).is_err());
        let owned = OwnedComponentFile {
            path: "data".into(),
            size_bytes: 7,
            sha256: format!("{:x}", Sha256::digest(b"managed")),
        };
        assert!(delete_file(&file, Expected::Owned(&owned)).unwrap());
        assert!(!file.exists());
        assert!(std::fs::rename(&managed, &renamed).is_err());
        drop(guards);
        std::fs::remove_dir(&managed).unwrap();
        std::fs::remove_dir(&root).unwrap();
    }

    #[test]
    fn changed_receipt_is_not_deleted() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("sgt-receipt-guard-{}-{unique}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let receipt = root.join("receipt.json");
        std::fs::write(&receipt, b"changed").unwrap();
        assert!(!delete_file(&receipt, Expected::Receipt(b"correct")).unwrap());
        assert_eq!(std::fs::read(&receipt).unwrap(), b"changed");
        std::fs::remove_file(receipt).unwrap();
        std::fs::remove_dir(root).unwrap();
    }

    #[test]
    fn modified_recorded_bytes_are_preserved_even_when_size_matches() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("sgt-owned-guard-{}-{unique}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("data");
        std::fs::write(&path, b"altered").unwrap();
        let owned = OwnedComponentFile {
            path: "data".into(),
            size_bytes: 7,
            sha256: format!("{:x}", Sha256::digest(b"managed")),
        };
        assert!(!delete_file(&path, Expected::Owned(&owned)).unwrap());
        assert_eq!(std::fs::read(&path).unwrap(), b"altered");
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
