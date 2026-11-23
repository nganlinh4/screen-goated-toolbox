//! Owned scratch storage and collision-safe publication for verified updates.

use std::fs::{File, OpenOptions};
use std::os::windows::ffi::OsStrExt as _;
use std::os::windows::fs::OpenOptionsExt as _;
use std::os::windows::io::AsRawHandle as _;
use std::path::Path;

use anyhow::{Context, Result};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Storage::FileSystem::{
    DELETE, FILE_DISPOSITION_FLAG_ON_CLOSE, FILE_DISPOSITION_INFO_EX, FILE_FLAG_DELETE_ON_CLOSE,
    FILE_GENERIC_READ, FILE_GENERIC_WRITE, FileDispositionInfoEx, SetFileInformationByHandle,
};

/// Keep the canonical directory chain and exact file stable through process creation.
pub(super) fn with_locked_target<T>(
    path: &Path,
    action: impl FnOnce(&mut File) -> Result<T>,
) -> Result<T> {
    use std::os::windows::fs::MetadataExt;
    use windows::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };
    anyhow::ensure!(path.is_absolute(), "Update path must be absolute");
    let parent = path.parent().context("Update path has no parent")?;
    let mut ancestors: Vec<_> = parent.ancestors().collect();
    ancestors.reverse();
    let mut directories = Vec::new();
    for directory in ancestors {
        let file = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0)
            .open(directory)?;
        let metadata = file.metadata()?;
        anyhow::ensure!(
            metadata.is_dir() && metadata.file_attributes() & 0x400 == 0,
            "Update directory is unsafe"
        );
        directories.push(file);
    }
    let mut file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ.0)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(path)
        .context("Verified staged update is missing")?;
    let metadata = file.metadata()?;
    anyhow::ensure!(
        metadata.is_file() && metadata.file_attributes() & 0x400 == 0,
        "Update is not a regular file"
    );
    let result = action(&mut file);
    drop(file);
    drop(directories);
    result
}

pub(super) struct UpdateScratch {
    pub(super) file: File,
}

impl UpdateScratch {
    pub(super) fn create(destination: &Path) -> Result<Self> {
        let mut nonce = [0_u8; 8];
        getrandom::fill(&mut nonce)
            .map_err(|error| anyhow::anyhow!("create update identity: {error}"))?;
        let path =
            destination.with_extension(format!("update-{:016x}.part", u64::from_le_bytes(nonce)));
        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .access_mode(FILE_GENERIC_READ.0 | FILE_GENERIC_WRITE.0 | DELETE.0)
            .share_mode(0)
            .custom_flags(FILE_FLAG_DELETE_ON_CLOSE.0)
            .open(path)
            .context("failed to create owned update staging file")?;
        Ok(Self { file })
    }

    /// Publish without replacing an existing path; the kernel removes abandoned scratch.
    pub(super) fn publish(self, destination: &Path) -> Result<()> {
        let parent = destination
            .parent()
            .context("update destination has no parent")?;
        let destination = parent.canonicalize()?.join(
            destination
                .file_name()
                .context("update destination has no filename")?,
        );
        let wide = destination.as_os_str().encode_wide().collect::<Vec<_>>();
        crate::verified_scratch::rename_file_by_handle(&self.file, &wide, false)
            .context("failed to publish update without replacing an existing file")?;
        unsafe {
            let disposition = FILE_DISPOSITION_INFO_EX {
                Flags: FILE_DISPOSITION_FLAG_ON_CLOSE,
            };
            SetFileInformationByHandle(
                HANDLE(self.file.as_raw_handle()),
                FileDispositionInfoEx,
                std::ptr::from_ref(&disposition).cast(),
                std::mem::size_of::<FILE_DISPOSITION_INFO_EX>() as u32,
            )
            .context("failed to retain published update")?;
        }
        Ok(())
    }
}
