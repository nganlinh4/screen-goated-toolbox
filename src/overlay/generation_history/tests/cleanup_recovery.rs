use super::*;

fn presence(path: &Path) -> std::io::Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn inaccessible() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        "inspection unavailable",
    )
}

fn pending_modified_result() -> (PathBuf, PathBuf, PathBuf, ResultHistoryStore) {
    let root = persistence::paired_root("cleanup-inspection");
    let managed = root.join("images");
    std::fs::create_dir_all(&managed).unwrap();
    let original = managed.join("result.png");
    let quarantine = managed.join(".sgt-prune-test.tmp");
    std::fs::write(&original, b"committed").unwrap();
    let recorded = inspect_delivery_artifact(&original.to_string_lossy()).unwrap();
    let history_entry = ResultHistoryEntry {
        output_path: original.to_string_lossy().to_string(),
        output_name: "result.png".to_string(),
        artifact_size_bytes: recorded.size_bytes,
        artifact_sha256: recorded.sha256.clone(),
        managed_artifact: true,
        ..entry(1, "image", recorded.size_bytes, true)
    };
    std::fs::rename(&original, &quarantine).unwrap();
    std::fs::write(&quarantine, b"user modified").unwrap();
    let store = ResultHistoryStore {
        pending_cleanup: vec![PendingCleanup {
            output_path: original.to_string_lossy().to_string(),
            artifact_size_bytes: recorded.size_bytes,
            artifact_sha256: recorded.sha256,
            artifact_file_identity: recorded.file_identity,
            quarantine_path: quarantine.to_string_lossy().to_string(),
            history_entry: Some(history_entry),
            resolution: CleanupResolution::Pending,
        }],
        ..Default::default()
    };
    (root, original, quarantine, store)
}

#[test]
fn inaccessible_cleanup_paths_keep_the_intent_until_recovery() {
    for denied_original in [true, false] {
        let (root, original, quarantine, mut store) = pending_modified_result();
        let denied = if denied_original {
            &original
        } else {
            &quarantine
        };
        let inspect = |path: &Path| {
            if path == denied {
                Err(inaccessible())
            } else {
                presence(path)
            }
        };
        assert!(!run_pending_cleanup_under_with(&mut store, &root, &inspect));
        assert_eq!(store.pending_cleanup.len(), 1);
        assert!(matches!(
            store.pending_cleanup[0].resolution,
            CleanupResolution::Pending
        ));
        assert_eq!(std::fs::read(&quarantine).unwrap(), b"user modified");
        assert!(!original.exists());
        assert!(run_pending_cleanup_under(&mut store, &root));
        assert!(store.pending_cleanup.is_empty());
        assert_eq!(std::fs::read(&original).unwrap(), b"user modified");
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn inaccessible_quarantine_candidates_are_not_reserved() {
    let (root, original, quarantine, mut store) = pending_modified_result();
    store.pending_cleanup[0].quarantine_path.clear();
    assert!(!prepare_cleanup_quarantines_with(&mut store, &|_| Err(
        inaccessible()
    )));
    assert!(store.pending_cleanup[0].quarantine_path.is_empty());
    assert!(!run_pending_cleanup_under(&mut store, &root));
    assert_eq!(store.pending_cleanup.len(), 1);
    assert_eq!(std::fs::read(&quarantine).unwrap(), b"user modified");
    assert!(!original.exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn inaccessible_resolution_target_defers_replay() {
    let (root, original, quarantine, mut store) = pending_modified_result();
    store.pending_cleanup[0].resolution = CleanupResolution::Restore {
        target_path: original.to_string_lossy().to_string(),
    };
    let inspect = |path: &Path| {
        if path == original {
            Err(inaccessible())
        } else {
            presence(path)
        }
    };
    assert!(!run_pending_cleanup_under_with(&mut store, &root, &inspect));
    assert_eq!(store.pending_cleanup.len(), 1);
    assert_eq!(std::fs::read(&quarantine).unwrap(), b"user modified");
    assert!(!original.exists());
    assert!(run_pending_cleanup_under(&mut store, &root));
    assert_eq!(std::fs::read(&original).unwrap(), b"user modified");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn inaccessible_absent_quarantine_does_not_discard_intent() {
    let (root, original, quarantine, mut store) = pending_modified_result();
    let held = root.join("held-result");
    std::fs::rename(&quarantine, &held).unwrap();
    let inspect = |path: &Path| {
        if path == quarantine {
            Err(inaccessible())
        } else {
            presence(path)
        }
    };
    assert!(!run_pending_cleanup_under_with(&mut store, &root, &inspect));
    assert_eq!(store.pending_cleanup.len(), 1);
    assert!(!original.exists());
    assert_eq!(std::fs::read(&held).unwrap(), b"user modified");
    std::fs::rename(&held, &quarantine).unwrap();
    assert!(run_pending_cleanup_under(&mut store, &root));
    assert_eq!(std::fs::read(&original).unwrap(), b"user modified");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn unavailable_parent_does_not_make_cleanup_paths_absent() {
    let (root, original, quarantine, mut store) = pending_modified_result();
    let parent = original.parent().unwrap();
    let held_parent = root.join("temporarily-unavailable");
    std::fs::rename(parent, &held_parent).unwrap();
    assert!(!run_pending_cleanup_under(&mut store, &root));
    assert_eq!(store.pending_cleanup.len(), 1);
    assert_eq!(
        std::fs::read(held_parent.join(quarantine.file_name().unwrap())).unwrap(),
        b"user modified"
    );
    std::fs::rename(&held_parent, parent).unwrap();
    assert!(run_pending_cleanup_under(&mut store, &root));
    assert_eq!(std::fs::read(&original).unwrap(), b"user modified");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn replaced_quarantine_defers_a_completed_restore() {
    let (root, original, quarantine, mut store) = pending_modified_result();
    store.pending_cleanup[0].resolution = CleanupResolution::Restore {
        target_path: original.to_string_lossy().to_string(),
    };
    std::fs::rename(&quarantine, &original).unwrap();
    std::fs::write(&quarantine, b"replacement").unwrap();
    assert!(!run_pending_cleanup_under(&mut store, &root));
    assert_eq!(store.pending_cleanup.len(), 1);
    assert_eq!(std::fs::read(&quarantine).unwrap(), b"replacement");
    assert_eq!(std::fs::read(&original).unwrap(), b"user modified");
    std::fs::remove_file(&quarantine).unwrap();
    assert!(run_pending_cleanup_under(&mut store, &root));
    assert!(store.pending_cleanup.is_empty());
    assert_eq!(std::fs::read(&original).unwrap(), b"user modified");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn inaccessible_recovery_candidates_do_not_replace_or_move_files() {
    let (root, original, quarantine, mut store) = pending_modified_result();
    std::fs::write(&original, b"other user's file").unwrap();
    let inspect = |path: &Path| {
        if path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().contains("-recovered-"))
        {
            Err(inaccessible())
        } else {
            presence(path)
        }
    };
    assert!(!run_pending_cleanup_under_with(&mut store, &root, &inspect));
    assert_eq!(store.pending_cleanup.len(), 1);
    assert_eq!(std::fs::read(&quarantine).unwrap(), b"user modified");
    assert_eq!(std::fs::read(&original).unwrap(), b"other user's file");
    assert!(run_pending_cleanup_under(&mut store, &root));
    assert_eq!(std::fs::read(&original).unwrap(), b"other user's file");
    assert_eq!(
        std::fs::read(&store.entries[0].output_path).unwrap(),
        b"user modified"
    );
    std::fs::remove_dir_all(root).unwrap();
}
