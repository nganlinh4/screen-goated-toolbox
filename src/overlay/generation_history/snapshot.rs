use super::*;

pub(super) fn list_snapshot_at(path: &Path, tool: &str) -> Result<Vec<ResultHistoryEntry>, String> {
    validate_tool(tool)?;
    let store = load_store(path)?;
    let pending = store
        .pending_renames
        .iter()
        .map(|rename| (rename.tool.as_str(), rename.entry_id.as_str()))
        .collect::<std::collections::HashSet<_>>();
    let mut entries = store
        .entries
        .into_iter()
        .filter(|entry| {
            entry.tool == tool
                && !pending.contains(&(entry.tool.as_str(), entry.id.as_str()))
                && output_may_exist(Path::new(&entry.output_path))
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.created_at_ms));
    Ok(entries)
}

pub(super) fn output_may_exist(path: &Path) -> bool {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata.file_type().is_file(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => !path
            .parent()
            .and_then(|parent| std::fs::metadata(parent).ok())
            .is_some_and(|parent| parent.is_dir()),
        Err(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch(PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn unavailable_destination_history_readers_preserve_rows_without_hiding_real_deletions() {
        let root = std::env::temp_dir().join(
            crate::overlay::creation_identity::random_id("sgt-history-availability-").unwrap(),
        );
        std::fs::create_dir(&root).unwrap();
        let _scratch = Scratch(root.clone());
        let online = root.join("online");
        let offline = root.join("offline");
        std::fs::create_dir(&online).unwrap();
        std::fs::create_dir(&offline).unwrap();
        std::fs::write(online.join("present.png"), b"result").unwrap();
        std::fs::write(offline.join("result.png"), b"result").unwrap();
        std::fs::create_dir(online.join("directory.png")).unwrap();
        let mut store = ResultHistoryStore::default();
        for (index, (id, output)) in [
            ("present", online.join("present.png")),
            ("unavailable", offline.join("result.png")),
            ("missing", online.join("missing.png")),
            ("unsafe", online.join("directory.png")),
        ]
        .into_iter()
        .enumerate()
        {
            store.entries.push(ResultHistoryEntry {
                id: id.into(),
                tool: "image".into(),
                source_path: String::new(),
                output_name: output.file_name().unwrap().to_string_lossy().into_owned(),
                output_path: output.to_string_lossy().into_owned(),
                created_at_ms: index as u64,
                artifact_size_bytes: 6,
                artifact_sha256: "a".repeat(64),
                managed_artifact: false,
                companion_origin_path: None,
                metadata: Value::Null,
            });
        }
        let journal = root.join("history.json");
        save_store(&journal, &store).unwrap();
        std::fs::rename(&offline, root.join("relocated")).unwrap();
        let before = std::fs::read(&journal).unwrap();
        let snapshot = list_snapshot_at(&journal, "image").unwrap();
        assert_eq!(std::fs::read(&journal).unwrap(), before);
        let reconciled = super::super::list_at(&journal, "image", 50).unwrap();
        for entries in [snapshot, reconciled] {
            assert_eq!(
                entries
                    .iter()
                    .map(|entry| entry.id.as_str())
                    .collect::<Vec<_>>(),
                ["unavailable", "present"]
            );
        }
        assert_eq!(load_store(&journal).unwrap().entries.len(), 2);
    }
}
