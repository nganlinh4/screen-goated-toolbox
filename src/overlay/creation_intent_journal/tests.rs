use super::*;
use serde_json::json;
use std::sync::atomic::Ordering;

fn test_path() -> PathBuf {
    std::env::temp_dir().join(format!(
        "sgt-creation-intent-{}-{}.json",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ))
}

fn intent(job: &str, created_at_ms: u64) -> Intent {
    let arguments = json!({"prompt": "A calm landscape", "sourceImagePaths": []});
    Intent {
        product: "image".to_string(),
        job_id: job.to_string(),
        dispatch_id: format!("dispatch-{job}"),
        created_at_ms,
        accepted_at_ms: created_at_ms,
        deadline_at_ms: created_at_ms
            + crate::overlay::creation_process_supervisor::MAX_WALL_TIME_MS,
        expires_at_ms: created_at_ms + INTENT_LIFETIME_MS,
        arguments_fingerprint: fingerprint(&arguments).unwrap(),
        arguments,
    }
}

#[test]
fn persisted_intent_keeps_same_delivery_identity_after_reload() {
    let path = test_path();
    record_at(&path, intent("image_1", 10)).unwrap();
    let store = read_store(&path).unwrap();
    assert_eq!(store.entries.len(), 1);
    assert_eq!(store.entries[0].job_id, "image_1");
    assert_eq!(store.entries[0].dispatch_id, "dispatch-image_1");
    clear_at(&path, "image", "image_1", 11).unwrap();
    assert!(read_store(&path).unwrap().entries.is_empty());
    let _ = std::fs::remove_file(path);
}

#[test]
fn expired_invalid_and_excess_entries_are_bounded() {
    let path = test_path();
    for index in 0..MAX_ENTRIES {
        record_at(&path, intent(&format!("image_{index}"), 100 + index as u64)).unwrap();
    }
    let before = read_store(&path).unwrap();
    assert_eq!(before.entries.len(), MAX_ENTRIES);
    assert!(record_at(&path, intent("overflow", 1_000)).is_err());
    let mut store = read_store(&path).unwrap();
    assert_eq!(store.entries.len(), MAX_ENTRIES);
    assert_eq!(
        store
            .entries
            .iter()
            .map(|entry| entry.job_id.as_str())
            .collect::<Vec<_>>(),
        before
            .entries
            .iter()
            .map(|entry| entry.job_id.as_str())
            .collect::<Vec<_>>()
    );
    store.entries.push(intent("expired", 1));
    prune(&mut store, INTENT_LIFETIME_MS + 1);
    assert!(!store.entries.iter().any(|entry| entry.job_id == "expired"));
    assert!(
        validate_intent(&Intent {
            dispatch_id: "../invalid".to_string(),
            ..intent("invalid", 10)
        })
        .is_err()
    );
    assert!(validate_persisted_path(Path::new("relative")).is_err());
    let _ = std::fs::remove_file(path);
}

#[test]
fn corrupt_intent_index_is_preserved_and_fails_closed() {
    let path = test_path();
    let corrupt = b"{ invalid-accepted-jobs";
    std::fs::write(&path, corrupt).unwrap();
    assert!(record_at(&path, intent("image_1", 10)).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), corrupt);
    let _ = std::fs::remove_file(path);
}

#[test]
fn recovery_identity_is_immutable_but_exact_replay_is_idempotent() {
    let path = test_path();
    let saved = intent("image_1", 10);
    record_at(&path, saved.clone()).unwrap();
    let mut replay = saved.clone();
    replay.created_at_ms = 11;
    replay.accepted_at_ms = 11;
    replay.deadline_at_ms = 11 + crate::overlay::creation_process_supervisor::MAX_WALL_TIME_MS;
    replay.expires_at_ms = 11 + INTENT_LIFETIME_MS;
    record_at(&path, replay).unwrap();
    assert_eq!(read_store(&path).unwrap().entries.len(), 1);

    let mut changed_dispatch = saved.clone();
    changed_dispatch.dispatch_id = "different-dispatch".to_string();
    assert!(record_at(&path, changed_dispatch).is_err());
    let mut changed_arguments = saved;
    changed_arguments.arguments = json!({"prompt": "A different request"});
    changed_arguments.arguments_fingerprint = fingerprint(&changed_arguments.arguments).unwrap();
    assert!(record_at(&path, changed_arguments).is_err());
    assert_eq!(read_store(&path).unwrap().entries.len(), 1);
    let _ = std::fs::remove_file(path);
}

