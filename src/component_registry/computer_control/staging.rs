use std::path::{Component, Path, PathBuf};

use anyhow::{Result, bail};

const MAX_TREE_DEPTH: usize = 8;

pub(super) struct TreeInventory {
    pub(super) files: Vec<PathBuf>,
    pub(super) directories: Vec<PathBuf>,
}

pub(super) fn ensure_directory_tree(root: &Path, target: &Path) -> Result<()> {
    std::fs::create_dir_all(root)?;
    require_directory(root)?;
    let relative = target
        .strip_prefix(root)
        .map_err(|_| anyhow::anyhow!("Computer Control staging path escaped its root"))?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            bail!("Computer Control staging path is unsafe");
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
        .ok_or_else(|| anyhow::anyhow!("Computer Control engine file has no parent"))?;
    ensure_directory_tree(root, parent)?;
    Ok(target)
}

pub(super) fn collect_tree(root: &Path, maximum_entries: usize) -> Result<TreeInventory> {
    require_directory(root)?;
    let mut inventory = TreeInventory {
        files: Vec::new(),
        directories: Vec::new(),
    };
    let mut entries = 0_usize;
    collect_tree_inner(root, root, &mut inventory, &mut entries, maximum_entries, 0)?;
    Ok(inventory)
}

fn collect_tree_inner(
    root: &Path,
    directory: &Path,
    inventory: &mut TreeInventory,
    entries: &mut usize,
    maximum_entries: usize,
    depth: usize,
) -> Result<()> {
    if depth > MAX_TREE_DEPTH {
        bail!("Computer Control engine tree is too deep");
    }
    for entry in std::fs::read_dir(directory)? {
        *entries = entries
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("Computer Control engine entry count overflow"))?;
        if *entries > maximum_entries {
            bail!("Computer Control engine has too many entries");
        }
        let path = entry?.path();
        let metadata = std::fs::symlink_metadata(&path)?;
        if super::super::receipt::is_reparse_point(&metadata) {
            bail!("Computer Control engine contains a reparse point");
        }
        let relative = path.strip_prefix(root)?.to_path_buf();
        if metadata.is_dir() {
            inventory.directories.push(relative);
            collect_tree_inner(root, &path, inventory, entries, maximum_entries, depth + 1)?;
        } else if metadata.is_file() {
            inventory.files.push(relative);
        } else {
            bail!("Computer Control engine contains an unsafe entry");
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
        bail!("Computer Control staging root is unsafe");
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
            Ok(_) => bail!("Computer Control staging cleanup found an unsafe owned path"),
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
            bail!("Computer Control cleanup path is unsafe");
        };
        current.push(name);
        match std::fs::symlink_metadata(&current) {
            Ok(metadata)
                if metadata.is_dir() && !super::super::receipt::is_reparse_point(&metadata) => {}
            Ok(_) => bail!("Computer Control cleanup parent is unsafe"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(error.into()),
        }
    }
    Ok(root.join(relative))
}

fn require_directory(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_dir() || super::super::receipt::is_reparse_point(&metadata) {
        bail!("Computer Control staging directory is unsafe");
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
            "sgt-cc-staging-test-{}-{unique}",
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

    #[test]
    fn exact_inventory_rejects_unknown_empty_directories() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("sgt-cc-tree-test-{}-{unique}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(root.join("bin")).unwrap();
        std::fs::write(root.join("bin/engine.exe"), b"engine").unwrap();
        std::fs::create_dir(root.join("unknown")).unwrap();
        let files = [super::super::EngineFile {
            path: "bin/engine.exe",
            size_bytes: 6,
            sha256: "unused",
        }];

        assert!(super::super::validate_exact_tree(&root, &files, false).is_err());

        std::fs::remove_file(root.join("bin/engine.exe")).unwrap();
        std::fs::remove_dir(root.join("bin")).unwrap();
        std::fs::remove_dir(root.join("unknown")).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
