//! Reparse-safe bounded staging helpers shared by managed component installers.

pub(super) use crate::component_registry::directory::{
    ensure_directory_tree, prepare_target_create as prepare_target,
};
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

pub(super) use super::owned_tree::cleanup_owned;

const MAX_TREE_DEPTH: usize = 8;

pub(super) struct TreeInventory {
    pub(super) files: Vec<PathBuf>,
    pub(super) directories: Vec<PathBuf>,
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
        bail!("component staging tree is too deep");
    }
    for entry in std::fs::read_dir(directory)? {
        *entries = entries
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("component staging entry count overflow"))?;
        if *entries > maximum_entries {
            bail!("component staging tree has too many entries");
        }
        let path = entry?.path();
        let metadata = std::fs::symlink_metadata(&path)?;
        if super::receipt::is_reparse_point(&metadata) {
            bail!("component staging tree contains a reparse point");
        }
        let relative = path.strip_prefix(root)?.to_path_buf();
        if metadata.is_dir() {
            inventory.directories.push(relative);
            collect_tree_inner(root, &path, inventory, entries, maximum_entries, depth + 1)?;
        } else if metadata.is_file() {
            inventory.files.push(relative);
        } else {
            bail!("component staging tree contains an unsafe entry");
        }
    }
    Ok(())
}

fn require_directory(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_dir() || super::receipt::is_reparse_point(&metadata) {
        bail!("component staging directory is unsafe");
    }
    Ok(())
}
