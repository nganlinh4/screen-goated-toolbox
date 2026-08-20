use std::path::Path;

use serde::Deserialize;

use super::*;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CompanionIdentity {
    pub path: String,
    pub name: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub file_identity: String,
}

pub(super) fn from_entry(entry: &ResultHistoryEntry) -> Option<CompanionIdentity> {
    let value = entry.metadata.get("download")?.clone();
    let companion: CompanionIdentity = serde_json::from_value(value).ok()?;
    let path = Path::new(&companion.path);
    let valid = path.is_absolute()
        && path.file_name().and_then(|name| name.to_str()) == Some(companion.name.as_str())
        && path.parent() == Path::new(&entry.output_path).parent()
        && path.file_stem() == Path::new(&entry.output_path).file_stem()
        && path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("fbx"))
        && companion.size_bytes > 0
        && companion.sha256.len() == 64
        && crate::overlay::creation_file_identity::valid(&companion.file_identity);
    valid.then_some(companion)
}

pub(super) fn delete_for_entry(entry: &ResultHistoryEntry) -> Result<(), String> {
    let Some(companion) = from_entry(entry) else {
        return Ok(());
    };
    let path = Path::new(&companion.path);
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("Result companion is unavailable.".to_string()),
        Ok(_) => {
            let file = crate::overlay::creation_delivery::publication::lock_owned_path(
                path,
                Some(&companion.file_identity),
                false,
                true,
            )?;
            let artifact = inspect_delivery_artifact(&companion.path)?;
            if artifact.size_bytes != companion.size_bytes
                || artifact.sha256 != companion.sha256
                || artifact.file_identity != companion.file_identity
            {
                return Err("Result companion changed before it could be deleted.".to_string());
            }
            crate::overlay::creation_delivery::publication::delete_owned(&file, path)
                .map_err(|error| format!("Could not delete {}: {error}", path.display()))
        }
    }
}

pub(super) fn pending_cleanup(entry: &ResultHistoryEntry) -> Option<PendingCleanup> {
    let companion = from_entry(entry)?;
    Some(PendingCleanup {
        output_path: companion.path,
        artifact_size_bytes: companion.size_bytes,
        artifact_sha256: companion.sha256,
        artifact_file_identity: companion.file_identity,
        quarantine_path: String::new(),
        history_entry: None,
        resolution: CleanupResolution::Pending,
    })
}
