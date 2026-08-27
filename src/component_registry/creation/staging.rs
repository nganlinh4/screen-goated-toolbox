use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

use anyhow::{Result, bail};

use crate::component_registry::receipt::{
    is_reparse_point, resolve_owned_path, validate_relative_path,
};

pub(super) fn prepare_target(root: &Path, relative: &Path) -> Result<PathBuf> {
    validate_relative_path(relative)?;
    validate_directory(root)?;
    let mut current = root.to_path_buf();
    if let Some(parent) = relative.parent() {
        for component in parent.components() {
            let Component::Normal(name) = component else {
                bail!("Creation archive path is unsafe")
            };
            current.push(name);
            match std::fs::create_dir(&current) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error.into()),
            }
            validate_directory(&current)?;
        }
    }
    resolve_owned_path(root, relative)
}

pub(super) fn cleanup_owned(root: &Path, paths: &[&str]) -> Result<()> {
    validate_directory(root)?;
    let mut directories = HashSet::new();
    for relative in paths {
        let relative = Path::new(relative);
        if let Ok(path) = resolve_owned_path(root, relative)
            && let Ok(metadata) = std::fs::symlink_metadata(&path)
            && metadata.is_file()
            && !is_reparse_point(&metadata)
        {
            std::fs::remove_file(path)?;
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
        if validate_directory(&path).is_ok() {
            let _ = std::fs::remove_dir(path);
        }
    }
    let _ = std::fs::remove_dir(root);
    Ok(())
}

fn validate_directory(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        bail!("Creation staging path is unsafe")
    }
    Ok(())
}