#[test]
fn ambiguous_recovery_identities_fail_closed_without_rewriting() {
    let path = test_path();
    let first = intent("image_1", 10);
    let mut duplicate_dispatch = intent("image_2", 11);
    duplicate_dispatch.dispatch_id = first.dispatch_id.clone();
    let mut duplicate_job = intent("image_1", 12);
    duplicate_job.dispatch_id = "another-dispatch".to_string();
    for ambiguous in [duplicate_dispatch, duplicate_job] {
        write_store(
            &path,
            &Store {
                version: VERSION,
                entries: vec![first.clone(), ambiguous],
                dropped_invalid: false,
            },
        )
        .unwrap();
        let before = std::fs::read(&path).unwrap();
        assert!(read_store(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn one_invalid_entry_is_dropped_without_disabling_the_journal() {
    let path = test_path();
    let valid = intent("image_1", 10);
    let mut corrupt = intent("image_2", 11);
    corrupt.arguments_fingerprint = "tampered".to_string();
    write_store(
        &path,
        &Store {
            version: VERSION,
            entries: vec![valid.clone(), corrupt],
            dropped_invalid: false,
        },
    )
    .unwrap();
    let store = read_store(&path).unwrap();
    assert!(store.dropped_invalid);
    assert_eq!(store.entries.len(), 1);
    assert_eq!(store.entries[0].job_id, valid.job_id);
    record_at(&path, intent("image_3", 12)).unwrap();
    let repaired = read_store(&path).unwrap();
    assert!(!repaired.dropped_invalid);
    assert_eq!(repaired.entries.len(), 2);
    let _ = std::fs::remove_file(path);
}

#[test]
fn persisted_deadline_survives_a_changed_wall_time_limit() {
    let path = test_path();
    let mut saved = intent("image_1", 10);
    // Written by a release whose wall-time limit differed from today's.
    saved.deadline_at_ms = saved.accepted_at_ms + 3 * 60 * 60 * 1_000;
    write_store(
        &path,
        &Store {
            version: VERSION,
            entries: vec![saved.clone()],
            dropped_invalid: false,
        },
    )
    .unwrap();
    let store = read_store(&path).unwrap();
    assert!(!store.dropped_invalid);
    assert_eq!(store.entries[0].deadline_at_ms, saved.deadline_at_ms);
    let mut no_deadline = intent("image_2", 10);
    no_deadline.deadline_at_ms = no_deadline.accepted_at_ms;
    assert!(validate_intent(&no_deadline).is_err());
    let _ = std::fs::remove_file(path);
}

#[test]
fn expiry_keeps_intents_until_their_owner_shows_the_deadline_result() {
    let mut owned = intent("model_1", 10);
    owned.product = "3d".into();
    let mut vector = intent("vector_1", 10);
    vector.product = "svg".into();
    let mut store = Store {
        version: VERSION,
        entries: vec![owned, vector, intent("image_1", 10)],
        dropped_invalid: false,
    };
    let after_expiry = 10 + INTENT_LIFETIME_MS + 1;
    prune(&mut store, after_expiry);
    let kept = store
        .entries
        .iter()
        .map(|intent| intent.job_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(kept, ["model_1", "vector_1"]);
    // The owner resolves a kept intent through its deadline path.
    assert!(
        store
            .entries
            .iter()
            .all(|intent| intent.deadline_at_ms <= after_expiry)
    );

    let path = test_path();
    let mut owned = intent("model_2", 10);
    owned.product = "3d".into();
    record_at(&path, owned).unwrap();
    let mut late = intent("model_3", after_expiry);
    late.product = "3d".into();
    record_at(&path, late).unwrap();
    let saved = read_store(&path).unwrap();
    assert_eq!(saved.entries.len(), 2);
    clear_at(&path, "3d", "model_2", after_expiry).unwrap();
    assert_eq!(read_store(&path).unwrap().entries.len(), 1);
    let _ = std::fs::remove_file(path);
}
