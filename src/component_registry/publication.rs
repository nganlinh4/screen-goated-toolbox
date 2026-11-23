use std::path::Path;

use anyhow::{Result, bail};

use super::lease::{ComponentLease, upgrade_to_exclusive};
use super::receipt::OwnedComponentFile;

pub(super) fn publish_verified(
    id: &str,
    version: &str,
    files: &[OwnedComponentFile],
    stage: &Path,
    lease: &mut ComponentLease,
) -> Result<()> {
    let target = super::component_version_root(id, version)?;
    let parent = super::ensure_component_parent(id)?;
    if target.parent() != Some(parent.as_path()) {
        bail!("component install target escaped its component directory");
    }
    match std::fs::symlink_metadata(&target) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            super::recovery::no_replace_move(stage, &target)
        }
        Err(error) => Err(error.into()),
        Ok(_) => {
            let exclusive = upgrade_to_exclusive(lease)?;
            super::recovery::publish_verified(
                id,
                version,
                files,
                "component integrity validation failed",
                stage,
                &exclusive,
            )
        }
    }
}
