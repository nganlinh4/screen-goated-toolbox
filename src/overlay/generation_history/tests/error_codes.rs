use super::*;

#[test]
fn rename_failures_reach_the_reply_as_stable_codes() {
    let root = persistence::paired_root("rename-error-codes");
    let (history, entry, _, _) = persistence::paired_result(&root);
    std::fs::write(root.join("taken.glb"), b"other").unwrap();
    for (name, code) in [
        ("taken", ErrorCode::NameTaken),
        ("bad/name", ErrorCode::NameInvalid),
        ("nul", ErrorCode::NameInvalid),
    ] {
        let error =
            rename_at(&history, "3d", &entry.id, name, DEFAULT_RESULTS_PER_TOOL).unwrap_err();
        assert_eq!(error.code(), code, "{name}");
        let reply = crate::overlay::creation_commands::reply_script("7", Err(error));
        assert!(reply.contains(&format!("\"error\":\"{}\"", code.as_str())));
        assert!(!reply.contains("taken.glb"));
    }
    let mut store = load_store(&history).unwrap();
    store.pending_renames.push(PendingRename {
        tool: "3d".into(),
        entry_id: entry.id.clone(),
        previous_path: String::new(),
        next_path: String::new(),
        next_name: String::new(),
        artifact_size_bytes: 0,
        artifact_sha256: String::new(),
        artifact_file_identity: String::new(),
    });
    save_store(&history, &store).unwrap();
    for error in [
        rename_at(&history, "3d", &entry.id, "next", DEFAULT_RESULTS_PER_TOOL).unwrap_err(),
        delete_at(&history, "3d", &entry.id).unwrap_err(),
    ] {
        assert_eq!(error.code(), ErrorCode::ResultBusy);
    }
    std::fs::remove_dir_all(root).unwrap();
}
