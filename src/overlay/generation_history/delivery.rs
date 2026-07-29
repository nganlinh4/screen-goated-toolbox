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

pub(crate) fn record_delivery(
    tool: &str,
    dispatch_id: &str,
    source_path: &str,
    output_path: &str,
    metadata: Value,
    artifact: &DeliveryArtifactIdentity,
    protected_paths: &std::collections::HashSet<String>,
) -> Result<ResultHistoryEntry, String> {
    let results_per_tool = results_per_tool_limit();
    let path = history_path();
    let protected_paths = protected_paths
        .iter()
        .map(|path| path_identity(path))
        .collect();
    let entry = {
        let _guard = HISTORY_LOCK
            .lock()
            .map_err(|_| "Result history is unavailable.".to_string())?;
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
