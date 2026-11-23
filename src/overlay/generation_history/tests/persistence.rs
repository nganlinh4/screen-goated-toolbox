use super::*;

pub(super) fn paired_result(root: &Path) -> (PathBuf, ResultHistoryEntry, PathBuf, PathBuf) {
    std::fs::create_dir_all(root).unwrap();
    let store_path = root.join("history.json");
    let output = root.join("original.glb");
    let companion = root.join("original.fbx");
    std::fs::write(&output, b"primary model").unwrap();
    std::fs::write(&companion, b"companion model").unwrap();
    let primary = inspect_delivery_artifact(output.to_str().unwrap()).unwrap();
    let side = inspect_delivery_artifact(companion.to_str().unwrap()).unwrap();
    let entry = record_at(
        &store_path,
        "3d",
        "",
        output.to_str().unwrap(),
        serde_json::json!({"download": {
            "path": companion.to_str().unwrap(), "name": "original.fbx",
            "sizeBytes": side.size_bytes, "sha256": side.sha256,
            "fileIdentity": side.file_identity
        }}),
        DEFAULT_RESULTS_PER_TOOL,
        (
            Some((primary.size_bytes, primary.sha256, primary.managed)),
            Some(DeliveryIdentity {
                dispatch_id: "paired-result",
                artifact_file_identity: &primary.file_identity,
            }),
        ),
    )
    .unwrap();
    (store_path, entry, output, companion)
}

pub(super) fn paired_root(test: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "sgt-history-{test}-{}-{}",
        std::process::id(),
        HISTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ))
}

