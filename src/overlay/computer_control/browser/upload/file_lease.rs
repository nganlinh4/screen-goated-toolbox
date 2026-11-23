//! Read-only ownership of the exact local resource assessed for disclosure.

use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::fs::{File, OpenOptions};
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS, FILE_READ_ATTRIBUTES, FILE_SHARE_READ,
    FILE_SHARE_WRITE, GetFileInformationByHandle,
};

#[derive(Debug, PartialEq, Eq)]
struct Identity {
    volume: u32,
    index: u64,
    size: u64,
    written: u64,
}

impl Identity {
    fn read(file: &File) -> Result<Self> {
        let metadata = file.metadata()?;
        ensure!(metadata.is_file(), "upload source is not a regular file");
        let mut information = BY_HANDLE_FILE_INFORMATION::default();
        unsafe { GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut information) }?;
        Ok(Self {
            volume: information.dwVolumeSerialNumber,
            index: (u64::from(information.nFileIndexHigh) << 32)
                | u64::from(information.nFileIndexLow),
            size: metadata.len(),
            written: metadata.last_write_time(),
        })
    }
}

pub(super) struct FileLease {
    path: PathBuf,
    requested: String,
    file: File,
    identity: Identity,
    _parents: Vec<File>,
}

impl FileLease {
    pub(super) fn open(path: &str, cancel: &AtomicBool) -> Result<Self> {
        ensure!(!cancel.load(Ordering::Acquire), "upload cancelled");
        ensure!(
            Path::new(path).is_absolute(),
            "upload path must be absolute"
        );
        let canonical = std::fs::canonicalize(path).context("resolve upload source")?;
        let parent = canonical.parent().context("upload source has no parent")?;
        let mut parents = Vec::new();
        let ancestors = parent.ancestors().collect::<Vec<_>>();
        for ancestor in ancestors.into_iter().rev() {
            ensure!(!cancel.load(Ordering::Acquire), "upload cancelled");
            let directory = OpenOptions::new()
                .access_mode(FILE_READ_ATTRIBUTES.0)
                .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0)
                .open(ancestor)?;
            ensure!(
                directory.metadata()?.is_dir(),
                "upload parent is not a directory"
            );
            parents.push(directory);
        }
        let file = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ.0)
            .open(&canonical)?;
        let identity = Identity::read(&file)?;
        let lease = Self {
            path: canonical,
            requested: path.to_string(),
            file,
            identity,
            _parents: parents,
        };
        lease.validate(cancel)?;
        Ok(lease)
    }

    pub(super) fn path(&self) -> Result<&str> {
        self.path
            .to_str()
            .context("upload source path is not Unicode")
    }

    pub(super) fn proposal(&self) -> Result<Value> {
        Ok(json!({
            "requested_path": self.requested, "canonical_path": self.path()?,
            "size_bytes": self.identity.size, "last_write_time": self.identity.written,
            "volume_identity": self.identity.volume, "file_identity": self.identity.index,
        }))
    }

    pub(super) fn validate(&self, cancel: &AtomicBool) -> Result<()> {
        ensure!(!cancel.load(Ordering::Acquire), "upload cancelled");
        ensure!(
            std::fs::canonicalize(&self.path)? == self.path,
            "upload path changed"
        );
        ensure!(
            Identity::read(&self.file)? == self.identity,
            "upload source changed"
        );
        // metadata(path) need not retain the filesystem's file identity fields.
        let named = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ.0)
            .open(&self.path)?;
        ensure!(
            Identity::read(&named)? == self.identity,
            "upload path identity changed"
        );
        ensure!(!cancel.load(Ordering::Acquire), "upload cancelled");
        Ok(())
    }
}

#[cfg(test)]
mod tests;
