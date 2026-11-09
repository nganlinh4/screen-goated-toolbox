use super::*;

/// An edit that replaces a delivered result must leave the result manageable.
///
/// History records the size, digest and file identity of every delivered
/// result, so an in-place edit makes every later rename, delete and cleanup
/// check answer "Result file changed since it was recorded" until the proof is
/// re-recorded. The SVG editor's save path is exactly this: replace the file,
/// then re-pin the proof with `refresh_artifact_proof`.
#[test]
fn in_place_edit_is_manageable_after_the_proof_is_refreshed() {
    let root = std::env::temp_dir().join(format!(
        "sgt-history-refresh-{}-{}",
        std::process::id(),
        HISTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).unwrap();
    let store_path = root.join("history.json");
    let output = root.join("vector.svg");
    std::fs::write(&output, b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>").unwrap();

    let delivered = inspect_delivery_artifact(output.to_str().unwrap()).unwrap();
    let entry = record_at(
        &store_path,
        "svg",
        "",
        output.to_str().unwrap(),
        serde_json::json!({}),
        DEFAULT_RESULTS_PER_TOOL,
        (
            Some((
                delivered.size_bytes,
                delivered.sha256.clone(),
                delivered.managed,
            )),
            Some(DeliveryIdentity {
                dispatch_id: "refresh-proof",
                artifact_file_identity: &delivered.file_identity,
            }),
        ),
    )
    .unwrap();
    assert_eq!(entry.artifact_size_bytes, delivered.size_bytes);

    // The editor replaces the file with different bytes.
    std::fs::write(
        &output,
        b"<svg xmlns=\"http://www.w3.org/2000/svg\"><path d=\"M0 0h8v8z\"/></svg>",
    )
    .unwrap();

    // Until the proof is re-recorded the store still describes the pre-edit bytes.
    let store = load_store(&store_path).unwrap();
    let stale = store
        .entries
        .iter()
        .find(|candidate| candidate.id == entry.id)
        .unwrap();
    assert_eq!(
        stale.artifact_size_bytes,
        b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>".len() as u64
    );
    assert_eq!(stale.artifact_sha256, delivered.sha256);

    super::super::refresh::refresh_at(&store_path, "svg", output.to_str().unwrap()).unwrap();

    let store = load_store(&store_path).unwrap();
    let refreshed = store
        .entries
        .iter()
        .find(|candidate| candidate.id == entry.id)
        .unwrap();
    assert_eq!(
        refreshed.artifact_size_bytes,
        std::fs::metadata(&output).unwrap().len()
    );
    assert_ne!(refreshed.artifact_sha256, delivered.sha256);

    // The re-recorded proof restores the manageable rename path.
    let renamed = rename_at(
        &store_path,
        "svg",
        &entry.id,
        "renamed",
        DEFAULT_RESULTS_PER_TOOL,
    )
    .unwrap();
    assert!(renamed.output_path.ends_with("renamed.svg"));
    assert!(Path::new(&renamed.output_path).is_file());

    let _ = std::fs::remove_dir_all(&root);
}
