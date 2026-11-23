use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

use super::*;
use crate::component_registry::receipt::{ComponentReceipt, RECEIPT_NAME, file_size_matches};

pub(super) fn validate_install(delivery: &CreationDelivery) -> Result<()> {
    validate_receipt(delivery, file_matches)
}

pub(super) fn validate_status(delivery: &CreationDelivery) -> Result<()> {
    validate_receipt(delivery, file_size_matches)
}

fn validate_receipt(
    delivery: &CreationDelivery,
    matcher: fn(&Path, &OwnedComponentFile) -> Result<bool>,
) -> Result<()> {
    let root = super::super::validate_version_root(COMPONENT_ID, delivery.version)?;
    let receipt = ComponentReceipt::read(&root.join(RECEIPT_NAME))?;
    if receipt.id != COMPONENT_ID
        || receipt.version != delivery.version
        || receipt.architecture != ARCHITECTURE
        || !receipt.dependencies.is_empty()
        || receipt.files.len() != delivery.files.len()
    {
        bail!("Creation receipt does not match this build");
    }
    for (actual, expected) in receipt.files.iter().zip(delivery.files) {
        let owned = owned_file(expected);
        if actual.path != owned.path
            || actual.size_bytes != owned.size_bytes
            || !actual.sha256.eq_ignore_ascii_case(&owned.sha256)
        {
            bail!("Creation receipt inventory does not match this build");
        }
        let path = resolve_owned_path(&root, &owned.path)?;
        if !matcher(&path, &owned)? {
            bail!("installed Creation file failed integrity verification");
        }
    }
    validate_exact_tree(&root, delivery)
}

pub(super) fn validate_exact_tree(root: &Path, delivery: &CreationDelivery) -> Result<()> {
    let mut expected = delivery
        .files
        .iter()
        .map(|file| PathBuf::from(file.path))
        .collect::<HashSet<_>>();
    expected.insert(RECEIPT_NAME.into());
    let actual = crate::component_registry::owned_tree::regular_files(
        root,
        "Creation package",
        MAX_ARCHIVE_ENTRIES + 1,
    )?
    .into_iter()
    .collect::<HashSet<_>>();
    if actual != expected {
        bail!("Creation package contains unowned files");
    }
    Ok(())
}
