//! Metadata-only snapshots of a destination entry and its current file object.

use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Globalization::{CSTR_EQUAL, CompareStringOrdinal};
use windows::Win32::Storage::FileSystem::{
    FILE_CASE_SENSITIVE_INFO, FILE_FLAG_BACKUP_SEMANTICS, FILE_ID_INFO, FILE_READ_ATTRIBUTES,
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FileCaseSensitiveInfo, FileIdInfo,
    GetFileInformationByHandleEx, GetFinalPathNameByHandleW, VOLUME_NAME_DOS,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FileId {
    volume: u64,
    id: [u8; 16],
}

impl FileId {
    fn read(file: &File) -> Result<Self> {
        let mut info = FILE_ID_INFO::default();
        unsafe {
            GetFileInformationByHandleEx(
                HANDLE(file.as_raw_handle()),
                FileIdInfo,
                (&mut info as *mut FILE_ID_INFO).cast(),
                std::mem::size_of::<FILE_ID_INFO>() as u32,
            )
        }?;
        Ok(Self {
            volume: info.VolumeSerialNumber,
            id: info.FileId.Identifier,
        })
    }
}

#[derive(Clone)]
struct Pinned {
    id: FileId,
    file: Arc<File>,
    case_sensitive: Option<bool>,
}

impl Pinned {
    fn open(path: &Path) -> Result<Self> {
        let file = OpenOptions::new()
            .access_mode(FILE_READ_ATTRIBUTES.0)
            .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0 | FILE_SHARE_DELETE.0)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0)
            .open(path)?;
        let mut case = FILE_CASE_SENSITIVE_INFO::default();
        let case_sensitive = file
            .metadata()?
            .is_dir()
            .then(|| {
                unsafe {
                    GetFileInformationByHandleEx(
                        HANDLE(file.as_raw_handle()),
                        FileCaseSensitiveInfo,
                        (&mut case as *mut FILE_CASE_SENSITIVE_INFO).cast(),
                        std::mem::size_of::<FILE_CASE_SENSITIVE_INFO>() as u32,
                    )
                }
                .ok()
                .map(|()| case.Flags != 0)
            })
            .flatten();
        Ok(Self {
            case_sensitive,
            id: FileId::read(&file)?,
            file: Arc::new(file),
        })
    }

    fn path(&self) -> Result<PathBuf> {
        let mut buffer = vec![0u16; 512];
        loop {
            let length = unsafe {
                GetFinalPathNameByHandleW(
                    HANDLE(self.file.as_raw_handle()),
                    &mut buffer,
                    VOLUME_NAME_DOS,
                )
            } as usize;
            ensure!(
                length != 0,
                "could not resolve the retained filesystem handle"
            );
            if length < buffer.len() {
                return Ok(PathBuf::from(OsString::from_wide(&buffer[..length])));
            }
            ensure!(
                length < 65_536,
                "resolved filesystem path exceeds its native budget"
            );
            buffer.resize(length + 1, 0);
        }
    }

    fn description(&self) -> Value {
        json!({"volume":self.id.volume,"file_id":format!("{:032x}", u128::from_le_bytes(self.id.id))})
    }
}

#[derive(Clone)]
pub(super) struct Snapshot {
    path: PathBuf,
    anchor_path: PathBuf,
    anchor: Pinned,
    tail: PathBuf,
    file: Option<Pinned>,
}

