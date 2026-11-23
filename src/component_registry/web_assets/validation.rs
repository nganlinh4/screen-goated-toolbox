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
    let actual = crate::component_registry::owned_tree::regular_files(
        root,
        "web asset package",
        MAX_ARCHIVE_ENTRIES + 1,
    )?
    .into_iter()
    .collect::<HashSet<_>>();
    if actual != expected {
        bail!("web asset package contains unowned files");
    }
    Ok(())
}
