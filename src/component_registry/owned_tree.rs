//! The one bounded, reparse-safe walk and cleanup of a component's file tree.
//!
//! Every installer inventories and cleans its staged or installed files here,
//! so a fix to reparse handling or traversal bounds reaches all of them.

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

use anyhow::{Result, bail};

use super::receipt::{is_reparse_point, resolve_owned_path};

/// No component package nests deeper than this.
const MAX_DEPTH: usize = 16;

/// Every regular file under `root`, relative to it.
///
/// The walk refuses reparse points and anything that is neither a file nor a
/// directory, stops at `max_files` files, at as many directories, and at
/// [`MAX_DEPTH`] levels, so a hostile tree cannot make it unbounded. `label`
/// names the component in errors.
pub(super) fn regular_files(root: &Path, label: &str, max_files: usize) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut pending = vec![(root.to_path_buf(), 0_usize)];
    let mut directories = 0_usize;
    while let Some((directory, depth)) = pending.pop() {
        if depth > MAX_DEPTH {
            bail!("{label} directory nesting is too deep");
        }
        directories += 1;
        if directories > max_files {
            bail!("{label} has too many directories");
        }
        let metadata = std::fs::symlink_metadata(&directory)?;
        if !metadata.is_dir() || is_reparse_point(&metadata) {
            bail!("{label} contains an unsafe directory");
        }
        for entry in std::fs::read_dir(&directory)? {
            let path = entry?.path();
            let metadata = std::fs::symlink_metadata(&path)?;
            if is_reparse_point(&metadata) {
                bail!("{label} contains a reparse point");
            }
            if metadata.is_dir() {
                if directories + pending.len() >= max_files {
                    bail!("{label} has too many directories");
                }
                pending.push((path, depth + 1));
            } else if metadata.is_file() {
                if files.len() >= max_files {
                    bail!("{label} has too many files");
                }
                files.push(path.strip_prefix(root)?.to_path_buf());
            } else {
                bail!("{label} contains an unsafe entry");
            }
        }
    }
    Ok(files)
}

/// Removes the listed owned files under `root`, then whichever of their parent
/// directories and `root` itself are left empty. Unlisted files are kept, and
/// an owned path that is no longer a regular file stops the cleanup.
pub(super) fn cleanup_owned(root: &Path, owned: &[PathBuf]) -> Result<()> {
    let metadata = match std::fs::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        bail!("component staging root is unsafe");
    }
    let mut parents = Vec::new();
    for relative in owned {
        super::receipt::validate_relative_path(relative)?;
        let path = checked_path(root, relative)?;
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_file() && !is_reparse_point(&metadata) => {
                std::fs::remove_file(&path)?;
            }
            Ok(_) => bail!("component staging cleanup found an unsafe owned path"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let mut parent = path.parent();
        while let Some(directory) = parent {
            if directory == root {
                break;
            }
            parents.push(directory.to_path_buf());
            parent = directory.parent();
        }
    }
    parents.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    parents.dedup();
    for parent in parents {
        let _ = std::fs::remove_dir(parent);
    }
    let _ = std::fs::remove_dir(root);
    Ok(())
}

/// Best-effort removal of the listed owned files under `root`, then of the
/// directories that held them and of `root` once they are empty. A listed path
/// that escapes `root` or is no longer a regular file is left alone, as is
/// every unlisted file.
pub(super) fn remove_listed(root: &Path, relative_paths: &[&str]) -> Result<()> {
    require_regular_directory(root)?;
    let mut directories = HashSet::new();
    for relative in relative_paths {
        let relative = Path::new(relative);
        if let Ok(path) = resolve_owned_path(root, relative)
            && let Ok(metadata) = std::fs::symlink_metadata(&path)
            && metadata.is_file()
            && !is_reparse_point(&metadata)
        {
            std::fs::remove_file(&path)?;
        }
        let mut parent = relative.parent();
        while let Some(directory) = parent {
            if directory.as_os_str().is_empty() {
                break;
            }
            directories.insert(directory.to_path_buf());
            parent = directory.parent();
        }
    }
    let mut directories = directories.into_iter().collect::<Vec<_>>();
    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for relative in directories {
        let path = root.join(relative);
        if require_regular_directory(&path).is_ok() {
            remove_empty_directory(&path)?;
        }
    }
    require_regular_directory(root)?;
    remove_empty_directory(root)
}

fn remove_empty_directory(path: &Path) -> Result<()> {
    match std::fs::remove_dir(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::DirectoryNotEmpty => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn require_regular_directory(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        bail!("component staging path is not a regular directory");
    }
    Ok(())
}

/// `root` joined with `relative`, refusing a parent directory on the way that
/// is a reparse point or not a directory.
fn checked_path(root: &Path, relative: &Path) -> Result<PathBuf> {
    let mut current = root.to_path_buf();
    let components = relative.components().collect::<Vec<_>>();
    for component in components.iter().take(components.len().saturating_sub(1)) {
        let Component::Normal(name) = component else {
            bail!("component staging cleanup path is unsafe");
        };
        current.push(name);
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.is_dir() && !is_reparse_point(&metadata) => {}
            Ok(_) => bail!("component staging cleanup parent is unsafe"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(error.into()),
        }
    }
    Ok(root.join(relative))
}

#[cfg(test)]
mod tests;
