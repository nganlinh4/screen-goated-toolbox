use std::path::{Component, Path, PathBuf};

use anyhow::{Result, bail};

use crate::component_registry::receipt::{is_reparse_point, validate_relative_path};

pub(super) fn ensure_directory_tree(root: &Path, target: &Path) -> Result<()> {
    std::fs::create_dir_all(root)?;
    require_directory(root)?;
    let relative = target
        .strip_prefix(root)
        .map_err(|_| anyhow::anyhow!("external tool staging path escaped its root"))?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            bail!("external tool staging path is unsafe");
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
    validate_relative_path(relative)?;
    let target = root.join(relative);
    let parent = target
        .parent()
        .ok_or_else(|| anyhow::anyhow!("external tool file has no parent"))?;
    ensure_directory_tree(root, parent)?;
    Ok(target)
}

pub(super) fn collect_regular_files(
    root: &Path,
    directory: &Path,
    files: &mut Vec<PathBuf>,
    maximum: usize,
) -> Result<()> {
    const MAX_DIRECTORIES: usize = 64;
    const MAX_DEPTH: usize = 32;
    let mut pending = vec![directory.to_path_buf()];
    let mut directories = 0_usize;
    while let Some(current) = pending.pop() {
        directories += 1;
        if directories > MAX_DIRECTORIES {
            bail!("external tool component has too many directories");
        }
        let relative = current.strip_prefix(root)?;
        if relative.components().count() > MAX_DEPTH {
            bail!("external tool component directory nesting is too deep");
        }
        require_directory(&current)?;
        for entry in std::fs::read_dir(&current)? {
            let path = entry?.path();
            let metadata = std::fs::symlink_metadata(&path)?;
            if is_reparse_point(&metadata) {
                bail!("external tool component contains a reparse point");
            }
            if metadata.is_dir() {
                if directories + pending.len() >= MAX_DIRECTORIES {
                    bail!("external tool component has too many directories");
                }
                pending.push(path);
            } else if metadata.is_file() {
                if files.len() >= maximum {
                    bail!("external tool component has too many files");
                }
                files.push(path.strip_prefix(root)?.to_path_buf());
            } else {
                bail!("external tool component contains an unsafe entry");
            }
        }
    }
    Ok(())
}

pub(super) fn cleanup_owned(root: &Path, owned: &[PathBuf]) -> Result<()> {
    let metadata = match std::fs::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        bail!("external tool staging root is unsafe");
    }
    let mut parents = Vec::new();
    for relative in owned {
        validate_relative_path(relative)?;
        let path = checked_path(root, relative)?;
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_file() && !is_reparse_point(&metadata) => {
                std::fs::remove_file(&path)?;
            }
            Ok(_) => bail!("external tool staging cleanup found an unsafe owned path"),
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

fn checked_path(root: &Path, relative: &Path) -> Result<PathBuf> {
    let mut current = root.to_path_buf();
    let components = relative.components().collect::<Vec<_>>();
    for component in components.iter().take(components.len().saturating_sub(1)) {
        let Component::Normal(name) = component else {
            bail!("external tool cleanup path is unsafe");
        };
        current.push(name);
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.is_dir() && !is_reparse_point(&metadata) => {}
            Ok(_) => bail!("external tool cleanup parent is unsafe"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(error.into()),
        }
    }
    Ok(root.join(relative))
}

fn require_directory(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        bail!("external tool staging directory is unsafe");
    }
    Ok(())
}
