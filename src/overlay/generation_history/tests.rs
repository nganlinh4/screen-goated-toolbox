use super::*;

mod persistence;

const TEST_FILE_IDENTITY: &str = "00000000:0000000000000001";

fn entry(id: usize, tool: &str, size: u64, managed: bool) -> ResultHistoryEntry {
    ResultHistoryEntry {
        id: format!("{tool}-{id}"),
        tool: tool.to_string(),
        source_path: "source.png".to_string(),
        output_path: format!("result-{id}.png"),
        output_name: format!("result-{id}.png"),
        created_at_ms: id as u64,
        artifact_size_bytes: size,
        artifact_sha256: "digest".to_string(),
        managed_artifact: managed,
        metadata: Value::Null,
    }
}

#[test]
fn public_history_view_excludes_delivery_ownership_proof() {
    let value = serde_json::to_value(public_entry(&entry(1, "image", 42, true))).unwrap();
    assert!(value.get("artifactSizeBytes").is_none());
    assert!(value.get("artifactSha256").is_none());
    assert!(value.get("managedArtifact").is_none());
    assert_eq!(value["outputName"], "result-1.png");
}

#[test]
fn retention_keeps_two_hundred_per_tool_and_newest_managed_result() {
    let mut store = ResultHistoryStore {
        entries: (0..=200).map(|id| entry(id, "image", 1, false)).collect(),
        pending_cleanup: Vec::new(),
        ..Default::default()
    };
    assert!(prune_store(&mut store, 200));
    assert_eq!(store.entries.len(), 200);
    assert!(store.entries.iter().any(|item| item.id == "image-200"));

    store.entries = vec![
        entry(3, "3d", 64, true),
        entry(2, "svg", 64, true),
        entry(1, "3d", MAX_MANAGED_ARTIFACT_BYTES + 1, true),
    ];
    store.pending_cleanup.clear();
    assert!(prune_store(&mut store, 200));
    assert_eq!(store.entries.len(), 2);
    assert!(store.entries.iter().any(|item| item.id == "3d-3"));
    assert!(store.entries.iter().any(|item| item.id == "svg-2"));
}

