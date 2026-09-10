use std::fs::{File, OpenOptions};
use std::os::windows::ffi::OsStrExt as _;
use std::os::windows::fs::OpenOptionsExt as _;
use std::os::windows::io::AsRawHandle as _;
use std::path::Path;

use anyhow::{Context, Result};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Storage::FileSystem::{
    DELETE, FILE_DISPOSITION_FLAG_ON_CLOSE, FILE_DISPOSITION_INFO_EX, FILE_FLAG_DELETE_ON_CLOSE,
    FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_RENAME_INFO, FileDispositionInfoEx, FileRenameInfo,
    SetFileInformationByHandle,
};

/// The kernel, not a destructor or a path sweep, owns interrupted-file cleanup.
pub(super) struct DownloadScratch {
    pub(super) file: File,
}

impl DownloadScratch {
    pub(super) fn create(destination: &Path) -> Result<Self> {
        let mut nonce = [0_u8; 8];
        getrandom::fill(&mut nonce)
            .map_err(|error| anyhow::anyhow!("create download identity: {error}"))?;
        let path = destination.with_extension(format!(
            "verified-download-{:016x}",
            u64::from_le_bytes(nonce)
        ));
        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .access_mode(FILE_GENERIC_READ.0 | FILE_GENERIC_WRITE.0 | DELETE.0)
            .share_mode(0)
            .custom_flags(FILE_FLAG_DELETE_ON_CLOSE.0)
            .open(path)
            .context("create crash-clean model staging file")?;
        Ok(Self { file })
    }

    /// Call only after the complete file's size and digest have been verified.
    /// Rename first: a crash can never leave a persistent nonce-named scratch.
    pub(super) fn publish(self, destination: &Path) -> Result<()> {
        let parent = destination
            .parent()
            .context("model destination has no parent")?;
        let destination = parent.canonicalize()?.join(
            destination
                .file_name()
                .context("model destination has no filename")?,
        );
        let wide = destination.as_os_str().encode_wide().collect::<Vec<_>>();
        let name_bytes = wide
            .len()
            .checked_mul(2)
            .context("model path length overflow")?;
        let size = std::mem::offset_of!(FILE_RENAME_INFO, FileName)
            .checked_add(
                name_bytes
                    .checked_add(2)
                    .context("model path length overflow")?,
            )
            .context("model rename size overflow")?;
        // FileName must have a NUL terminator even though FileNameLength excludes
        // it. Alignment padding alone does not guarantee space for that terminator.
        // FILE_RENAME_INFO contains a HANDLE; keep its allocation aligned as well.
        let mut buffer = vec![0_u64; size.div_ceil(std::mem::size_of::<u64>())];
        unsafe {
            let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFO>();
            (*info).Anonymous.ReplaceIfExists = true;
            (*info).RootDirectory = HANDLE::default();
            (*info).FileNameLength = u32::try_from(name_bytes)?;
            std::ptr::copy_nonoverlapping(
                wide.as_ptr(),
                std::ptr::addr_of_mut!((*info).FileName).cast::<u16>(),
                wide.len(),
            );
            SetFileInformationByHandle(
                HANDLE(self.file.as_raw_handle()),
                FileRenameInfo,
                buffer.as_ptr().cast(),
                u32::try_from(size)?,
            )
            .context("atomically publish verified model file")?;

            // Unlike FileDispositionInfo, the Ex ON_CLOSE operation can clear
            // the delete-on-close state established by CreateFile.
            let disposition = FILE_DISPOSITION_INFO_EX {
                Flags: FILE_DISPOSITION_FLAG_ON_CLOSE,
            };
            SetFileInformationByHandle(
                HANDLE(self.file.as_raw_handle()),
                FileDispositionInfoEx,
                std::ptr::from_ref(&disposition).cast(),
                std::mem::size_of::<FILE_DISPOSITION_INFO_EX>() as u32,
            )
            .context("retain published verified model file")?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "scratch_tests.rs"]
mod tests;
