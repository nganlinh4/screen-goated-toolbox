use std::path::{Component, Path, PathBuf};

use anyhow::{Result, bail};

pub(super) fn ensure_directory_tree(root: &Path, target: &Path) -> Result<()> {
    std::fs::create_dir_all(root)?;
    require_directory(root)?;
    let relative = target
        .strip_prefix(root)
        .map_err(|_| anyhow::anyhow!("model staging path escaped its root"))?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            bail!("model staging path is unsafe");
        };
        current.push(name);
        match std::fs::create_dir(&current) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
        require_directory(&current)?;
    }
    Ok(())
}

pub(super) fn prepare_target(root: &Path, relative: &Path) -> Result<PathBuf> {
    super::super::receipt::validate_relative_path(relative)?;
    let target = root.join(relative);
    let parent = target
        .parent()
        .ok_or_else(|| anyhow::anyhow!("model file has no parent"))?;
    ensure_directory_tree(root, parent)?;
    Ok(target)
}

pub(super) fn collect_regular_files(
    root: &Path,
    directory: &Path,
    files: &mut Vec<PathBuf>,
    maximum: usize,
) -> Result<()> {
    for entry in std::fs::read_dir(directory)? {
        if files.len() >= maximum {
            bail!("model component has too many files");
        }
        let path = entry?.path();
        let metadata = std::fs::symlink_metadata(&path)?;
        if super::super::receipt::is_reparse_point(&metadata) {
            bail!("model component contains a reparse point");
        }
        if metadata.is_dir() {
            collect_regular_files(root, &path, files, maximum)?;
        } else if metadata.is_file() {
            files.push(
                path.strip_prefix(root)
                    .map_err(|_| anyhow::anyhow!("model file escaped its root"))?
                    .to_path_buf(),
            );
        } else {
            bail!("model component contains an unsafe entry");
        }
    }
    Ok(())
}

pub(super) fn remove_empty_parents(mut parent: Option<&Path>, stop: &Path) -> Result<()> {
    while let Some(directory) = parent {
        if directory == stop {
            break;
        }
        match std::fs::remove_dir(directory) {
            Ok(()) => parent = directory.parent(),
            Err(error) if error.kind() == std::io::ErrorKind::DirectoryNotEmpty => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                parent = directory.parent()
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn require_directory(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_dir() || super::super::receipt::is_reparse_point(&metadata) {
        bail!("model staging directory is unsafe");
    }
    Ok(())
}
