use std::path::{Component, Path, PathBuf};

use anyhow::{Result, bail};

pub(super) fn ensure_directory_tree(root: &Path, target: &Path) -> Result<()> {
    std::fs::create_dir_all(root)?;
    require_directory(root)?;
    let relative = target
        .strip_prefix(root)
        .map_err(|_| anyhow::anyhow!("recorder staging path escaped its root"))?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            bail!("recorder staging path is unsafe");
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
        .ok_or_else(|| anyhow::anyhow!("recorder file has no parent"))?;
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
            bail!("recorder component has too many files");
        }
        let path = entry?.path();
        let metadata = std::fs::symlink_metadata(&path)?;
        if super::super::receipt::is_reparse_point(&metadata) {
            bail!("recorder component contains a reparse point");
        }
        if metadata.is_dir() {
            collect_regular_files(root, &path, files, maximum)?;
        } else if metadata.is_file() {
            files.push(path.strip_prefix(root)?.to_path_buf());
        } else {
            bail!("recorder component contains an unsafe entry");
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
    if !metadata.is_dir() || super::super::receipt::is_reparse_point(&metadata) {
        bail!("recorder staging root is unsafe");
    }
    let mut parents = Vec::new();
    for relative in owned {
        super::super::receipt::validate_relative_path(relative)?;
        let path = checked_path(root, relative)?;
        match std::fs::symlink_metadata(&path) {
            Ok(metadata)
                if metadata.is_file() && !super::super::receipt::is_reparse_point(&metadata) =>
            {
                std::fs::remove_file(&path)?;
            }
            Ok(_) => bail!("recorder staging cleanup preserved an unsafe owned path"),
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
            bail!("recorder cleanup path is unsafe");
        };
        current.push(name);
        match std::fs::symlink_metadata(&current) {
            Ok(metadata)
                if metadata.is_dir() && !super::super::receipt::is_reparse_point(&metadata) => {}
            Ok(_) => bail!("recorder cleanup parent is unsafe"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(error.into()),
        }
    }
    Ok(root.join(relative))
}

fn require_directory(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_dir() || super::super::receipt::is_reparse_point(&metadata) {
        bail!("recorder staging directory is unsafe");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleanup_preserves_unknown_files() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "sgt-recorder-staging-test-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("owned.bin"), b"owned").unwrap();
        std::fs::write(root.join("unknown.bin"), b"unknown").unwrap();
        cleanup_owned(&root, &["owned.bin".into()]).unwrap();
        assert!(!root.join("owned.bin").exists());
        assert_eq!(std::fs::read(root.join("unknown.bin")).unwrap(), b"unknown");
        std::fs::remove_file(root.join("unknown.bin")).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
