use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

use super::*;

pub(super) fn validate_install(delivery: &WebAssetDelivery) -> Result<()> {
    validate_receipt(delivery, file_matches)
}

pub(super) fn validate_status(delivery: &WebAssetDelivery) -> Result<()> {
    validate_receipt(delivery, file_size_matches)
}

fn validate_receipt(
    delivery: &WebAssetDelivery,
    matcher: fn(&Path, &OwnedComponentFile) -> Result<bool>,
) -> Result<()> {
    let root = super::super::validate_version_root(delivery.component.id(), delivery.version)?;
    let receipt = ComponentReceipt::read(&root.join(RECEIPT_NAME))?;
    let dependencies = delivery
        .component
        .dependencies()
        .iter()
        .map(|value| (*value).to_string())
        .collect::<Vec<_>>();
    if receipt.id != delivery.component.id()
        || receipt.version != delivery.version
        || receipt.architecture != ARCHITECTURE
        || receipt.dependencies != dependencies
        || receipt.files.len() != delivery.files.len()
    {
        bail!("web asset receipt does not match this build");
    }
    for (receipt_file, expected) in receipt.files.iter().zip(delivery.files) {
        let owned = owned_file(expected);
        if receipt_file.path != owned.path
            || receipt_file.size_bytes != owned.size_bytes
            || !receipt_file.sha256.eq_ignore_ascii_case(&owned.sha256)
        {
            bail!("web asset receipt file does not match this build");
        }
        let path = resolve_owned_path(&root, &owned.path)?;
        if !matcher(&path, &owned)? {
            bail!("installed web asset failed integrity verification");
        }
    }
    validate_exact_tree(&root, delivery)
}

pub(super) fn validate_exact_tree(root: &Path, delivery: &WebAssetDelivery) -> Result<()> {
    let mut expected = delivery
        .files
        .iter()
        .map(|file| PathBuf::from(file.path))
        .collect::<HashSet<_>>();
    expected.insert(PathBuf::from(RECEIPT_NAME));
    let mut actual = HashSet::new();
    collect_regular_files(root, root, &mut actual, 0)?;
    if actual != expected {
        bail!("web asset package contains unowned files");
    }
    Ok(())
}

fn collect_regular_files(
    root: &Path,
    directory: &Path,
    files: &mut HashSet<PathBuf>,
    depth: usize,
) -> Result<()> {
    if depth > 16 || files.len() > MAX_ARCHIVE_ENTRIES {
        bail!("web asset package exceeds traversal limits");
    }
    let metadata = std::fs::symlink_metadata(directory)?;
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        bail!("web asset package contains an unsafe directory");
    }
    for entry in std::fs::read_dir(directory)? {
        if files.len() >= MAX_ARCHIVE_ENTRIES {
            bail!("web asset package exceeds traversal limits");
        }
        let path = entry?.path();
        let metadata = std::fs::symlink_metadata(&path)?;
        if metadata.is_dir() && !is_reparse_point(&metadata) {
            collect_regular_files(root, &path, files, depth + 1)?;
        } else if metadata.is_file() && !is_reparse_point(&metadata) {
            files.insert(path.strip_prefix(root)?.to_path_buf());
        } else {
            bail!("web asset package contains an unsafe entry");
        }
    }
    Ok(())
}
