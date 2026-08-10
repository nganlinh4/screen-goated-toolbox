use std::path::Path;

use anyhow::{Result, bail};

use super::*;

pub(super) fn validate_delivery(delivery: &LocalAsrDelivery) -> Result<()> {
    validate_install_root(
        &version_root(delivery.id, delivery.version)?,
        delivery.id,
        delivery.version,
        delivery.files,
        &ComponentKind::from_id(delivery.id)?.dependencies(),
    )
}

pub(super) fn validate_delivery_status(delivery: &LocalAsrDelivery) -> Result<()> {
    validate_component_root(
        &version_root(delivery.id, delivery.version)?,
        delivery.id,
        delivery.version,
        delivery.files,
        &ComponentKind::from_id(delivery.id)?.dependencies(),
        file_size_matches,
    )
}

fn validate_install_root(
    root: &Path,
    id: &str,
    version: &str,
    files: &[LocalAsrFile],
    dependencies: &[String],
) -> Result<()> {
    validate_component_root(root, id, version, files, dependencies, file_matches)
}

fn validate_component_root(
    root: &Path,
    id: &str,
    version: &str,
    files: &[LocalAsrFile],
    dependencies: &[String],
    matches: fn(&Path, &OwnedComponentFile) -> Result<bool>,
) -> Result<()> {
    let metadata = std::fs::symlink_metadata(root)?;
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        bail!("local ASR component root is unsafe");
    }
    let receipt = ComponentReceipt::read(&root.join(RECEIPT_NAME))?;
    if receipt.id != id
        || receipt.version != version
        || receipt.architecture != ARCHITECTURE
        || receipt.dependencies != dependencies
        || receipt.files.len() != files.len()
    {
        bail!("local ASR component receipt does not match this build");
    }
    for expected in files {
        let owned = owned_file(expected);
        if !receipt.files.iter().any(|file| same_file(file, &owned))
            || !matches(&resolve_owned_path(root, Path::new(expected.path))?, &owned)?
        {
            bail!("local ASR component file failed integrity verification");
        }
    }
    validate_exact_tree(root, files)
}

pub(super) fn validate_exact_tree(root: &Path, files: &[LocalAsrFile]) -> Result<()> {
    let mut actual = Vec::new();
    staging::collect_regular_files(root, root, &mut actual, MAX_COMPONENT_FILES + 1)?;
    actual.retain(|path| path != Path::new(RECEIPT_NAME));
    if actual.len() != files.len()
        || actual
            .iter()
            .any(|path| !files.iter().any(|file| Path::new(file.path) == path))
    {
        bail!("local ASR component contains unowned files");
    }
    Ok(())
}

pub(super) fn owned_file(file: &LocalAsrFile) -> OwnedComponentFile {
    OwnedComponentFile {
        path: file.path.into(),
        size_bytes: file.size_bytes,
        sha256: file.sha256.to_string(),
    }
}

fn same_file(left: &OwnedComponentFile, right: &OwnedComponentFile) -> bool {
    left.path == right.path
        && left.size_bytes == right.size_bytes
        && left.sha256.eq_ignore_ascii_case(&right.sha256)
}
