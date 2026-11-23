//! Identity-aware protection of paths participating in creation delivery.

use std::path::{Path, PathBuf};

use super::*;

const UNAVAILABLE: &str = "Creation result ownership is unavailable.";

#[derive(Clone, Debug)]
struct PathKey {
    path: String,
    identity: Option<String>,
}

fn key(path: &Path) -> Result<PathKey, String> {
    if !path.is_absolute() {
        return Err(UNAVAILABLE.to_string());
    }
    let parent = std::fs::canonicalize(path.parent().ok_or(UNAVAILABLE)?)
        .map_err(|_| UNAVAILABLE.to_string())?;
    let name = path.file_name().ok_or(UNAVAILABLE)?;
    let lexical = parent.join(name);
    let (resolved, identity) = match std::fs::symlink_metadata(&lexical) {
        Ok(_) => {
            let metadata = std::fs::metadata(&lexical).map_err(|_| UNAVAILABLE.to_string())?;
            if !metadata.is_file() {
                return Err(UNAVAILABLE.to_string());
            }
            let resolved = std::fs::canonicalize(&lexical).map_err(|_| UNAVAILABLE.to_string())?;
            let identity = crate::overlay::creation_file_identity::from_path(&resolved)?;
            (resolved, Some(identity))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (lexical, None),
        _ => return Err(UNAVAILABLE.to_string()),
    };
    Ok(PathKey {
        path: crate::overlay::creation_path::normalized_absolute(&resolved)?
            .to_string_lossy()
            .to_string(),
        identity,
    })
}

fn matches(left: &PathKey, right: &PathKey) -> Result<bool, String> {
    Ok(
        crate::overlay::creation_path::same(&left.path, &right.path)?
            || left
                .identity
                .as_ref()
                .is_some_and(|identity| right.identity.as_ref() == Some(identity)),
    )
}

#[derive(Default)]
pub(super) struct ProtectedPaths {
    keys: Vec<PathKey>,
}

impl ProtectedPaths {
    pub(super) fn pending() -> Result<Self, String> {
        Self::from_saved_reservations(
            crate::overlay::creation_delivery::pending_output_reservations()?,
        )
    }

    pub(super) fn from_saved_reservations(
        reservations: impl IntoIterator<
            Item = crate::overlay::creation_delivery::PendingOutputReservation,
        >,
    ) -> Result<Self, String> {
        let mut keys = Vec::new();
        for reservation in reservations {
            let path = crate::overlay::creation_path::normalized_absolute(Path::new(
                &reservation.output_path,
            ))?;
            keys.push(PathKey {
                path: path.to_string_lossy().to_string(),
                identity: reservation.file_identity,
            });
        }
        Ok(Self { keys })
    }

    pub(super) fn from_paths(paths: impl IntoIterator<Item = PathBuf>) -> Result<Self, String> {
        let keys = paths
            .into_iter()
            .map(|path| key(&path))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { keys })
    }

    pub(super) fn from_saved_paths(
        paths: impl IntoIterator<Item = PathBuf>,
    ) -> Result<Self, String> {
        Self::from_saved_reservations(paths.into_iter().map(|path| {
            crate::overlay::creation_delivery::PendingOutputReservation {
                output_path: path.to_string_lossy().to_string(),
                file_identity: None,
            }
        }))
    }

    pub(super) fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    pub(super) fn extend(&mut self, other: &Self) {
        self.keys.extend(other.keys.iter().cloned());
    }

    pub(super) fn conflicts_path(&self, path: &Path) -> Result<bool, String> {
        if self.is_empty() {
            return Ok(false);
        }
        let candidate = key(path)?;
        for protected in &self.keys {
            if matches(protected, &candidate)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(super) fn conflicts_entry(&self, entry: &ResultHistoryEntry) -> Result<bool, String> {
        if self.is_empty() {
            return Ok(false);
        }
        if self
            .conflicts_path(Path::new(&entry.output_path))
            .unwrap_or(true)
        {
            return Ok(true);
        }
        let side = match companion::checked_from_entry(entry) {
            Ok(side) => side,
            Err(_) => return Ok(true),
        };
        match side {
            Some(side) => Ok(self.conflicts_path(Path::new(&side.path)).unwrap_or(true)),
            None => Ok(false),
        }
    }

    pub(super) fn expanded(
        &self,
        store: &ResultHistoryStore,
    ) -> Result<std::collections::HashSet<String>, String> {
        let mut result = self
            .keys
            .iter()
            .map(|value| path_identity(&value.path))
            .collect::<std::collections::HashSet<_>>();
        for entry in &store.entries {
            if self.conflicts_entry(entry)? {
                result.insert(path_identity(&entry.output_path));
            }
        }
        Ok(result)
    }
}

pub(crate) fn ensure_output_unreserved(path: &Path) -> Result<(), String> {
    let _guard = locking::acquire()?;
    ensure_output_unreserved_at(&history_path(), path)
}

fn ensure_output_unreserved_at(store_path: &Path, path: &Path) -> Result<(), String> {
    let store = load_store(store_path)?;
    let mut paths = Vec::new();
    for pending in &store.pending_deletes {
        paths.push(PathBuf::from(&pending.entry.output_path));
        if let Some(side) =
            companion::checked_from_entry(&pending.entry).map_err(|_| UNAVAILABLE.to_string())?
        {
            paths.push(PathBuf::from(side.path));
        }
    }
    for cleanup in &store.pending_cleanup {
        paths.push(PathBuf::from(&cleanup.output_path));
        if !cleanup.quarantine_path.is_empty() {
            paths.push(PathBuf::from(&cleanup.quarantine_path));
        }
    }
    for rename in &store.pending_renames {
        paths.push(PathBuf::from(&rename.previous_path));
        paths.push(PathBuf::from(&rename.next_path));
    }
    if ProtectedPaths::from_paths(paths)?.conflicts_path(path)? {
        return Err("Result file is awaiting history recovery.".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_destination_preserves_captured_identity_protection() {
        let root = root();
        let destination = root.join("destination");
        std::fs::create_dir(&destination).unwrap();
        let output = destination.join("result.bin");
        std::fs::write(&output, b"owned-result").unwrap();
        let identity = crate::overlay::creation_file_identity::from_path(&output).unwrap();
        let alias = root.join("result-alias.bin");
        std::fs::hard_link(&output, &alias).unwrap();
        std::fs::rename(&destination, root.join("moved-destination")).unwrap();
        let result = (|| {
            let protection = ProtectedPaths::from_saved_reservations([
                crate::overlay::creation_delivery::PendingOutputReservation {
                    output_path: output.to_string_lossy().to_string(),
                    file_identity: Some(identity),
                },
            ])?;
            assert!(protection.conflicts_path(&alias)?);
            assert!(!protection.conflicts_path(&root.join("different.bin"))?);
            Ok::<(), String>(())
        })();
        std::fs::remove_dir_all(&root).unwrap();
        result.expect("Saved ownership could not survive an unavailable path");
    }

    fn root() -> PathBuf {
        let root = std::env::temp_dir()
            .join(crate::overlay::creation_identity::random_id("history-protection-").unwrap());
        std::fs::create_dir(&root).unwrap();
        root
    }

    #[cfg(windows)]
    #[test]
    fn missing_unicode_case_alias_conflicts_before_either_file_exists() {
        let root = root();
        let reserved = root.join("rÉsumÉ.glb");
        let alias = root.join("RÉSUMÉ.GLB");
        let alternate = root.join("résumé.glb");
        assert!(!reserved.exists() && !alias.exists() && !alternate.exists());
        let protected = ProtectedPaths::from_paths([reserved]).unwrap();
        assert!(protected.conflicts_path(&alias).unwrap());
        assert!(protected.conflicts_path(&alternate).unwrap());
        assert!(
            !protected
                .conflicts_path(&root.join("different.glb"))
                .unwrap()
        );
        std::fs::remove_dir(root).unwrap();
    }

    #[test]
    fn pending_rename_reserves_both_names_and_existing_hardlink_alias() {
        let root = root();
        let index = root.join("history.json");
        let original = root.join("result.glb");
        let alias = root.join("alias.glb");
        let target = root.join("renamed.glb");
        std::fs::write(&original, b"model").unwrap();
        std::fs::hard_link(&original, &alias).unwrap();
        let mut store = ResultHistoryStore::default();
        store.pending_renames.push(PendingRename {
            tool: "3d".into(),
            entry_id: "id".into(),
            previous_path: original.to_string_lossy().to_string(),
            next_path: target.to_string_lossy().to_string(),
            next_name: "renamed.glb".into(),
            artifact_size_bytes: 5,
            artifact_sha256: "a".repeat(64),
            artifact_file_identity: String::new(),
        });
        save_store(&index, &store).unwrap();
        for reserved in [&original, &alias, &target] {
            assert!(ensure_output_unreserved_at(&index, reserved).is_err());
        }
        assert!(ensure_output_unreserved_at(&index, &root.join("unrelated.glb")).is_ok());
        std::fs::remove_file(index).unwrap();
        std::fs::remove_file(alias).unwrap();
        std::fs::remove_file(original).unwrap();
        std::fs::remove_dir(root).unwrap();
    }

    #[test]
    fn active_delivery_alias_blocks_rename_and_delete_without_affecting_data() {
        let root = root();
        let index = root.join("history.json");
        let output = root.join("result.png");
        let alias = root.join("alias.png");
        std::fs::write(&output, b"original").unwrap();
        std::fs::hard_link(&output, &alias).unwrap();
        let entry = record_at(
            &index,
            "image",
            "",
            output.to_str().unwrap(),
            Value::Null,
            200,
            (None, None),
        )
        .unwrap();
        let protected = ProtectedPaths::from_paths([alias.clone()]).unwrap();
        assert!(rename_at_protected(&index, "image", &entry.id, "other", 200, &protected).is_err());
        assert!(delete_at_protected(&index, "image", &entry.id, &protected).is_err());
        assert!(delete_all_at_protected(&index, "image", &protected).is_err());
        assert_eq!(std::fs::read(&output).unwrap(), b"original");
        assert_eq!(load_store(&index).unwrap().entries.len(), 1);
        std::fs::remove_file(index).unwrap();
        std::fs::remove_file(alias).unwrap();
        std::fs::remove_file(output).unwrap();
        std::fs::remove_dir(root).unwrap();
    }

    #[test]
    fn pending_pair_and_cleanup_reserve_paths_and_hard_links() {
        let root = root();
        let index = root.join("history.json");
        let primary = root.join("model.glb");
        let companion = root.join("model.fbx");
        let alias = root.join("alias.fbx");
        let cleanup = root.join("cleanup.png");
        let quarantine = root.join("quarantine.png");
        std::fs::write(&primary, b"primary").unwrap();
        std::fs::write(&companion, b"companion").unwrap();
        std::fs::hard_link(&companion, &alias).unwrap();
        std::fs::write(&quarantine, b"quarantine").unwrap();
        let primary_artifact = inspect_delivery_artifact(primary.to_str().unwrap()).unwrap();
        let side = inspect_delivery_artifact(companion.to_str().unwrap()).unwrap();
        let mut store = ResultHistoryStore::default();
        store.pending_deletes.push(serde_json::from_value(serde_json::json!({
            "entry": {"id":"pair", "tool":"3d", "sourcePath":"", "outputPath":primary.to_str().unwrap(),
                "outputName":"model.glb", "createdAtMs":1, "metadata":{"download":{
                    "path":companion.to_str().unwrap(), "name":"model.fbx", "sizeBytes":side.size_bytes,
                    "sha256":side.sha256, "fileIdentity":side.file_identity
                }}},
            "artifactFileIdentity": primary_artifact.file_identity
        })).unwrap());
        store.pending_cleanup.push(PendingCleanup {
            output_path: cleanup.to_string_lossy().to_string(),
            artifact_size_bytes: 1,
            artifact_sha256: "a".repeat(64),
            artifact_file_identity: String::new(),
            quarantine_path: quarantine.to_string_lossy().to_string(),
            history_entry: None,
            resolution: CleanupResolution::Pending,
        });
        save_store(&index, &store).unwrap();
        for reserved in [&primary, &companion, &alias, &cleanup, &quarantine] {
            assert!(ensure_output_unreserved_at(&index, reserved).is_err());
        }
        assert!(ensure_output_unreserved_at(&index, &root.join("other.png")).is_ok());
        std::fs::remove_file(index).unwrap();
        std::fs::remove_file(primary).unwrap();
        std::fs::remove_file(alias).unwrap();
        std::fs::remove_file(companion).unwrap();
        std::fs::remove_file(quarantine).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
