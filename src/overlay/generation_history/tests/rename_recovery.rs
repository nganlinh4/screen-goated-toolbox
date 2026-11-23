use super::*;

#[cfg(windows)]
#[test]
fn inaccessible_renamed_result_is_retained_hidden_and_recovers() {
    use std::os::windows::fs::OpenOptionsExt as _;
    use windows::Win32::Storage::FileSystem::FILE_SHARE_READ;

    let root = persistence::paired_root("rename-sharing");
    let (store_path, entry, previous, companion) = persistence::paired_result(&root);
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
    let blocker = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ.0)
        .open(&next)
        .unwrap();

    assert!(list_at(&store_path, "3d", 0).unwrap().is_empty());
    assert!(list_snapshot_at(&store_path, "3d").unwrap().is_empty());
    assert!(delete_at(&store_path, "3d", &entry.id).is_err());
    assert!(rename_at(&store_path, "3d", &entry.id, "other", 0).is_err());
    let saved = load_store(&store_path).unwrap();
    assert_eq!(saved.entries.len(), 1);
    assert_eq!(saved.pending_renames.len(), 1);
    assert!(saved.pending_cleanup.is_empty());
    assert!(next.exists());
    assert!(companion.exists());

    drop(blocker);
    let recovered = list_at(&store_path, "3d", DEFAULT_RESULTS_PER_TOOL).unwrap();
    assert_eq!(recovered.len(), 1);
    assert_eq!(Path::new(&recovered[0].output_path), next);
    assert_eq!(
        validated_companion_path(&recovered[0]).unwrap(),
        Some(companion)
    );
    assert!(load_store(&store_path).unwrap().pending_renames.is_empty());
    delete_at(&store_path, "3d", &entry.id).unwrap();
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn unverified_primary_cannot_schedule_companion_cleanup() {
    let root = persistence::paired_root("unverified-primary");
    let (store_path, mut entry, _, companion) = persistence::paired_result(&root);
    let mut store = load_store(&store_path).unwrap();
    entry.managed_artifact = true;
    store.delivery_identities.clear();
    assert!(!queue_managed_cleanup(&mut store, entry));
    assert!(store.pending_cleanup.is_empty());
    assert!(companion.exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn rename_recovery_defers_only_conflicting_delivery_paths() {
    for maintenance in [false, true] {
        let root = persistence::paired_root("rename-protected");
        let (store_path, entry, previous, companion) = persistence::paired_result(&root);
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
        let overlapping = protected::ProtectedPaths::from_paths([next.clone()]).unwrap();
        maintain_at_protected(&store_path, &overlapping).unwrap();
        assert_eq!(load_store(&store_path).unwrap().pending_renames.len(), 1);
        let unrelated_path = root.join("unrelated.glb");
        if maintenance {
            let unrelated = protected::ProtectedPaths::from_paths([unrelated_path]).unwrap();
            maintain_at_protected(&store_path, &unrelated).unwrap();
        } else {
            let mut store = load_store(&store_path).unwrap();
            let unrelated =
                std::collections::HashSet::from([path_identity(&unrelated_path.to_string_lossy())]);
            reconcile_store_protected(
                &store_path,
                &mut store,
                DEFAULT_RESULTS_PER_TOOL,
                &unrelated,
            )
            .unwrap();
        }
        let recovered = load_store(&store_path).unwrap();
        assert!(recovered.pending_renames.is_empty());
        assert_eq!(Path::new(&recovered.entries[0].output_path), next);
        assert!(companion.exists());
        delete_at(&store_path, "3d", &entry.id).unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }
}
