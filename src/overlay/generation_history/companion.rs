use std::path::Path;

use serde::Deserialize;

use super::*;

mod legacy;
pub(super) use legacy::{bind_orphaned, migrate_cleanup};

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
    let primary = Path::new(&entry.output_path);
    let origin = entry
        .companion_origin_path
        .as_deref()
        .map(Path::new)
        .unwrap_or(primary);
    let valid = path.is_absolute()
        && path.file_name().and_then(|name| name.to_str()) == Some(companion.name.as_str())
        && path.parent() == primary.parent()
        && origin.is_absolute()
        && origin.parent() == primary.parent()
        && path.file_stem() == origin.file_stem()
        && path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("fbx"))
        && companion.size_bytes > 0
        && companion.sha256.len() == 64
        && crate::overlay::creation_file_identity::valid(&companion.file_identity);
    valid.then_some(companion)
}

pub(super) fn checked_from_entry(
    entry: &ResultHistoryEntry,
) -> Result<Option<CompanionIdentity>, ()> {
    match entry.metadata.get("download") {
        None | Some(Value::Null) => Ok(None),
        Some(_) => from_entry(entry).map(Some).ok_or(()),
    }
}

pub(crate) fn validated_companion_path(
    entry: &ResultHistoryEntry,
) -> Result<Option<PathBuf>, String> {
    let Some(companion) = checked_from_entry(entry)
        .map_err(|_| "Result companion ownership cannot be verified.".to_string())?
    else {
        return Ok(None);
    };
    let inspected = inspect_delivery_artifact(&companion.path)?;
    if inspected.size_bytes != companion.size_bytes
        || inspected.sha256 != companion.sha256
        || inspected.file_identity != companion.file_identity
    {
        return Err("Result companion changed before export.".to_string());
    }
    Ok(Some(PathBuf::from(companion.path)))
}

pub(super) fn lock_for_entry(
    entry: &ResultHistoryEntry,
) -> Result<Option<(std::fs::File, PathBuf)>, String> {
    let Some(companion) = from_entry(entry) else {
        return Ok(None);
    };
    let path = Path::new(&companion.path);
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
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
            Ok(Some((file, path.to_path_buf())))
        }
    }
}