#[test]
fn delete_all_removes_only_the_requested_tools_results() {
    let root = std::env::temp_dir().join(format!(
        "sgt-history-delete-all-{}",
        HISTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).unwrap();
    let store_path = root.join("history.json");
    for (tool, name) in [("3d", "one.glb"), ("3d", "two.glb"), ("svg", "keep.svg")] {
        let output = root.join(name);
        std::fs::write(&output, b"result").unwrap();
        record_at(
            &store_path,
            tool,
            "source.png",
            output.to_str().unwrap(),
            Value::Null,
            DEFAULT_RESULTS_PER_TOOL,
            (None, None),
        )
        .unwrap();
    }

    assert_eq!(delete_all_at(&store_path, "3d").unwrap(), 2);
    assert!(list_snapshot_at(&store_path, "3d").unwrap().is_empty());
    assert_eq!(list_snapshot_at(&store_path, "svg").unwrap().len(), 1);
    assert!(!root.join("one.glb").exists());
    assert!(!root.join("two.glb").exists());
    assert!(root.join("keep.svg").exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn retention_handles_corrupt_sizes_and_deduplicates_cleanup_intents() {
    let mut store = ResultHistoryStore {
        entries: (0..=201)
            .map(|id| {
                let mut item = entry(id, "image", u64::MAX, true);
                item.output_path = "same-managed-result.png".to_string();
                item
            })
            .collect(),
        pending_cleanup: Vec::new(),
        delivery_identities: (0..=201)
            .map(|id| HistoryDeliveryIdentity {
                tool: "image".to_string(),
                dispatch_id: format!("dispatch-{id}"),
                entry_id: format!("image-{id}"),
                artifact_file_identity: TEST_FILE_IDENTITY.to_string(),
            })
            .collect(),
        ..Default::default()
    };

    assert!(prune_store(&mut store, 200));
    assert_eq!(store.entries.len(), 1);
    assert_eq!(store.pending_cleanup.len(), 1);
}

#[test]
fn retention_counts_unresolved_cleanup_bytes_against_global_budget() {
    let pending_entry = entry(0, "svg", MAX_MANAGED_ARTIFACT_BYTES, true);
    let mut store = ResultHistoryStore {
        entries: vec![entry(2, "image", 1, true), entry(1, "image", 1, true)],
        pending_cleanup: vec![PendingCleanup {
            output_path: pending_entry.output_path.clone(),
            artifact_size_bytes: pending_entry.artifact_size_bytes,
            artifact_sha256: pending_entry.artifact_sha256.clone(),
            artifact_file_identity: TEST_FILE_IDENTITY.to_string(),
            quarantine_path: String::new(),
            history_entry: Some(pending_entry),
            resolution: CleanupResolution::Pending,
        }],
        ..Default::default()
    };

    assert!(prune_store(&mut store, 200));
    assert_eq!(store.entries.len(), 1);
    assert_eq!(store.entries[0].id, "image-2");
}

#[test]
fn cleanup_quarantines_exact_bytes_and_restores_modified_artifacts() {
    let root = std::env::temp_dir().join(format!(
        "sgt-result-cleanup-{}-{}",
        std::process::id(),
        now_ms()
    ));
    let managed = root.join("images");
    std::fs::create_dir_all(&managed).unwrap();

    let exact_path = managed.join("exact.png");
    std::fs::write(&exact_path, b"owned bytes").unwrap();
    let (exact_size, exact_digest) = digest_file(&exact_path).unwrap();
    let exact_identity = crate::overlay::creation_file_identity::from_path(&exact_path).unwrap();
    let exact_entry = ResultHistoryEntry {
        output_path: exact_path.to_string_lossy().to_string(),
        output_name: "exact.png".to_string(),
        artifact_size_bytes: exact_size,
        artifact_sha256: exact_digest.clone(),
        managed_artifact: true,
        ..entry(1, "image", exact_size, true)
    };

    let changed_path = managed.join("changed.png");
    std::fs::write(&changed_path, b"original bytes").unwrap();
    let (changed_size, changed_digest) = digest_file(&changed_path).unwrap();
    let changed_identity =
        crate::overlay::creation_file_identity::from_path(&changed_path).unwrap();
    let changed_entry = ResultHistoryEntry {
        output_path: changed_path.to_string_lossy().to_string(),
        output_name: "changed.png".to_string(),
        artifact_size_bytes: changed_size,
        artifact_sha256: changed_digest.clone(),
        managed_artifact: true,
        ..entry(2, "image", changed_size, true)
    };

    let mut store = ResultHistoryStore {
        entries: Vec::new(),
        pending_cleanup: vec![
            PendingCleanup {
                output_path: exact_path.to_string_lossy().to_string(),
                artifact_size_bytes: exact_size,
                artifact_sha256: exact_digest,
                artifact_file_identity: exact_identity,
                quarantine_path: String::new(),
                history_entry: Some(exact_entry),
                resolution: CleanupResolution::Pending,
            },
            PendingCleanup {
                output_path: changed_path.to_string_lossy().to_string(),
                artifact_size_bytes: changed_size,
                artifact_sha256: changed_digest,
                artifact_file_identity: changed_identity,
                quarantine_path: String::new(),
                history_entry: Some(changed_entry),
                resolution: CleanupResolution::Pending,
            },
        ],
        ..Default::default()
    };
    assert!(prepare_cleanup_quarantines(&mut store));
    std::fs::write(&changed_path, b"user modified bytes").unwrap();

    assert!(run_pending_cleanup_under(&mut store, &root));
    assert!(!exact_path.exists());
    assert_eq!(
        std::fs::read(&changed_path).unwrap(),
        b"user modified bytes".to_vec()
    );
    assert!(store.pending_cleanup.is_empty());
    assert_eq!(store.entries.len(), 1);
    assert!(!store.entries[0].managed_artifact);

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn cleanup_relinquishes_modified_bytes_when_original_name_is_reused() {
    let root = std::env::temp_dir().join(format!(
        "sgt-result-cleanup-collision-{}-{}",
        std::process::id(),
        now_ms()
    ));
    let managed = root.join("images");
    std::fs::create_dir_all(&managed).unwrap();
    let original = managed.join("result.png");
    std::fs::write(&original, b"committed").unwrap();
    let (size, digest) = digest_file(&original).unwrap();
    let file_identity = crate::overlay::creation_file_identity::from_path(&original).unwrap();
    let history_entry = ResultHistoryEntry {
        output_path: original.to_string_lossy().to_string(),
        output_name: "result.png".to_string(),
        artifact_size_bytes: size,
        artifact_sha256: digest.clone(),
        managed_artifact: true,
        ..entry(1, "image", size, true)
    };
    let mut store = ResultHistoryStore {
        entries: Vec::new(),
        pending_cleanup: vec![PendingCleanup {
            output_path: original.to_string_lossy().to_string(),
            artifact_size_bytes: size,
            artifact_sha256: digest,
            artifact_file_identity: file_identity,
            quarantine_path: String::new(),
            history_entry: Some(history_entry),
            resolution: CleanupResolution::Pending,
        }],
        ..Default::default()
    };
    assert!(prepare_cleanup_quarantines(&mut store));
    let quarantine = PathBuf::from(&store.pending_cleanup[0].quarantine_path);
    std::fs::rename(&original, &quarantine).unwrap();
    std::fs::write(&quarantine, b"modified").unwrap();
    std::fs::write(&original, b"replacement").unwrap();

    let cleanup_changed = run_pending_cleanup_under(&mut store, &root);
    assert!(cleanup_changed);
    assert!(store.pending_cleanup.is_empty());
    assert_eq!(std::fs::read(&original).unwrap(), b"replacement");
    assert_eq!(store.entries.len(), 1);
    assert!(!store.entries[0].managed_artifact);
    assert_ne!(Path::new(&store.entries[0].output_path), original);
    assert_eq!(
        std::fs::read(&store.entries[0].output_path).unwrap(),
        b"modified"
    );

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn cleanup_restore_resolution_replays_after_move_before_history_save() {
    let root = std::env::temp_dir().join(format!(
        "sgt-cleanup-restore-replay-{}-{}",
        std::process::id(),
        now_ms()
    ));
    let managed = root.join("images");
    std::fs::create_dir_all(&managed).unwrap();
    let original = managed.join("result.png");
    let quarantine = managed.join(".sgt-prune-replay.tmp");
    std::fs::write(&quarantine, b"user modified").unwrap();
    let artifact = inspect_delivery_artifact(&quarantine.to_string_lossy()).unwrap();
    let history_entry = ResultHistoryEntry {
        output_path: original.to_string_lossy().to_string(),
        output_name: "result.png".to_string(),
        artifact_size_bytes: artifact.size_bytes,
        artifact_sha256: artifact.sha256.clone(),
        managed_artifact: true,
        ..entry(1, "image", artifact.size_bytes, true)
    };
    let store_path = root.join("history.json");
    let store = ResultHistoryStore {
        pending_cleanup: vec![PendingCleanup {
            output_path: original.to_string_lossy().to_string(),
            artifact_size_bytes: artifact.size_bytes,
            artifact_sha256: artifact.sha256,
            artifact_file_identity: artifact.file_identity,
            quarantine_path: quarantine.to_string_lossy().to_string(),
            history_entry: Some(history_entry),
            resolution: CleanupResolution::Restore {
                target_path: original.to_string_lossy().to_string(),
            },
        }],
        ..Default::default()
    };
    save_store(&store_path, &store).unwrap();
    std::fs::rename(&quarantine, &original).unwrap();

    let mut recovered = load_store(&store_path).unwrap();
    assert!(run_pending_cleanup_under(&mut recovered, &root));
    assert!(recovered.pending_cleanup.is_empty());
    assert_eq!(recovered.entries.len(), 1);
    assert!(!recovered.entries[0].managed_artifact);
    assert_eq!(std::fs::read(&original).unwrap(), b"user modified");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn cleanup_relinquish_resolution_replays_after_move_before_history_save() {
    let root = std::env::temp_dir().join(format!(
        "sgt-cleanup-relinquish-replay-{}-{}",
        std::process::id(),
        now_ms()
    ));
    let managed = root.join("images");
    std::fs::create_dir_all(&managed).unwrap();
    let original = managed.join("result.png");
    let quarantine = managed.join(".sgt-prune-replay.tmp");
    let recovered_path = managed.join("result-recovered-100-1.png");
    std::fs::write(&original, b"foreign").unwrap();
    std::fs::write(&quarantine, b"user modified").unwrap();
    let artifact = inspect_delivery_artifact(&quarantine.to_string_lossy()).unwrap();
    let history_entry = ResultHistoryEntry {
        output_path: original.to_string_lossy().to_string(),
        output_name: "result.png".to_string(),
        artifact_size_bytes: artifact.size_bytes,
        artifact_sha256: artifact.sha256.clone(),
        managed_artifact: true,
        ..entry(1, "image", artifact.size_bytes, true)
    };
    let store_path = root.join("history.json");
    let store = ResultHistoryStore {
        pending_cleanup: vec![PendingCleanup {
            output_path: original.to_string_lossy().to_string(),
            artifact_size_bytes: artifact.size_bytes,
            artifact_sha256: artifact.sha256,
            artifact_file_identity: artifact.file_identity,
            quarantine_path: quarantine.to_string_lossy().to_string(),
            history_entry: Some(history_entry),
            resolution: CleanupResolution::Relinquish {
                target_path: recovered_path.to_string_lossy().to_string(),
            },
        }],
        ..Default::default()
    };
    save_store(&store_path, &store).unwrap();
    std::fs::rename(&quarantine, &recovered_path).unwrap();

    let mut recovered = load_store(&store_path).unwrap();
    assert!(run_pending_cleanup_under(&mut recovered, &root));
    assert!(recovered.pending_cleanup.is_empty());
    assert_eq!(std::fs::read(&original).unwrap(), b"foreign");
    assert_eq!(std::fs::read(&recovered_path).unwrap(), b"user modified");
    assert_eq!(
        recovered.entries[0].output_path,
        recovered_path.to_string_lossy()
    );
    assert!(!recovered.entries[0].managed_artifact);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn cleanup_never_adopts_identical_replacement_before_queueing() {
    let root = std::env::temp_dir().join(format!(
        "sgt-cleanup-replacement-{}-{}",
        std::process::id(),
        now_ms()
    ));
    let managed = root.join("images");
    std::fs::create_dir_all(&managed).unwrap();
    let output = managed.join("result.png");
    std::fs::write(&output, b"same bytes").unwrap();
    let committed = inspect_delivery_artifact(&output.to_string_lossy()).unwrap();
    let history_entry = ResultHistoryEntry {
        id: "image-owned".to_string(),
        output_path: output.to_string_lossy().to_string(),
        output_name: "result.png".to_string(),
        artifact_size_bytes: committed.size_bytes,
        artifact_sha256: committed.sha256,
        managed_artifact: true,
        ..entry(1, "image", committed.size_bytes, true)
    };
    let mut store = ResultHistoryStore {
        delivery_identities: vec![HistoryDeliveryIdentity {
            tool: "image".to_string(),
            dispatch_id: "dispatch-owned".to_string(),
            entry_id: history_entry.id.clone(),
            artifact_file_identity: committed.file_identity,
        }],
        ..Default::default()
    };
    std::fs::remove_file(&output).unwrap();
    std::fs::write(&output, b"same bytes").unwrap();
    assert!(queue_managed_cleanup(&mut store, history_entry));
    assert!(prepare_cleanup_quarantines(&mut store));
    assert!(!run_pending_cleanup_under(&mut store, &root));
    assert_eq!(std::fs::read(&output).unwrap(), b"same bytes");
    assert_eq!(store.pending_cleanup.len(), 1);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn oversized_history_index_is_ignored_without_allocating_its_declared_size() {
    let root = std::env::temp_dir().join(format!(
        "sgt-oversized-result-history-{}-{}",
        std::process::id(),
        now_ms()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let store_path = root.join("history.json");
    let file = std::fs::File::create(&store_path).unwrap();
    file.set_len(MAX_HISTORY_INDEX_BYTES + 1).unwrap();

    assert!(load_store(&store_path).is_err());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn legacy_history_never_claims_unproven_file_ownership() {
    let first: ResultHistoryEntry = serde_json::from_value(serde_json::json!({
        "id": "legacy-1",
        "tool": "image",
        "sourcePath": "source.png",
        "outputPath": "first.png",
        "outputName": "first.png",
        "createdAtMs": 1,
        "metadata": null
    }))
    .unwrap();
    let second = ResultHistoryEntry {
        id: "legacy-2".to_string(),
        output_path: "second.png".to_string(),
        output_name: "second.png".to_string(),
        created_at_ms: 2,
        ..first.clone()
    };
    assert!(!first.managed_artifact);
    assert_eq!(first.artifact_size_bytes, 0);
    assert!(first.artifact_sha256.is_empty());
    let mut store = ResultHistoryStore {
        entries: vec![first, second],
        pending_cleanup: Vec::new(),
        ..Default::default()
    };

    assert!(prune_store(&mut store, 1));
    assert!(store.pending_cleanup.is_empty());
}

#[test]
fn filters_missing_results_and_renames_and_deletes_real_files() {
    let root = std::env::temp_dir().join(format!(
        "sgt-result-history-{}-{}",
        std::process::id(),
        now_ms()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let store_path = root.join("history.json");
    let output = root.join("model.glb");
    std::fs::write(&output, b"glTF").unwrap();

    let entry = record_at(
        &store_path,
        "3d",
        "source.png",
        output.to_str().unwrap(),
        serde_json::json!({ "isSegmented": true }),
        DEFAULT_RESULTS_PER_TOOL,
        (None, None),
    )
    .unwrap();
    assert_eq!(entry.artifact_size_bytes, 4);
    assert!(entry.artifact_sha256.is_empty());
    assert!(!entry.managed_artifact);
    assert_eq!(list_snapshot_at(&store_path, "3d").unwrap().len(), 1);
    assert_eq!(
        list_at(&store_path, "3d", DEFAULT_RESULTS_PER_TOOL)
            .unwrap()
            .len(),
        1
    );

    let renamed = rename_at(
        &store_path,
        "3d",
        &entry.id,
        "hero",
        DEFAULT_RESULTS_PER_TOOL,
    )
    .unwrap();
    assert!(renamed.output_path.ends_with("hero.glb"));
    assert!(Path::new(&renamed.output_path).is_file());

    delete_at(&store_path, "3d", &entry.id).unwrap();
    assert!(!Path::new(&renamed.output_path).exists());
    assert!(
        list_at(&store_path, "3d", DEFAULT_RESULTS_PER_TOOL)
            .unwrap()
            .is_empty()
    );

    let missing = root.join("missing.svg");
    std::fs::write(&missing, b"<svg/>").unwrap();
    record_at(
        &store_path,
        "svg",
        "source.png",
        missing.to_str().unwrap(),
        Value::Null,
        DEFAULT_RESULTS_PER_TOOL,
        (None, None),
    )
    .unwrap();
    std::fs::remove_file(&missing).unwrap();
    assert!(
        list_at(&store_path, "svg", DEFAULT_RESULTS_PER_TOOL)
            .unwrap()
            .is_empty()
    );
    let _ = std::fs::remove_dir_all(root);
}
