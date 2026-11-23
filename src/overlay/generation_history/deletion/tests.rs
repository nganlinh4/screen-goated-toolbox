use super::*;

fn pair() -> (PathBuf, PendingDelete) {
    let root = std::env::temp_dir().join(format!(
        "sgt-delete-journal-{}-{}",
        std::process::id(),
        HISTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).unwrap();
    let primary = root.join("model.glb");
    let side = root.join("model.fbx");
    std::fs::write(&primary, b"primary").unwrap();
    std::fs::write(&side, b"companion").unwrap();
    let artifact = inspect_delivery_artifact(&primary.to_string_lossy()).unwrap();
    let companion = inspect_delivery_artifact(&side.to_string_lossy()).unwrap();
    let entry = ResultHistoryEntry {
        id: "delete-pair".into(),
        tool: "3d".into(),
        source_path: String::new(),
        output_path: primary.to_string_lossy().into_owned(),
        output_name: "model.glb".into(),
        created_at_ms: 1,
        artifact_size_bytes: artifact.size_bytes,
        artifact_sha256: artifact.sha256,
        managed_artifact: false,
        companion_origin_path: None,
        metadata: serde_json::json!({"download": {"path": side.to_string_lossy(), "name": "model.fbx", "sizeBytes": companion.size_bytes, "sha256": companion.sha256, "fileIdentity": companion.file_identity}}),
    };
    (
        root,
        PendingDelete {
            entry,
            artifact_file_identity: artifact.file_identity,
        },
    )
}

#[test]
fn journal_replays_before_and_after_first_file_is_removed() {
    for removed_first in [false, true] {
        let (root, pending) = pair();
        let path = root.join("history.json");
        if removed_first {
            std::fs::remove_file(root.join("model.fbx")).unwrap();
        }
        let mut store = ResultHistoryStore {
            pending_deletes: vec![pending],
            ..Default::default()
        };
        save_store(&path, &store).unwrap();
        store = load_store(&path).unwrap();
        assert!(pending_path(
            &store,
            &root.join("model.glb").to_string_lossy()
        ));
        assert!(pending_path(
            &store,
            &root.join("model.fbx").to_string_lossy()
        ));
        assert!(reconcile_pending_deletes(&mut store));
        assert!(store.pending_deletes.is_empty());
        assert!(!root.join("model.glb").exists());
        assert!(!root.join("model.fbx").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn replay_preserves_replaced_files_and_restores_the_entry() {
    for replace_side in [false, true] {
        let (root, pending) = pair();
        let changed = root.join(if replace_side {
            "model.fbx"
        } else {
            "model.glb"
        });
        std::fs::remove_file(&changed).unwrap();
        std::fs::write(&changed, b"replacement").unwrap();
        let mut store = ResultHistoryStore {
            pending_deletes: vec![pending],
            ..Default::default()
        };
        assert!(reconcile_pending_deletes(&mut store));
        assert!(store.pending_deletes.is_empty());
        assert_eq!(store.entries.len(), 1);
        assert_eq!(std::fs::read(changed).unwrap(), b"replacement");
        assert!(root.join("model.glb").exists());
        assert!(root.join("model.fbx").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn malformed_or_unbound_companion_never_deletes_primary() {
    for orphaned in [false, true] {
        let (root, mut pending) = pair();
        if orphaned {
            let renamed = root.join("renamed.glb");
            std::fs::rename(&pending.entry.output_path, &renamed).unwrap();
            pending.entry.output_path = renamed.to_string_lossy().into_owned();
        } else {
            pending.entry.metadata["download"] = serde_json::json!({"path": "invalid"});
        }
        let history = root.join("history.json");
        let entry = pending.entry.clone();
        let recorded = ResultHistoryStore {
            entries: vec![entry.clone()],
            delivery_identities: vec![HistoryDeliveryIdentity {
                tool: entry.tool.clone(),
                entry_id: entry.id.clone(),
                dispatch_id: "dispatch".into(),
                artifact_file_identity: pending.artifact_file_identity.clone(),
            }],
            ..Default::default()
        };
        save_store(&history, &recorded).unwrap();
        assert!(delete_at(&history, &entry.tool, &entry.id).is_err());
        assert_eq!(load_store(&history).unwrap().entries.len(), 1);
        let mut store = ResultHistoryStore {
            pending_deletes: vec![pending],
            ..Default::default()
        };
        assert!(!reconcile_pending_deletes(&mut store));
        assert_eq!(store.pending_deletes.len(), 1);
        assert!(Path::new(&store.pending_deletes[0].entry.output_path).exists());
        assert!(root.join("model.fbx").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn absent_companion_is_distinct_from_invalid_metadata() {
    let (root, mut pending) = pair();
    pending.entry.metadata = serde_json::json!({});
    let mut store = ResultHistoryStore {
        pending_deletes: vec![pending],
        ..Default::default()
    };
    assert!(reconcile_pending_deletes(&mut store));
    assert!(!root.join("model.glb").exists());
    assert!(root.join("model.fbx").exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn admission_rejects_pending_artifact_aliases_and_companion_references() {
    let (root, pending) = pair();
    let alias = root.join("alias.fbx");
    std::fs::hard_link(root.join("model.fbx"), &alias).unwrap();
    let unrelated = root.join("other.glb");
    std::fs::write(&unrelated, b"unrelated").unwrap();
    let store = ResultHistoryStore {
        pending_deletes: vec![pending],
        ..Default::default()
    };
    assert!(pending_admission(
        &store,
        &alias.to_string_lossy(),
        &Value::Null
    ));
    assert!(pending_admission(
        &store,
        &unrelated.to_string_lossy(),
        &serde_json::json!({"download":{"path":alias}})
    ));
    assert!(!pending_admission(
        &store,
        &unrelated.to_string_lossy(),
        &Value::Null
    ));
    assert!(pending_admission(
        &store,
        &unrelated.to_string_lossy(),
        &serde_json::json!({"download":{"path":root.join("missing.fbx")}})
    ));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn deleting_shared_primary_or_companion_preserves_both_entries() {
    for shared_companion in [false, true] {
        let (root, pending) = pair();
        let mut other = pending.entry.clone();
        other.id = "other".into();
        if shared_companion {
            let output = root.join("other.glb");
            let side = root.join("other.fbx");
            std::fs::write(&output, b"other").unwrap();
            std::fs::hard_link(root.join("model.fbx"), &side).unwrap();
            other.output_path = output.to_string_lossy().into_owned();
            other.metadata = serde_json::json!({"download":{"path":side}});
        } else {
            let alias = root.join("other.glb");
            std::fs::hard_link(root.join("model.glb"), &alias).unwrap();
            other.output_path = alias.to_string_lossy().into_owned();
            other.metadata = Value::Null;
        }
        let history = root.join("history.json");
        let store = ResultHistoryStore {
            entries: vec![pending.entry.clone(), other],
            delivery_identities: vec![HistoryDeliveryIdentity {
                tool: "3d".into(),
                entry_id: pending.entry.id.clone(),
                dispatch_id: "dispatch".into(),
                artifact_file_identity: pending.artifact_file_identity.clone(),
            }],
            ..Default::default()
        };
        save_store(&history, &store).unwrap();
        assert!(delete_at(&history, "3d", &pending.entry.id).is_err());
        assert_eq!(load_store(&history).unwrap().entries.len(), 2);
        assert!(root.join("model.glb").exists());
        assert!(root.join("model.fbx").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
