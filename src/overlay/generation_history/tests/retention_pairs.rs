use super::*;

fn paired_store(label: &str) -> (PathBuf, PathBuf, PathBuf, PathBuf, ResultHistoryStore) {
    let root = persistence::paired_root(label);
    let managed = root.join("3d-generator");
    let (history, entry, primary, companion) = persistence::paired_result(&managed);
    let mut store = load_store(&history).unwrap();
    store.entries[0].managed_artifact = true;
    store.entries[0].created_at_ms = 1;
    let mut newest = entry;
    newest.id = "newest".into();
    newest.created_at_ms = 2;
    newest.metadata = Value::Null;
    newest.managed_artifact = false;
    store.entries.push(newest);
    (root, history, primary, companion, store)
}

#[test]
fn malformed_and_changed_companions_survive_pruning_pressure() {
    for malformed in [false, true] {
        let (root, _, primary, companion, mut store) = paired_store("protected-pair");
        if malformed {
            store.entries[0].metadata["download"] = serde_json::json!({"path": "invalid"});
        } else {
            std::fs::write(&companion, b"changed").unwrap();
        }
        prune_store(&mut store, 1);
        assert_eq!(store.entries.len(), 2);
        assert!(store.pending_cleanup.is_empty());
        assert!(store.pending_deletes.is_empty());
        assert!(primary.exists());
        assert!(companion.exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn companion_bytes_contribute_to_live_storage_pressure() {
    let (root, _, _, companion, mut store) = paired_store("pair-byte-budget");
    store.entries.truncate(1);
    store.entries[0].artifact_size_bytes = MAX_MANAGED_ARTIFACT_BYTES - 30;
    std::fs::write(&companion, [7_u8; 50]).unwrap();
    store.entries[0].metadata["download"]["sizeBytes"] = serde_json::json!(50);
    store.entries.push(entry(2, "image", 20, true));
    store.entries.push(entry(1, "image", 10, true));
    assert!(prune_store(&mut store, 10));
    assert!(store.entries.iter().any(|item| item.tool == "3d"));
    assert!(!store.entries.iter().any(|item| item.id == "image-1"));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn forged_or_mismatched_companion_sizes_do_not_evict_other_results() {
    for malformed in [false, true] {
        let (root, _, _, _, mut store) = paired_store("forged-pair-budget");
        store.entries.truncate(1);
        store.entries[0].artifact_size_bytes = MAX_MANAGED_ARTIFACT_BYTES - 30;
        if malformed {
            store.entries[0].metadata["download"] = serde_json::json!({"sizeBytes": u64::MAX});
        } else {
            store.entries[0].metadata["download"]["sizeBytes"] = serde_json::json!(u64::MAX);
        }
        store.entries.push(entry(2, "image", 20, true));
        store.entries.push(entry(1, "image", 10, true));
        assert!(!prune_store(&mut store, 10));
        assert!(store.entries.iter().any(|item| item.id == "image-1"));
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn paired_pruning_replays_as_one_group_and_restores_changed_companion() {
    let (root, history, primary, companion, mut store) = paired_store("queued-pair");
    // The newer synthetic entry must not claim the same file.
    store.entries[1].output_path = root.join("newer.glb").to_string_lossy().into_owned();
    std::fs::write(&store.entries[1].output_path, b"newer").unwrap();
    assert!(prune_store(&mut store, 1));
    assert_eq!(store.pending_deletes.len(), 1);
    assert!(store.pending_cleanup.is_empty());
    save_store(&history, &store).unwrap();
    std::fs::write(&companion, b"changed after queue").unwrap();
    let mut recovered = load_store(&history).unwrap();
    assert!(deletion::reconcile_pending_deletes(&mut recovered));
    assert!(recovered.pending_deletes.is_empty());
    assert!(recovered.entries.iter().any(|entry| entry.id != "newest"));
    assert!(primary.exists());
    assert_eq!(std::fs::read(&companion).unwrap(), b"changed after queue");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn legacy_independent_pair_cleanup_remains_deferred() {
    let (root, _, primary, companion, mut store) = paired_store("legacy-pair");
    let entry = store.entries.remove(0);
    let identity = store.delivery_identities[0].artifact_file_identity.clone();
    let side = companion::from_entry(&entry).unwrap();
    store.entries.clear();
    store.pending_cleanup = vec![
        PendingCleanup {
            output_path: entry.output_path.clone(),
            artifact_size_bytes: entry.artifact_size_bytes,
            artifact_sha256: entry.artifact_sha256.clone(),
            artifact_file_identity: identity,
            quarantine_path: primary
                .parent()
                .unwrap()
                .join(".sgt-prune-primary.tmp")
                .to_string_lossy()
                .into_owned(),
            history_entry: Some(entry),
            resolution: CleanupResolution::Pending,
        },
        PendingCleanup {
            output_path: side.path,
            artifact_size_bytes: side.size_bytes,
            artifact_sha256: side.sha256,
            artifact_file_identity: side.file_identity,
            quarantine_path: companion
                .parent()
                .unwrap()
                .join(".sgt-prune-side.tmp")
                .to_string_lossy()
                .into_owned(),
            history_entry: None,
            resolution: CleanupResolution::Pending,
        },
    ];
    assert!(!run_pending_cleanup_under(&mut store, &root));
    assert_eq!(store.pending_cleanup.len(), 2);
    assert!(primary.exists());
    assert!(companion.exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn legacy_unassociated_quarantined_companion_is_never_deleted() {
    for malformed_primary in [false, true] {
        let (root, _, primary, companion, mut store) = paired_store("legacy-unassociated");
        let mut entry = store.entries.remove(0);
        let side = companion::from_entry(&entry).unwrap();
        let quarantine = companion.parent().unwrap().join(".sgt-prune-side.tmp");
        std::fs::rename(&companion, &quarantine).unwrap();
        store.entries.clear();
        store.pending_cleanup = vec![PendingCleanup {
            output_path: side.path,
            artifact_size_bytes: side.size_bytes,
            artifact_sha256: side.sha256,
            artifact_file_identity: side.file_identity,
            quarantine_path: quarantine.to_string_lossy().into_owned(),
            history_entry: None,
            resolution: CleanupResolution::Pending,
        }];
        if malformed_primary {
            entry.metadata["download"] = serde_json::json!({});
            store.pending_cleanup.push(PendingCleanup {
                output_path: entry.output_path.clone(),
                artifact_size_bytes: entry.artifact_size_bytes,
                artifact_sha256: entry.artifact_sha256.clone(),
                artifact_file_identity: store.delivery_identities[0].artifact_file_identity.clone(),
                quarantine_path: primary
                    .parent()
                    .unwrap()
                    .join(".sgt-prune-primary.tmp")
                    .to_string_lossy()
                    .into_owned(),
                history_entry: Some(entry),
                resolution: CleanupResolution::Pending,
            });
        }
        assert!(!run_pending_cleanup_under(&mut store, &root));
        assert_eq!(std::fs::read(&quarantine).unwrap(), b"companion model");
        assert!(primary.exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
