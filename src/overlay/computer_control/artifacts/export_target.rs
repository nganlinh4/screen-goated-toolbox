//! A prepared destination route; parent creation is deferred until commit.

use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use anyhow::{Context, Result, ensure};
use windows::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
};

use crate::overlay::computer_control::native_file::Identity;

pub(super) struct Target {
    requested: PathBuf,
    pub(super) path: PathBuf,
    directories: Vec<PinnedDirectory>,
    anchor_identity: Identity,
    missing: Vec<OsString>,
    pub(super) created_directories: usize,
    pub(super) created_paths: Vec<PathBuf>,
}

impl Target {
    pub(super) fn prepare(requested: &Path) -> Result<Self> {
        ensure!(
            requested.is_absolute(),
            "path must be an absolute output path"
        );
        ensure!(
            requested.file_name().is_some(),
            "path must name an output file"
        );
        let path = resolve(requested)?;
        let mut anchor = path.parent().context("target has no parent")?;
        let mut missing = Vec::new();
        while !entry_exists(anchor)? {
            missing.push(
                anchor
                    .file_name()
                    .context("missing parent has no name")?
                    .to_owned(),
            );
            anchor = anchor.parent().context("target has no existing ancestor")?;
        }
        let mut directories = Vec::new();
        for ancestor in anchor.ancestors().collect::<Vec<_>>().into_iter().rev() {
            directories.push(pin(ancestor)?);
        }
        ensure!(
            resolve(requested)? == path,
            "target route changed during preparation"
        );
        let anchor_identity = directories
            .last()
            .context("target has no pinned ancestor")?
            .identity;
        missing.reverse();
        Ok(Self {
            requested: requested.to_owned(),
            path,
            directories,
            anchor_identity,
            missing,
            created_directories: 0,
            created_paths: Vec::new(),
        })
    }

    pub(super) fn proposal(&self) -> serde_json::Value {
        let anchor = self.anchor_identity;
        serde_json::json!({
            "requested_path": self.requested.to_string_lossy(),
            "canonical_target": self.path.to_string_lossy(),
            "ancestor_volume": anchor.volume,
            "ancestor_file_id": format!("{:032x}", u128::from_le_bytes(anchor.id)),
            "missing_parent_count": self.missing.len(),
        })
    }

    pub(super) fn revalidate(&self) -> Result<()> {
        for directory in &self.directories {
            ensure!(
                pin(&directory.path)?.identity == directory.identity,
                "authorized parent directory identity changed"
            );
        }
        ensure!(
            resolve(&self.requested)? == self.path,
            "authorized target route changed"
        );
        Ok(())
    }

    pub(super) fn create_parents(&mut self, cancel: &AtomicBool) -> Result<()> {
        self.revalidate()?;
        let parent = self.path.parent().context("target has no parent")?;
        let mut current = parent.to_owned();
        for _ in &self.missing {
            current.pop();
        }
        for name in &self.missing {
            current.push(name);
            // A concurrent directory may have appeared. Pin and validate it, but
            // never follow a newly introduced reparse point.
            let creation = crate::overlay::computer_control::executor::with_job_admission(
                Some(cancel),
                || std::fs::create_dir(&current),
            )
            .context("artifact parent creation cancelled")?;
            match creation {
                Ok(()) => {
                    self.created_directories += 1;
                    self.created_paths.push(current.clone());
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error.into()),
            }
            self.directories.push(pin(&current)?);
        }
        self.revalidate()
    }

    pub(super) fn parent(&self) -> &File {
        &self
            .directories
            .last()
            .expect("pinned publication parent")
            .file
    }
}

fn entry_exists(path: &Path) -> Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn resolve(path: &Path) -> Result<PathBuf> {
    let mut cursor = path;
    let mut missing = Vec::new();
    while !entry_exists(cursor)? {
        let name = cursor
            .file_name()
            .context("target has no resolvable ancestor")?;
        let text = name.to_string_lossy();
        ensure!(
            !text.contains(':') && !text.ends_with(['.', ' ']),
            "missing target component is not a canonical file name"
        );
        missing.push(name.to_owned());
        cursor = cursor.parent().context("target has no parent")?;
    }
    let mut resolved = std::fs::canonicalize(cursor).context("resolve output target")?;
    for name in missing.into_iter().rev() {
        resolved.push(name);
    }
    Ok(resolved)
}

struct PinnedDirectory {
    path: PathBuf,
    file: File,
    identity: Identity,
}

fn pin(path: &Path) -> Result<PinnedDirectory> {
    let directory = OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES.0)
        .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0 | FILE_SHARE_DELETE.0)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(path)?;
    let metadata = directory.metadata()?;
    ensure!(
        metadata.is_dir() && metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 == 0,
        "publication parent is not an ordinary directory"
    );
    let identity = Identity::read(&directory)?;
    Ok(PinnedDirectory {
        path: path.to_owned(),
        file: directory,
        identity,
    })
}