#[test]
fn renamed_model_keeps_companion_for_export_and_delete() {
    let root = paired_root("rename-companion");
    let (store_path, entry, _, companion) = paired_result(&root);
    rename_at(
        &store_path,
        "3d",
        &entry.id,
        "renamed",
        DEFAULT_RESULTS_PER_TOOL,
    )
    .unwrap();
    let renamed = rename_at(
        &store_path,
        "3d",
        &entry.id,
        "again",
        DEFAULT_RESULTS_PER_TOOL,
    )
    .unwrap();
    assert_eq!(renamed.output_name, "again.glb");
    assert_eq!(
        validated_companion_path(&renamed).unwrap(),
        Some(companion.clone())
    );
    assert!(
        serde_json::to_value(public_entry(&renamed))
            .unwrap()
            .get("companionOriginPath")
            .is_none()
    );
    delete_at(&store_path, "3d", &entry.id).unwrap();
    assert!(!Path::new(&renamed.output_path).exists());
    assert!(!companion.exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn renamed_model_rejects_a_replaced_companion() {
    let root = paired_root("rename-replaced-companion");
    let (store_path, entry, _, companion) = paired_result(&root);
    let renamed = rename_at(
        &store_path,
        "3d",
        &entry.id,
        "renamed",
        DEFAULT_RESULTS_PER_TOOL,
    )
    .unwrap();
    std::fs::remove_file(&companion).unwrap();
    std::fs::write(&companion, b"replacement").unwrap();
    assert!(validated_companion_path(&renamed).is_err());
    assert!(delete_at(&store_path, "3d", &entry.id).is_err());
    assert_eq!(std::fs::read(&companion).unwrap(), b"replacement");
    assert!(Path::new(&renamed.output_path).exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn rename_does_not_bind_a_replaced_companion() {
    let root = paired_root("rename-replaced-before");
    let (store_path, entry, output, companion) = paired_result(&root);
    std::fs::remove_file(&companion).unwrap();
    std::fs::write(&companion, b"replacement").unwrap();
    assert!(
        rename_at(
            &store_path,
            "3d",
            &entry.id,
            "renamed",
            DEFAULT_RESULTS_PER_TOOL
        )
        .is_err()
    );
    assert!(output.exists());
    assert_eq!(std::fs::read(&companion).unwrap(), b"replacement");
    let store = load_store(&store_path).unwrap();
    assert!(store.entries[0].companion_origin_path.is_none());
    assert!(store.pending_renames.is_empty());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn companion_binding_survives_each_primary_rename_crash_window() {
    for moved in [false, true] {
        let root = paired_root("rename-companion-crash");
        let (store_path, entry, previous, companion) = paired_result(&root);
        let next = root.join("renamed.glb");
        let artifact = inspect_delivery_artifact(&previous.to_string_lossy()).unwrap();
        let mut store = load_store(&store_path).unwrap();
        store.entries[0].companion_origin_path = Some(previous.to_string_lossy().to_string());
        store.pending_renames.push(PendingRename {
            tool: "3d".to_string(),
            entry_id: entry.id.clone(),
            previous_path: previous.to_string_lossy().to_string(),
            next_path: next.to_string_lossy().to_string(),
            next_name: "renamed.glb".to_string(),
            artifact_size_bytes: artifact.size_bytes,
            artifact_sha256: artifact.sha256,
            artifact_file_identity: artifact.file_identity,
        });
        save_store(&store_path, &store).unwrap();
        if moved {
            std::fs::rename(&previous, &next).unwrap();
        }
        assert!(list_snapshot_at(&store_path, "3d").unwrap().is_empty());
        if moved {
            maintain_at(&store_path).unwrap();
            assert!(load_store(&store_path).unwrap().pending_renames.is_empty());
        }
        let listed = list_at(&store_path, "3d", DEFAULT_RESULTS_PER_TOOL).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(
            Path::new(&listed[0].output_path),
            if moved { &next } else { &previous }
        );
        assert_eq!(
            validated_companion_path(&listed[0]).unwrap(),
            Some(companion)
        );
        assert!(load_store(&store_path).unwrap().pending_renames.is_empty());
        delete_at(&store_path, "3d", &entry.id).unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn crash_recovery_does_not_trust_a_replaced_companion() {
    let root = paired_root("rename-companion-crash-replaced");
    let (store_path, entry, previous, companion) = paired_result(&root);
    let next = root.join("renamed.glb");
    let artifact = inspect_delivery_artifact(&previous.to_string_lossy()).unwrap();
    let mut store = load_store(&store_path).unwrap();
    store.entries[0].companion_origin_path = Some(previous.to_string_lossy().to_string());
    store.pending_renames.push(PendingRename {
        tool: "3d".to_string(),
        entry_id: entry.id.clone(),
        previous_path: previous.to_string_lossy().to_string(),
        next_path: next.to_string_lossy().to_string(),
        next_name: "renamed.glb".to_string(),
        artifact_size_bytes: artifact.size_bytes,
        artifact_sha256: artifact.sha256,
        artifact_file_identity: artifact.file_identity,
    });
    save_store(&store_path, &store).unwrap();
    std::fs::rename(&previous, &next).unwrap();
    std::fs::remove_file(&companion).unwrap();
    std::fs::write(&companion, b"replacement").unwrap();
    let listed = list_at(&store_path, "3d", DEFAULT_RESULTS_PER_TOOL).unwrap();
    assert_eq!(Path::new(&listed[0].output_path), next);
    assert!(validated_companion_path(&listed[0]).is_err());
    assert!(delete_at(&store_path, "3d", &entry.id).is_err());
    assert!(next.exists());
    assert_eq!(std::fs::read(&companion).unwrap(), b"replacement");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn delete_reconciles_a_pending_primary_rename_first() {
    let root = paired_root("rename-companion-crash-delete");
    let (store_path, entry, previous, companion) = paired_result(&root);
    let next = root.join("renamed.glb");
    let artifact = inspect_delivery_artifact(&previous.to_string_lossy()).unwrap();
    let mut store = load_store(&store_path).unwrap();
    store.entries[0].companion_origin_path = Some(previous.to_string_lossy().to_string());
    store.pending_renames.push(PendingRename {
        tool: "3d".to_string(),
        entry_id: entry.id.clone(),
        previous_path: previous.to_string_lossy().to_string(),
        next_path: next.to_string_lossy().to_string(),
        next_name: "renamed.glb".to_string(),
        artifact_size_bytes: artifact.size_bytes,
        artifact_sha256: artifact.sha256,
        artifact_file_identity: artifact.file_identity,
    });
    save_store(&store_path, &store).unwrap();
    std::fs::rename(&previous, &next).unwrap();
    delete_at(&store_path, "3d", &entry.id).unwrap();
    assert!(!next.exists());
    assert!(!companion.exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn frontend_metadata_cannot_supply_a_companion_origin_binding() {
    let root = paired_root("metadata-origin");
    let (store_path, entry, _, companion) = paired_result(&root);
    let mut store = load_store(&store_path).unwrap();
    let unrelated = root.join("other.fbx");
    std::fs::rename(&companion, &unrelated).unwrap();
    let side = inspect_delivery_artifact(&unrelated.to_string_lossy()).unwrap();
    // A file newer than the record cannot be the companion it published, so
    // neither metadata nor the legacy-rename migration may bind it.
    store.entries[0].created_at_ms = 1;
    store.entries[0].metadata = serde_json::json!({
        "companionOriginPath": root.join("other.glb").to_string_lossy(),
        "download": {"path": unrelated.to_string_lossy(), "name": "other.fbx",
            "sizeBytes": side.size_bytes, "sha256": side.sha256, "fileIdentity": side.file_identity}
    });
    save_store(&store_path, &store).unwrap();
    let result = rename_at(
        &store_path,
        "3d",
        &entry.id,
        "renamed",
        DEFAULT_RESULTS_PER_TOOL,
    );
    assert!(result.is_err());
    assert!(delete_at(&store_path, "3d", &entry.id).is_err());
    assert!(Path::new(&entry.output_path).exists());
    assert_eq!(std::fs::read(&unrelated).unwrap(), b"companion model");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn changed_companion_prevents_deleting_the_primary() {
    let root = std::env::temp_dir().join(format!(
        "sgt-history-companion-{}-{}",
        std::process::id(),
        HISTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).unwrap();
    let store_path = root.join("history.json");
    let output = root.join("result.glb");
    let companion = root.join("result.fbx");
    std::fs::write(&output, b"original model").unwrap();
    std::fs::write(&companion, b"original companion").unwrap();
    let artifact = inspect_delivery_artifact(output.to_str().unwrap()).unwrap();
    let side = inspect_delivery_artifact(companion.to_str().unwrap()).unwrap();
    let metadata = serde_json::json!({"download": {
        "path": companion.to_str().unwrap(), "name": "result.fbx",
        "sizeBytes": side.size_bytes, "sha256": side.sha256, "fileIdentity": side.file_identity
    }});
    let entry = record_at(
        &store_path,
        "3d",
        "",
        output.to_str().unwrap(),
        metadata,
        DEFAULT_RESULTS_PER_TOOL,
        (
            Some((artifact.size_bytes, artifact.sha256, artifact.managed)),
            Some(DeliveryIdentity {
                dispatch_id: "companion-test",
                artifact_file_identity: &artifact.file_identity,
            }),
        ),
    )
    .unwrap();
    std::fs::remove_file(&companion).unwrap();
    std::fs::write(&companion, b"replacement companion").unwrap();
    assert!(delete_at(&store_path, "3d", &entry.id).is_err());
    assert_eq!(std::fs::read(&output).unwrap(), b"original model");
    assert_eq!(std::fs::read(&companion).unwrap(), b"replacement companion");
    assert_eq!(load_store(&store_path).unwrap().entries.len(), 1);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn deleting_a_replaced_result_preserves_the_file_and_history() {
    let root = std::env::temp_dir().join(format!(
        "sgt-history-ownership-{}-{}",
        std::process::id(),
        HISTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).unwrap();
    let store_path = root.join("history.json");
    let output = root.join("result.png");
    std::fs::write(&output, b"original").unwrap();
    let artifact = inspect_delivery_artifact(output.to_str().unwrap()).unwrap();
    let entry = record_at(
        &store_path,
        "image",
        "",
        output.to_str().unwrap(),
        Value::Null,
        DEFAULT_RESULTS_PER_TOOL,
        (
            Some((artifact.size_bytes, artifact.sha256, artifact.managed)),
            Some(DeliveryIdentity {
                dispatch_id: "ownership-test",
                artifact_file_identity: &artifact.file_identity,
            }),
        ),
    )
    .unwrap();
    std::fs::write(&output, b"modified").unwrap();
    assert!(delete_at(&store_path, "image", &entry.id).is_err());
    assert_eq!(std::fs::read(&output).unwrap(), b"modified");
    std::fs::remove_file(&output).unwrap();
    std::fs::write(&output, b"replacement").unwrap();
    assert!(delete_at(&store_path, "image", &entry.id).is_err());
    assert!(
        rename_at(
            &store_path,
            "image",
            &entry.id,
            "renamed",
            DEFAULT_RESULTS_PER_TOOL
        )
        .is_err()
    );
    assert_eq!(std::fs::read(&output).unwrap(), b"replacement");
    assert_eq!(load_store(&store_path).unwrap().entries.len(), 1);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn legacy_history_can_be_removed_without_deleting_its_unverified_file() {
    let root = std::env::temp_dir().join(format!(
        "sgt-history-legacy-{}-{}",
        std::process::id(),
        HISTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).unwrap();
    let store_path = root.join("history.json");
    let output = root.join("result.png");
    std::fs::write(&output, b"keep this file").unwrap();
    let entry = record_at(
        &store_path,
        "image",
        "",
        output.to_str().unwrap(),
        Value::Null,
        DEFAULT_RESULTS_PER_TOOL,
        (None, None),
    )
    .unwrap();
    delete_at(&store_path, "image", &entry.id).unwrap();
    assert_eq!(std::fs::read(&output).unwrap(), b"keep this file");
    assert!(load_store(&store_path).unwrap().entries.is_empty());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn dispatch_identity_is_idempotent_without_content_deduplication() {
    let root = std::env::temp_dir().join(format!(
        "sgt-result-dispatch-{}-{}",
        std::process::id(),
        now_ms()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let store_path = root.join("history.json");
    let first = root.join("first.png");
    let second = root.join("second.png");
    std::fs::write(&first, b"same image bytes").unwrap();
    std::fs::write(&second, b"same image bytes").unwrap();
    let (size, digest) = digest_file(&first).unwrap();
    let first_identity = crate::overlay::creation_file_identity::from_path(&first).unwrap();
    let second_identity = crate::overlay::creation_file_identity::from_path(&second).unwrap();
    let metadata = serde_json::json!({"operation": "create_image"});

    let first_entry = record_at(
        &store_path,
        "image",
        "",
        first.to_str().unwrap(),
        metadata.clone(),
        DEFAULT_RESULTS_PER_TOOL,
        (
            Some((size, digest.clone(), false)),
            Some(DeliveryIdentity {
                dispatch_id: "dispatch-one",
                artifact_file_identity: &first_identity,
            }),
        ),
    )
    .unwrap();
    let replay = record_at(
        &store_path,
        "image",
        "",
        first.to_str().unwrap(),
        metadata.clone(),
        DEFAULT_RESULTS_PER_TOOL,
        (
            Some((size, digest.clone(), false)),
            Some(DeliveryIdentity {
                dispatch_id: "dispatch-one",
                artifact_file_identity: &first_identity,
            }),
        ),
    )
    .unwrap();
    assert_eq!(replay.id, first_entry.id);

    let second_entry = record_at(
        &store_path,
        "image",
        "",
        second.to_str().unwrap(),
        metadata,
        DEFAULT_RESULTS_PER_TOOL,
        (
            Some((size, digest, false)),
            Some(DeliveryIdentity {
                dispatch_id: "dispatch-two",
                artifact_file_identity: &second_identity,
            }),
        ),
    )
    .unwrap();
    assert_ne!(second_entry.id, first_entry.id);
    assert_eq!(
        list_at(&store_path, "image", DEFAULT_RESULTS_PER_TOOL)
            .unwrap()
            .len(),
        2
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn rename_journal_reconciles_crash_after_physical_rename() {
    let root = std::env::temp_dir().join(format!(
        "sgt-result-rename-crash-{}-{}",
        std::process::id(),
        now_ms()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let store_path = root.join("history.json");
    let previous = root.join("before.svg");
    let next = root.join("after.svg");
    std::fs::write(&previous, b"<svg/>").unwrap();
    let entry = record_at(
        &store_path,
        "svg",
        "source.png",
        previous.to_str().unwrap(),
        Value::Null,
        DEFAULT_RESULTS_PER_TOOL,
        (None, None),
    )
    .unwrap();
    let mut store = load_store(&store_path).unwrap();
    let artifact = inspect_delivery_artifact(&previous.to_string_lossy()).unwrap();
    store.pending_renames.push(PendingRename {
        tool: "svg".to_string(),
        entry_id: entry.id.clone(),
        previous_path: previous.to_string_lossy().to_string(),
        next_path: next.to_string_lossy().to_string(),
        next_name: "after.svg".to_string(),
        artifact_size_bytes: artifact.size_bytes,
        artifact_sha256: artifact.sha256,
        artifact_file_identity: artifact.file_identity,
    });
    save_store(&store_path, &store).unwrap();
    std::fs::rename(&previous, &next).unwrap();

    let listed = list_at(&store_path, "svg", DEFAULT_RESULTS_PER_TOOL).unwrap();
    assert_eq!(listed[0].output_path, next.to_string_lossy());
    assert!(load_store(&store_path).unwrap().pending_renames.is_empty());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn corrupt_history_index_is_preserved_and_never_overwritten() {
    let root = std::env::temp_dir().join(format!(
        "sgt-result-corrupt-{}-{}",
        std::process::id(),
        now_ms()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let store_path = root.join("history.json");
    let output = root.join("result.png");
    let corrupt = b"{ this-is-not-history-json";
    std::fs::write(&store_path, corrupt).unwrap();
    std::fs::write(&output, b"png bytes").unwrap();

    assert!(
        record_at(
            &store_path,
            "image",
            "",
            output.to_str().unwrap(),
            Value::Null,
            DEFAULT_RESULTS_PER_TOOL,
            (None, None),
        )
        .is_err()
    );
    assert_eq!(std::fs::read(&store_path).unwrap(), corrupt);
    let _ = std::fs::remove_dir_all(root);
}
