//! Durable notice for component files deliberately preserved during removal.

use std::collections::BTreeSet;
use std::io::Read as _;
use std::path::Path;
use std::sync::Mutex;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use crate::component_registry::{catalog::validate_identifier, receipt::is_reparse_point};

const MAX_BYTES: u64 = 64 * 1024;
const MAX_COMPONENTS: usize = 256;
static LOCK: Mutex<()> = Mutex::new(());

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Notices {
    schema_version: u32,
    preserved_component_ids: BTreeSet<String>,
}

impl Default for Notices {
    fn default() -> Self {
        Self {
            schema_version: 1,
            preserved_component_ids: BTreeSet::new(),
        }
    }
}

fn path() -> std::path::PathBuf {
    #[cfg(test)]
    return std::env::temp_dir().join(format!(
        "sgt-component-removal-notices-{}.json",
        std::process::id()
    ));
    #[cfg(not(test))]
    crate::paths::app_runtime_local_data_dir().join("component-removal-notices.json")
}

fn read(path: &Path) -> Result<Notices> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Notices::default()),
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_file() || is_reparse_point(&metadata) || metadata.len() > MAX_BYTES {
        bail!("component removal notice is unsafe");
    }
    let mut contents = String::new();
    std::fs::File::open(path)?
        .take(MAX_BYTES + 1)
        .read_to_string(&mut contents)?;
    let notices: Notices = serde_json::from_str(&contents)?;
    validate(&notices)?;
    Ok(notices)
}

fn validate(notices: &Notices) -> Result<()> {
    if notices.schema_version != 1 || notices.preserved_component_ids.len() > MAX_COMPONENTS {
        bail!("component removal notice is invalid");
    }
    for id in &notices.preserved_component_ids {
        validate_identifier(id)?;
        if !crate::component_registry::embedded_catalog().is_removable(id) {
            bail!("component removal notice names an unknown component");
        }
    }
    Ok(())
}

pub(crate) fn set_preserved(id: &str, preserved: bool) -> Result<()> {
    let _guard = LOCK.lock().unwrap_or_else(|error| error.into_inner());
    set_at(&path(), id, preserved)
}

fn set_at(path: &Path, id: &str, preserved: bool) -> Result<()> {
    validate_identifier(id)?;
    let mut notices = read(path)?;
    let changed = if preserved {
        notices.preserved_component_ids.insert(id.to_string())
    } else {
        notices.preserved_component_ids.remove(id)
    };
    if changed {
        validate(&notices)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        crate::atomic_json::write_json_atomic(path, &notices)?;
    }
    Ok(())
}

#[cfg(not(feature = "recorder-worker"))]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "offline-engine shell: no engine downloads models today; see docs/OFFLINE-ENGINES.md"
    )
)]
pub(crate) fn is_preserved(id: &str) -> Result<bool> {
    validate_identifier(id)?;
    let _guard = LOCK.lock().unwrap_or_else(|error| error.into_inner());
    Ok(read(&path())?.preserved_component_ids.contains(id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserved_outcome_survives_reload_and_success_clears_only_that_component() {
        let root =
            std::env::temp_dir().join(format!("sgt-removal-attention-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("notices.json");
        set_at(&file, "test-runtime-notice", true).unwrap();
        set_at(&file, "test-other-notice", true).unwrap();
        assert!(
            read(&file)
                .unwrap()
                .preserved_component_ids
                .contains("test-runtime-notice")
        );
        set_at(&file, "test-runtime-notice", false).unwrap();
        let reloaded = read(&file).unwrap();
        assert!(
            !reloaded
                .preserved_component_ids
                .contains("test-runtime-notice")
        );
        assert!(
            reloaded
                .preserved_component_ids
                .contains("test-other-notice")
        );
    }

    #[cfg(not(feature = "recorder-worker"))]
    #[test]
    fn replayed_preservation_is_recorded_before_pending_is_cleared() {
        let _mutation = crate::component_registry::acquire_mutation_guard().unwrap();
        let id = format!("test-preserved-replay-{}", std::process::id());
        crate::component_registry::pending::record(&id).unwrap();
        super::super::finish_pending(
            &id,
            &super::super::RemovalOutcome::PreservedModified(vec!["opaque".into()]),
        )
        .unwrap();
        assert!(
            !crate::component_registry::pending::list()
                .unwrap()
                .contains(&id)
        );
        assert!(is_preserved(&id).unwrap());
        // Reads deserialize the on-disk record each time, as a fresh process does.
        assert!(read(&path()).unwrap().preserved_component_ids.contains(&id));
        super::super::finish_pending(&id, &super::super::RemovalOutcome::Missing).unwrap();
        assert!(!is_preserved(&id).unwrap());
    }

    #[cfg(not(feature = "recorder-worker"))]
    #[test]
    fn queued_removal_reports_preserved_modified_bytes_after_use_ends() {
        use sha2::{Digest, Sha256};
        let mutation = crate::component_registry::acquire_mutation_guard().unwrap();
        let id = format!("test-preserved-queued-{}", std::process::id());
        let root = crate::component_registry::ensure_version_root(&id, "1.0.0").unwrap();
        let file = root.join("component.bin");
        std::fs::write(&file, b"modified bytes").unwrap();
        let receipt = crate::component_registry::ComponentReceipt {
            schema_version: 1,
            id: id.clone(),
            version: "1.0.0".to_string(),
            architecture: "x64".to_string(),
            dependencies: vec![],
            files: vec![crate::component_registry::OwnedComponentFile {
                path: "component.bin".into(),
                size_bytes: 14,
                sha256: format!("{:x}", Sha256::digest(b"original bytes")),
            }],
        };
        crate::component_registry::write_receipt(&root, &receipt).unwrap();
        let lease = crate::component_registry::acquire(&id).unwrap();
        drop(mutation);
        assert_eq!(
            crate::component_registry::request_remove(&id).unwrap(),
            super::super::RemovalOutcome::Pending
        );
        drop(lease);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let _mutation = crate::component_registry::acquire_mutation_guard().unwrap();
            let pending = crate::component_registry::pending::list()
                .unwrap()
                .contains(&id);
            let preserved = is_preserved(&id).unwrap();
            if !pending && preserved {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "preservation notice did not become durable"
            );
            drop(_mutation);
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(std::fs::read(&file).unwrap(), b"modified bytes");
    }
}
