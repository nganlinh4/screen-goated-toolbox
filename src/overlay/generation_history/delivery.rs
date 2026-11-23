use super::*;
use sha2::{Digest as _, Sha256};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DeliveryArtifactIdentity {
    pub size_bytes: u64,
    pub sha256: String,
    pub file_identity: String,
    pub managed: bool,
}

pub(crate) fn inspect_delivery_artifact(
    output_path: &str,
) -> Result<DeliveryArtifactIdentity, String> {
    let path = Path::new(output_path);
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| format!("Could not inspect artifact: {error}"))?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_HISTORY_ARTIFACT_BYTES {
        return Err("Artifact is not a supported regular file.".to_string());
    }
    let mut file = std::fs::File::open(path)
        .map_err(|error| format!("Could not inspect artifact: {error}"))?;
    let file_identity = crate::overlay::creation_file_identity::from_file(&file)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut read_total = 0_u64;
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("Could not inspect artifact: {error}"))?;
        if read == 0 {
            break;
        }
        read_total = read_total.saturating_add(read as u64);
        if read_total > metadata.len() || read_total > MAX_HISTORY_ARTIFACT_BYTES {
            return Err("Artifact changed while it was being inspected.".to_string());
        }
        digest.update(&buffer[..read]);
    }
    if read_total != metadata.len()
        || crate::overlay::creation_file_identity::from_file(&file)? != file_identity
    {
        return Err("Artifact changed while it was being inspected.".to_string());
    }
    Ok(DeliveryArtifactIdentity {
        size_bytes: read_total,
        sha256: format!("{:x}", digest.finalize()),
        file_identity,
        managed: is_managed_artifact(path),
    })
}

pub(crate) fn has_delivery_receipt(
    tool: &str,
    dispatch_id: &str,
    output_path: &str,
) -> Result<bool, String> {
    let _guard = locking::acquire()?;
    let store = load_store(&history_path())?;
    Ok(store_has_delivery_receipt(
        &store,
        tool,
        dispatch_id,
        output_path,
    ))
}

fn store_has_delivery_receipt(
    store: &ResultHistoryStore,
    tool: &str,
    dispatch_id: &str,
    output_path: &str,
) -> bool {
    store.delivery_identities.iter().any(|identity| {
        identity.tool == tool
            && identity.dispatch_id == dispatch_id
            && store.entries.iter().any(|entry| {
                entry.id == identity.entry_id
                    && entry.tool == tool
                    && same_path(&entry.output_path, output_path)
            })
    })
}

pub(crate) fn record_delivery(
    tool: &str,
    dispatch_id: &str,
    source_path: &str,
    output_path: &str,
    metadata: Value,
    artifact: &DeliveryArtifactIdentity,
    protected_paths: &std::collections::HashSet<String>,
) -> Result<ResultHistoryEntry, String> {
    let results_per_tool = configuration::results_per_tool_for_delivery();
    let path = history_path();
    let mut pending =
        protected::ProtectedPaths::from_saved_paths(protected_paths.iter().map(PathBuf::from))?;
    pending.extend(&protected::ProtectedPaths::pending()?);
    let entry = {
        let _guard = locking::acquire()?;
        let protected_paths = pending.expanded(&load_store(&path)?)?;
        record_at_protected(
            &path,
            tool,
            source_path,
            output_path,
            metadata,
            RecordOptions {
                results_per_tool,
                inspected_artifact: Some((
                    artifact.size_bytes,
                    artifact.sha256.clone(),
                    artifact.managed,
                )),
                delivery: Some(DeliveryIdentity {
                    dispatch_id,
                    artifact_file_identity: &artifact.file_identity,
                }),
                protected_paths: &protected_paths,
            },
        )?
    };
    schedule_maintenance(path);
    Ok(entry)
}

#[cfg(test)]
mod receipt_tests {
    use super::*;

    #[test]
    fn completion_receipt_requires_the_same_dispatch_and_live_history_entry() {
        let mut store = ResultHistoryStore::default();
        store.entries.push(ResultHistoryEntry {
            id: "entry".to_string(),
            tool: "3d".to_string(),
            source_path: "source.png".to_string(),
            output_path: "model.glb".to_string(),
            output_name: "model.glb".to_string(),
            created_at_ms: 1,
            artifact_size_bytes: 1,
            artifact_sha256: "hash".to_string(),
            managed_artifact: true,
            companion_origin_path: None,
            metadata: Value::Null,
        });
        store.delivery_identities.push(HistoryDeliveryIdentity {
            tool: "3d".to_string(),
            dispatch_id: "dispatch".to_string(),
            entry_id: "entry".to_string(),
            artifact_file_identity: "identity".to_string(),
        });
        assert!(store_has_delivery_receipt(
            &store,
            "3d",
            "dispatch",
            "model.glb"
        ));
        assert!(!store_has_delivery_receipt(
            &store,
            "3d",
            "other",
            "model.glb"
        ));
        assert!(!store_has_delivery_receipt(
            &store,
            "3d",
            "dispatch",
            "other.glb"
        ));
        store.entries.clear();
        assert!(!store_has_delivery_receipt(
            &store,
            "3d",
            "dispatch",
            "model.glb"
        ));
    }
}