impl Snapshot {
    pub(super) fn capture(path: &Path) -> Result<Self> {
        ensure!(path.is_absolute(), "resource target is not absolute");
        match Pinned::open(path) {
            Ok(file) => {
                ensure!(
                    file.file.metadata()?.is_file(),
                    "resource target is not a regular file"
                );
                let canonical = file.path()?;
                let parent = canonical
                    .parent()
                    .context("resource target has no parent")?;
                let anchor = Pinned::open(parent)?;
                ensure!(
                    anchor.file.metadata()?.is_dir(),
                    "resource parent is not a directory"
                );
                let anchor_path = anchor.path()?;
                let tail = PathBuf::from(
                    canonical
                        .file_name()
                        .context("resource target has no name")?,
                );
                ensure!(
                    anchor_path.join(&tail) == canonical && file.path()? == canonical,
                    "resource entry changed while resolving its identity"
                );
                ensure!(
                    Pinned::open(path)?.id == file.id,
                    "resource path changed during identity capture"
                );
                Ok(Self {
                    path: canonical,
                    anchor_path,
                    anchor,
                    tail,
                    file: Some(file),
                })
            }
            Err(error)
                if error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound) =>
            {
                Self::missing(path)
            }
            Err(error) => Err(error),
        }
    }

    fn missing(path: &Path) -> Result<Self> {
        let mut ancestor = path.parent().context("resource target has no parent")?;
        let mut missing = vec![
            path.file_name()
                .context("resource target has no name")?
                .to_os_string(),
        ];
        loop {
            match Pinned::open(ancestor) {
                Ok(anchor) => {
                    ensure!(
                        anchor.file.metadata()?.is_dir(),
                        "resource ancestor is not a directory"
                    );
                    let anchor_path = anchor.path()?;
                    let tail = missing.iter().rev().collect::<PathBuf>();
                    let still_missing = Pinned::open(path).err().is_some_and(|error| {
                        error
                            .downcast_ref::<std::io::Error>()
                            .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound)
                    });
                    ensure!(still_missing, "resource appeared during identity capture");
                    return Ok(Self {
                        path: anchor_path.join(&tail),
                        anchor_path,
                        anchor,
                        tail,
                        file: None,
                    });
                }
                Err(error)
                    if error
                        .downcast_ref::<std::io::Error>()
                        .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound) =>
                {
                    missing.push(
                        ancestor
                            .file_name()
                            .context("resource ancestor is unavailable")?
                            .to_os_string(),
                    );
                    ancestor = ancestor
                        .parent()
                        .context("resource ancestor is unavailable")?;
                }
                Err(error) => return Err(error),
            }
        }
    }

    pub(super) fn same_entry(&self, other: &Self) -> bool {
        if self.anchor.id == other.anchor.id {
            if self.tail == other.tail {
                return true;
            }
            return names_equal(
                &self.tail,
                &other.tail,
                self.anchor.case_sensitive,
                other.anchor.case_sensitive,
            ) && self.current_case_alias(other);
        }
        // A previously missing parent may have been created by the exact export.
        // Both pre-existing anchors must still identify their original objects.
        self.path == other.path
            && (self.file.is_none() || other.file.is_none())
            && self.anchor_is_current()
            && other.anchor_is_current()
    }

    fn anchor_is_current(&self) -> bool {
        Pinned::open(&self.anchor_path).is_ok_and(|anchor| anchor.id == self.anchor.id)
    }

    fn current_case_alias(&self, other: &Self) -> bool {
        let resolve = || -> Result<bool> {
            let left = Pinned::open(&self.anchor.path()?.join(&self.tail))?;
            let right = Pinned::open(&other.anchor.path()?.join(&other.tail))?;
            Ok(left.id == right.id && left.path()? == right.path()?)
        };
        resolve().unwrap_or(false)
    }

    pub(super) fn same_file(&self, other: &Self) -> bool {
        self.file
            .as_ref()
            .zip(other.file.as_ref())
            .is_some_and(|(left, right)| left.id == right.id)
    }

    pub(super) fn related(&self, other: &Self) -> bool {
        self.same_entry(other) || self.same_file(other)
    }

    pub(super) fn has_file(&self) -> bool {
        self.file.is_some()
    }

    pub(super) fn while_receipt_matches<T>(
        &self,
        receipt: &Value,
        operation: impl FnOnce() -> T,
    ) -> Option<T> {
        let expected = receipt.get("sha256").and_then(Value::as_str)?;
        if expected.len() != 64 || !expected.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        let pinned = self.file.as_ref()?;
        super::super::super::executor::with_stable_file(
            &self.path,
            32 * 1024 * 1024,
            |file, bytes| {
                if FileId::read(file).ok() != Some(pinned.id)
                    || !format!("{:x}", Sha256::digest(bytes)).eq_ignore_ascii_case(expected)
                {
                    return None;
                }
                let current = Self::capture(&self.path).ok()?;
                if !self.same_entry(&current) || !self.same_file(&current) {
                    return None;
                }
                Some(operation())
            },
        )
        .flatten()
    }

    pub(super) fn description(&self) -> Value {
        json!({"canonical_target":self.path,"entry":{"ancestor":self.anchor.description(),"relative_name":self.tail,"case_sensitive":self.anchor.case_sensitive},
            "file":self.file.as_ref().map(Pinned::description)})
    }
}

fn names_equal(
    left: &Path,
    right: &Path,
    left_case: Option<bool>,
    right_case: Option<bool>,
) -> bool {
    if left == right {
        return true;
    }
    if left_case != Some(false)
        || right_case != Some(false)
        || left.components().count() != 1
        || right.components().count() != 1
    {
        return false;
    }
    let left = left.as_os_str().encode_wide().collect::<Vec<_>>();
    let right = right.as_os_str().encode_wide().collect::<Vec<_>>();
    unsafe { CompareStringOrdinal(&left, &right, true) == CSTR_EQUAL }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_proven_case_insensitive_parents_can_match_case_aliases() {
        let upper = Path::new("Future.csv");
        let lower = Path::new("future.csv");
        assert!(names_equal(upper, lower, Some(false), Some(false)));
        for (left, right) in [
            (None, None),
            (Some(true), Some(true)),
            (Some(false), Some(true)),
            (None, Some(false)),
        ] {
            assert!(!names_equal(upper, lower, left, right));
        }
        assert!(!names_equal(
            Path::new("new/Future.csv"),
            Path::new("new/future.csv"),
            Some(false),
            Some(false)
        ));
    }
}
