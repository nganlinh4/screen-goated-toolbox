use super::persistence::{paired_result, paired_root};
use super::*;
use deletion::replay::{Role, quarantine_path};

/// History as an older version left it after renaming only the primary.
fn legacy_renamed(label: &str) -> (PathBuf, PathBuf, ResultHistoryEntry, PathBuf, PathBuf) {
    let root = paired_root(label);
    let (history, entry, primary, companion) = paired_result(&root);
    let renamed = root.join("renamed.glb");
    std::fs::rename(&primary, &renamed).unwrap();
    let mut store = load_store(&history).unwrap();
    store.entries[0].output_path = renamed.to_string_lossy().into_owned();
    store.entries[0].output_name = "renamed.glb".into();
    store.entries[0].companion_origin_path = None;
    save_store(&history, &store).unwrap();
    (root, history, entry, renamed, companion)
}

#[test]
fn legacy_renamed_entry_rebinds_companion_for_export_and_delete() {
    let (root, history, entry, renamed, companion) = legacy_renamed("legacy-rename-bind");
    let unbound = load_store(&history).unwrap().entries[0].clone();
    assert!(validated_companion_path(&unbound).is_err());
    let listed = list_at(&history, "3d", DEFAULT_RESULTS_PER_TOOL).unwrap();
    assert_eq!(
        validated_companion_path(&listed[0]).unwrap(),
        Some(companion.clone())
    );
    assert!(
        load_store(&history).unwrap().entries[0]
            .companion_origin_path
            .is_some()
    );
    delete_at(&history, "3d", &entry.id).unwrap();
    assert!(!renamed.exists());
    assert!(!companion.exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn ambiguous_or_newer_companions_are_left_unbound() {
    for case in ["newer", "claimed-stem", "claimed-companion"] {
        let (root, history, _, renamed, companion) = legacy_renamed("legacy-rename-ambiguous");
        let mut store = load_store(&history).unwrap();
        match case {
            // The companion cannot postdate the record that published it.
            "newer" => store.entries[0].created_at_ms = 1,
            "claimed-stem" => {
                let mut other = store.entries[0].clone();
                other.id = "other".into();
                other.output_path = root.join("original.glb").to_string_lossy().into_owned();
                other.metadata = Value::Null;
                store.entries.push(other);
            }
            _ => {
                let mut other = store.entries[0].clone();
                other.id = "other".into();
                other.output_path = root.join("other.glb").to_string_lossy().into_owned();
                store.entries.push(other);
            }
        }
        save_store(&history, &store).unwrap();
        assert!(!companion::bind_orphaned(&mut store), "{case}");
        assert!(store.entries[0].companion_origin_path.is_none(), "{case}");
        assert!(renamed.exists() && companion.exists(), "{case}");
        std::fs::remove_dir_all(root).unwrap();
    }
}

/// Retention journal as an older version wrote it: one independent cleanup
/// per pair member, with the primary already moved to its prune quarantine.
fn legacy_pair_cleanup(label: &str) -> (PathBuf, PathBuf, PathBuf, PathBuf, PathBuf) {
    let root = paired_root(label);
    let (history, _, primary, companion) = paired_result(&root);
    let mut store = load_store(&history).unwrap();
    let entry = store.entries.remove(0);
    let side = companion::from_entry(&entry).unwrap();
    let quarantine = root.join(".sgt-prune-legacy-primary.tmp");
    std::fs::rename(&primary, &quarantine).unwrap();
    store.pending_cleanup = vec![
        PendingCleanup {
            output_path: entry.output_path.clone(),
            artifact_size_bytes: entry.artifact_size_bytes,
            artifact_sha256: entry.artifact_sha256.clone(),
            artifact_file_identity: store.delivery_identities[0].artifact_file_identity.clone(),
            quarantine_path: quarantine.to_string_lossy().into_owned(),
            history_entry: Some(entry),
            resolution: CleanupResolution::Pending,
        },
        PendingCleanup {
            output_path: side.path,
            artifact_size_bytes: side.size_bytes,
            artifact_sha256: side.sha256,
            artifact_file_identity: side.file_identity,
            quarantine_path: String::new(),
            history_entry: None,
            resolution: CleanupResolution::Pending,
        },
    ];
    save_store(&history, &store).unwrap();
    (root, history, primary, companion, quarantine)
}

#[test]
fn legacy_pair_cleanup_migrates_to_one_pair_deletion() {
    let (root, history, primary, companion, quarantine) = legacy_pair_cleanup("legacy-pair-ok");
    maintain_at(&history).unwrap();
    let store = load_store(&history).unwrap();
    assert!(store.pending_cleanup.is_empty());
    assert!(store.pending_deletes.is_empty());
    assert!(!primary.exists() && !quarantine.exists() && !companion.exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn legacy_pair_with_a_changed_member_returns_to_history_whole() {
    let (root, history, primary, companion, quarantine) =
        legacy_pair_cleanup("legacy-pair-changed");
    std::fs::write(&companion, b"edited companion").unwrap();
    maintain_at(&history).unwrap();
    let store = load_store(&history).unwrap();
    assert!(store.pending_cleanup.is_empty());
    assert!(store.pending_deletes.is_empty());
    assert_eq!(store.entries.len(), 1);
    assert_eq!(std::fs::read(&primary).unwrap(), b"primary model");
    assert!(!quarantine.exists());
    assert_eq!(std::fs::read(&companion).unwrap(), b"edited companion");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn legacy_companion_cleanup_returns_to_the_live_entry_that_owns_it() {
    let root = paired_root("legacy-owned-companion");
    let (history, _, primary, companion) = paired_result(&root);
    let mut store = load_store(&history).unwrap();
    let side = companion::from_entry(&store.entries[0]).unwrap();
    let quarantine = root.join(".sgt-prune-legacy-side.tmp");
    std::fs::rename(&companion, &quarantine).unwrap();
    store.pending_cleanup.push(PendingCleanup {
        output_path: side.path,
        artifact_size_bytes: side.size_bytes,
        artifact_sha256: side.sha256,
        artifact_file_identity: side.file_identity,
        quarantine_path: quarantine.to_string_lossy().into_owned(),
        history_entry: None,
        resolution: CleanupResolution::Pending,
    });
    save_store(&history, &store).unwrap();
    maintain_at(&history).unwrap();
    let store = load_store(&history).unwrap();
    assert!(store.pending_cleanup.is_empty());
    assert_eq!(
        validated_companion_path(&store.entries[0]).unwrap(),
        Some(companion.clone())
    );
    assert!(primary.exists() && !quarantine.exists());
    std::fs::remove_dir_all(root).unwrap();
}

fn pending_pair_delete(label: &str) -> (PathBuf, PathBuf, ResultHistoryEntry, PathBuf, PathBuf) {
    let root = paired_root(label);
    let (history, entry, primary, companion) = paired_result(&root);
    let mut store = load_store(&history).unwrap();
    let identity = store.delivery_identities[0].artifact_file_identity.clone();
    let entry_record = store.entries.remove(0);
    store
        .pending_deletes
        .push(deletion::PendingDelete::new(entry_record, identity));
    save_store(&history, &store).unwrap();
    (root, history, entry, primary, companion)
}

#[test]
fn pair_delete_restores_history_when_a_visible_member_changed_mid_journal() {
    let (root, history, entry, primary, companion) = pending_pair_delete("pair-crash-changed");
    // Crash after the companion was hidden, then the primary was edited.
    let hidden = quarantine_path(&entry, &companion, Role::Companion).unwrap();
    std::fs::rename(&companion, &hidden).unwrap();
    std::fs::write(&primary, b"edited primary").unwrap();
    let mut store = load_store(&history).unwrap();
    assert!(deletion::reconcile_pending_deletes(&mut store));
    assert!(store.pending_deletes.is_empty());
    assert_eq!(store.entries.len(), 1);
    assert!(!store.delivery_identities.is_empty());
    assert_eq!(std::fs::read(&primary).unwrap(), b"edited primary");
    assert_eq!(std::fs::read(&companion).unwrap(), b"companion model");
    assert!(!hidden.exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn pair_delete_finishes_from_every_hidden_crash_state() {
    for state in ["companion-hidden", "both-hidden", "companion-deleted"] {
        let (root, history, entry, primary, companion) = pending_pair_delete("pair-crash-hidden");
        let hidden_primary = quarantine_path(&entry, &primary, Role::Primary).unwrap();
        let hidden_side = quarantine_path(&entry, &companion, Role::Companion).unwrap();
        std::fs::rename(&companion, &hidden_side).unwrap();
        if state != "companion-hidden" {
            std::fs::rename(&primary, &hidden_primary).unwrap();
        }
        if state == "companion-deleted" {
            std::fs::remove_file(&hidden_side).unwrap();
            // A new file under the visible name is not part of the journal.
            std::fs::write(&primary, b"new unrelated file").unwrap();
        }
        let mut store = load_store(&history).unwrap();
        assert!(deletion::reconcile_pending_deletes(&mut store), "{state}");
        assert!(store.pending_deletes.is_empty(), "{state}");
        assert!(store.entries.is_empty(), "{state}");
        assert!(!hidden_primary.exists() && !hidden_side.exists(), "{state}");
        assert!(!companion.exists(), "{state}");
        assert_eq!(primary.exists(), state == "companion-deleted", "{state}");
        std::fs::remove_dir_all(root).unwrap();
    }
}
