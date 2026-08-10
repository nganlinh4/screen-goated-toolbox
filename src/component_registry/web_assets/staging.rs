use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

use anyhow::{Result, bail};

use crate::component_registry::receipt::{
    is_reparse_point, resolve_owned_path, validate_relative_path,
};

pub(super) fn prepare_target(root: &Path, relative: &Path) -> Result<PathBuf> {
    validate_relative_path(relative)?;
    validate_regular_directory(root)?;
    let mut current = root.to_path_buf();
    if let Some(parent) = relative.parent() {
        for component in parent.components() {
            let Component::Normal(name) = component else {
                bail!("web asset parent path is unsafe");
            };
            current.push(name);
            match std::fs::create_dir(&current) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error.into()),
            }
            validate_regular_directory(&current)?;
        }
    }
    resolve_owned_path(root, relative)
}

pub(super) fn cleanup_owned(root: &Path, relative_paths: &[&str]) -> Result<()> {
    validate_regular_directory(root)?;
    let mut directories = HashSet::new();
    for relative in relative_paths {
        let relative = Path::new(relative);
        let path = match resolve_owned_path(root, relative) {
            Ok(path) => path,
            Err(_) => continue,
        };
        if let Ok(metadata) = std::fs::symlink_metadata(&path)
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
        if validate_regular_directory(&path).is_ok() {
            match std::fs::remove_dir(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::DirectoryNotEmpty => {}
                Err(error) => return Err(error.into()),
            }
        }
    }
    validate_regular_directory(root)?;
    match std::fs::remove_dir(root) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::DirectoryNotEmpty => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn validate_regular_directory(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        bail!("web asset staging path is not a regular directory");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parent_creation_rejects_a_non_directory_component() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "sgt-web-staging-parent-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("assets"), b"not a directory").unwrap();
        assert!(prepare_target(&root, Path::new("assets/index.js")).is_err());
        std::fs::remove_file(root.join("assets")).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
