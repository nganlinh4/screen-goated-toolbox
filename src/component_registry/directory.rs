//! Checked directory creation and retained parent guards for payload writes.

use std::fs::{File, OpenOptions};
use std::os::windows::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};

use anyhow::{Result, ensure};
use windows::Win32::Storage::FileSystem::{
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE,
};

use super::receipt::{is_reparse_point, validate_relative_path};

pub(super) fn open_directory(path: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0 | FILE_FLAG_BACKUP_SEMANTICS.0)
        .open(path)?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_dir() && !is_reparse_point(&metadata),
        "component directory is unsafe"
    );
    Ok(file)
}

pub(super) fn lock_ancestors(root: &Path) -> Result<Vec<File>> {
    ensure!(root.is_absolute(), "component root is not absolute");
    let mut paths = root.ancestors().collect::<Vec<_>>();
    paths.reverse();
    paths.into_iter().map(open_directory).collect()
}

pub(super) fn ensure_directory(path: &Path) -> Result<Vec<File>> {
    ensure!(path.is_absolute(), "component directory is not absolute");
    let mut paths = path.ancestors().collect::<Vec<_>>();
    paths.reverse();
    let mut guards = Vec::with_capacity(paths.len());
    for path in paths {
        create_directory(path)?;
        guards.push(open_directory(path)?);
    }
    Ok(guards)
}

/// Creation only; payload writers retain the guards returned by `prepare_target`.
pub(super) fn ensure_directory_tree(root: &Path, target: &Path) -> Result<()> {
    let relative = target
        .strip_prefix(root)
        .map_err(|_| anyhow::anyhow!("component directory escaped its root"))?;
    if !relative.as_os_str().is_empty() {
        validate_relative_path(relative)?;
    }
    drop(ensure_directory(target)?);
    Ok(())
}

/// Existing-root policy: the caller must have created/admitted its staging root.
pub(super) fn prepare_target(root: &Path, relative: &Path) -> Result<(PathBuf, Vec<File>)> {
    validate_relative_path(relative)?;
    prepare(root, relative, lock_ancestors(root)?)
}

/// Creation policy for installers which also own a not-yet-created root.
pub(super) fn prepare_target_create(root: &Path, relative: &Path) -> Result<(PathBuf, Vec<File>)> {
    validate_relative_path(relative)?;
    prepare(root, relative, ensure_directory(root)?)
}

fn prepare(root: &Path, relative: &Path, mut guards: Vec<File>) -> Result<(PathBuf, Vec<File>)> {
    let mut current = root.to_path_buf();
    if let Some(parent) = relative.parent() {
        for part in parent.components() {
            let std::path::Component::Normal(name) = part else {
                anyhow::bail!("component parent path is unsafe");
            };
            current.push(name);
            create_directory(&current)?;
            guards.push(open_directory(&current)?);
        }
    }
    Ok((root.join(relative), guards))
}

fn create_directory(path: &Path) -> Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => return Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    match std::fs::create_dir(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
#[path = "directory_tests.rs"]
mod tests;
