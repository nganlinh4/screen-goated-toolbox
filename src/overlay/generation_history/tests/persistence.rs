use super::*;

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
