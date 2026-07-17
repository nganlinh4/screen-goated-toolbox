use super::*;

#[test]
fn same_snapshot_edits_are_serialized_and_only_one_commits() {
    let fixture = Fixture::new(b"base");
    let expected = fixture.hash();
    let barrier = Arc::new(Barrier::new(3));
    let handles: Vec<_> = ["first", "second"]
        .into_iter()
        .map(|replacement| {
            let path = fixture.path.clone();
            let expected = expected.clone();
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                edit_text_file(&json!({
                    "path": path,
                    "expected_sha256": expected,
                    "replacements": [{
                        "old_text": "base",
                        "new_text": replacement,
                        "expected_count": 1
                    }]
                }))
            })
        })
        .collect();
    barrier.wait();
    let results: Vec<Value> = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect();
    assert_eq!(
        results.iter().filter(|result| result["ok"] == true).count(),
        1
    );
    let rejected = results.iter().find(|result| result["ok"] == false).unwrap();
    assert_eq!(rejected["code"], "ERR_TEXT_FILE_STALE");
    assert_eq!(rejected["tool_mutated_file"], false);
    assert_eq!(rejected["external_change_detected"], true);
    assert!(matches!(
        fs::read_to_string(&fixture.path).unwrap().as_str(),
        "first" | "second"
    ));
}

#[test]
fn in_place_write_after_validation_is_audited_and_preserved() {
    let fixture = Fixture::new(b"base");
    let path = fs::canonicalize(&fixture.path).unwrap();
    let mut guard = EditGuard::acquire(&path).unwrap();
    let original = guard.read_bounded(MAX_FILE_BYTES).unwrap();
    let staged = write_synced_sibling(&path, b"tool-version").unwrap();
    let current = guard
        .validate_current(&path, &original, MAX_FILE_BYTES)
        .unwrap();

    fs::write(&path, b"external-version").unwrap();
    let outcome = guard.commit_audited(
        &path,
        current,
        staged,
        &original,
        b"tool-version",
        MAX_FILE_BYTES,
    );
    let CommitOutcome::Ambiguous {
        tool_mutated_file,
        external_change_detected,
        recovery_backup: Some(recovery_backup),
        recovery_sha256: Some(recovery_sha256),
        ..
    } = outcome
    else {
        panic!("in-place replacement must produce an audited ambiguity");
    };
    assert!(tool_mutated_file);
    assert!(external_change_detected);
    assert_eq!(fs::read(&path).unwrap(), b"tool-version");
    assert_eq!(fs::read(&recovery_backup).unwrap(), b"external-version");
    assert_eq!(recovery_sha256, sha256_hex(b"external-version"));
}

#[test]
fn staged_cleanup_preserves_a_foreign_namespace_replacement() {
    let fixture = Fixture::new(b"base");
    let staged = write_synced_sibling(&fixture.path, b"tool-version").unwrap();
    let staged_path = staged.path().to_path_buf();
    let displaced_path = fixture.dir.join("displaced-stage.tmp");

    fs::rename(&staged_path, &displaced_path).unwrap();
    fs::write(&staged_path, b"foreign-version").unwrap();
    drop(staged);

    assert_eq!(fs::read(&staged_path).unwrap(), b"foreign-version");
    assert_eq!(fs::read(&displaced_path).unwrap(), b"tool-version");
}

#[test]
fn namespace_replacement_after_validation_is_audited_and_preserved() {
    let fixture = Fixture::new(b"base");
    let path = fs::canonicalize(&fixture.path).unwrap();
    let mut guard = EditGuard::acquire(&path).unwrap();
    let original = guard.read_bounded(MAX_FILE_BYTES).unwrap();
    let staged = write_synced_sibling(&path, b"tool-version").unwrap();
    let current = guard
        .validate_current(&path, &original, MAX_FILE_BYTES)
        .unwrap();

    let external = write_synced_sibling(&path, b"external-version").unwrap();
    atomic_replace(&path, external).unwrap();
    let outcome = guard.commit_audited(
        &path,
        current,
        staged,
        &original,
        b"tool-version",
        MAX_FILE_BYTES,
    );
    let CommitOutcome::Ambiguous {
        tool_mutated_file,
        external_change_detected,
        recovery_backup: Some(recovery_backup),
        recovery_sha256: Some(recovery_sha256),
        ..
    } = outcome
    else {
        panic!("namespace replacement must produce an audited ambiguity");
    };
    assert!(tool_mutated_file);
    assert!(external_change_detected);
    assert_eq!(fs::read(&path).unwrap(), b"tool-version");
    assert_eq!(fs::read(&recovery_backup).unwrap(), b"external-version");
    assert_eq!(recovery_sha256, sha256_hex(b"external-version"));
}

#[test]
fn namespace_delete_after_validation_never_reports_verified_effect() {
    let fixture = Fixture::new(b"base");
    let path = fs::canonicalize(&fixture.path).unwrap();
    let mut guard = EditGuard::acquire(&path).unwrap();
    let original = guard.read_bounded(MAX_FILE_BYTES).unwrap();
    let staged = write_synced_sibling(&path, b"tool-version").unwrap();
    let current = guard
        .validate_current(&path, &original, MAX_FILE_BYTES)
        .unwrap();
    fs::remove_file(&path).unwrap();
    let outcome = guard.commit_audited(
        &path,
        current,
        staged,
        &original,
        b"tool-version",
        MAX_FILE_BYTES,
    );
    let CommitOutcome::Ambiguous {
        tool_mutated_file,
        external_change_detected,
        ..
    } = outcome
    else {
        panic!("namespace deletion must remain conservative");
    };
    assert!(!tool_mutated_file);
    assert!(external_change_detected);
}
