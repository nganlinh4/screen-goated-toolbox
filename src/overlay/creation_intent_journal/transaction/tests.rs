use super::*;

fn setup(old: bool) -> (PathBuf, PathBuf, PathBuf, Transaction) {
    let root = std::env::temp_dir().join(format!(
        "sgt-intent-transaction-{}-{}",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    let path = root.join("journal.json");
    if old {
        std::fs::write(&path, b"previous accepted state").unwrap();
    }
    let temporary = root.join(".journal.json.42-7.tmp");
    std::fs::write(&temporary, b"new state").unwrap();
    let backup = root.join(".journal.json.42-7.tmp.backup");
    let transaction = Transaction {
        version: 1,
        temporary: temporary.file_name().unwrap().to_str().unwrap().to_string(),
        backup: backup.file_name().unwrap().to_str().unwrap().to_string(),
        old: stamp(&path).unwrap(),
        new: stamp(&temporary).unwrap().unwrap(),
    };
    std::fs::write(
        marker_path(&path).unwrap(),
        serde_json::to_vec(&transaction).unwrap(),
    )
    .unwrap();
    (path, temporary, backup, transaction)
}

fn cleanup(path: &Path) {
    if path.exists() {
        std::fs::remove_file(path).unwrap();
    }
    std::fs::remove_dir(path.parent().unwrap()).unwrap();
}

#[test]
fn recovery_preserves_the_previous_state_before_replacement() {
    let (path, temporary, _, _) = setup(true);
    reconcile(&path).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"previous accepted state");
    assert!(!temporary.exists());
    cleanup(&path);
}

#[test]
fn recovery_restores_named_backup_after_original_moved() {
    let (path, temporary, backup, _) = setup(true);
    std::fs::rename(&path, &backup).unwrap();
    reconcile(&path).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"previous accepted state");
    assert!(!temporary.exists() && !backup.exists());
    cleanup(&path);
}

#[test]
fn recovery_keeps_committed_new_state_after_replacement() {
    let (path, temporary, backup, _) = setup(true);
    std::fs::rename(&path, &backup).unwrap();
    std::fs::rename(&temporary, &path).unwrap();
    reconcile(&path).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"new state");
    assert!(!backup.exists());
    cleanup(&path);
}

#[cfg(windows)]
#[test]
fn current_journal_stays_locked_until_backup_cleanup_finishes() {
    let (path, temporary, backup, _) = setup(true);
    std::fs::rename(&path, &backup).unwrap();
    std::fs::rename(&temporary, &path).unwrap();
    reconcile_using(&path, || {
        assert!(std::fs::remove_file(&path).is_err());
        assert!(backup.exists());
    })
    .unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"new state");
    assert!(!backup.exists());
    cleanup(&path);
}

#[test]
fn changed_backup_or_partial_marker_is_preserved() {
    let (path, temporary, backup, _) = setup(true);
    std::fs::rename(&path, &backup).unwrap();
    std::fs::write(&backup, b"changed backup").unwrap();
    assert!(reconcile(&path).is_err());
    assert_eq!(std::fs::read(&backup).unwrap(), b"changed backup");
    assert_eq!(std::fs::read(&temporary).unwrap(), b"new state");
    std::fs::write(marker_path(&path).unwrap(), b"{").unwrap();
    assert!(reconcile(&path).is_err());
    std::fs::remove_file(temporary).unwrap();
    std::fs::remove_file(backup).unwrap();
    std::fs::remove_file(marker_path(&path).unwrap()).unwrap();
    cleanup(&path);
}

#[test]
fn absent_initial_state_remains_fail_closed_if_commit_is_uncertain() {
    let (path, temporary, _, _) = setup(false);
    assert!(reconcile(&path).is_err());
    assert_eq!(std::fs::read(&temporary).unwrap(), b"new state");
    std::fs::remove_file(temporary).unwrap();
    std::fs::remove_file(marker_path(&path).unwrap()).unwrap();
    cleanup(&path);
}

#[test]
fn late_initial_target_is_not_overwritten() {
    let root = std::env::temp_dir().join(format!(
        "sgt-intent-late-{}-{}",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    let path = root.join("journal.json");
    let result = write_using(
        &path,
        b"new state",
        |path, temporary, backup, publication| {
            assert!(publication.old.is_none());
            std::fs::write(path, b"intervening state")?;
            publish_bound(path, temporary, backup, publication)
        },
    );
    assert!(result.is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"intervening state");
    assert!(reconcile(&path).is_err());
    for item in std::fs::read_dir(&root).unwrap() {
        std::fs::remove_file(item.unwrap().path()).unwrap();
    }
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn failed_replacement_restores_named_preimage_on_next_access() {
    for code in [1176, 1177] {
        let root = std::env::temp_dir().join(format!(
            "sgt-intent-failure-{}-{}",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("journal.json");
        std::fs::write(&path, b"accepted state").unwrap();
        let failure = write_using(&path, b"new state", |path, _, backup, publication| {
            assert!(publication.old.is_some());
            if code == 1177 {
                std::fs::rename(path, backup)?;
            }
            Err(std::io::Error::from_raw_os_error(code))
        })
        .unwrap_err();
        assert!(failure.is_uncertain());
        reconcile(&path).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"accepted state");
        cleanup(&path);
    }
}

#[test]
fn successful_publish_with_changed_result_is_uncertain() {
    let root = std::env::temp_dir().join(format!(
        "sgt-intent-uncertain-{}-{}",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    let path = root.join("journal.json");
    std::fs::write(&path, b"old state").unwrap();
    let error = write_using(
        &path,
        b"new state",
        |path, temporary, backup, publication| {
            publish_bound(path, temporary, backup, publication)?;
            std::fs::write(path, b"externally changed")?;
            Ok(())
        },
    )
    .unwrap_err();
    assert!(error.is_uncertain());
    assert!(error.to_string().contains("do not retry"));
    assert_eq!(std::fs::read(&path).unwrap(), b"externally changed");
    assert!(reconcile(&path).is_err());
    for item in std::fs::read_dir(&root).unwrap() {
        std::fs::remove_file(item.unwrap().path()).unwrap();
    }
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn intervening_destination_is_never_displaced_by_publication() {
    let root = std::env::temp_dir().join(format!(
        "sgt-intent-intervening-{}-{}",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    let path = root.join("journal.json");
    let displaced = root.join("displaced");
    std::fs::write(&path, b"accepted state").unwrap();
    let mut attempts = 0;
    let error = write_using(
        &path,
        b"new state",
        |path, temporary, backup, publication| {
            attempts += 1;
            if attempts == 1 {
                std::fs::rename(path, &displaced)?;
                std::fs::write(path, b"intervening state")?;
            }
            publish_bound(path, temporary, backup, publication)
        },
    )
    .unwrap_err();
    // Nothing of ours moved, so the failure is definite, and the same-user
    // file at the journal name stays exactly where it was.
    assert!(!error.is_uncertain());
    assert_eq!(std::fs::read(&path).unwrap(), b"intervening state");
    assert_eq!(std::fs::read(&displaced).unwrap(), b"accepted state");
    let names = std::fs::read_dir(&root)
        .unwrap()
        .map(|item| item.unwrap().file_name().into_string().unwrap())
        .collect::<Vec<_>>();
    assert!(
        !names.iter().any(|name| name.ends_with(".backup")),
        "{names:?}"
    );
    let staged = names.iter().find(|name| name.ends_with(".tmp")).unwrap();
    assert_eq!(std::fs::read(root.join(staged)).unwrap(), b"new state");
    assert!(reconcile(&path).is_err());
    for item in std::fs::read_dir(&root).unwrap() {
        std::fs::remove_file(item.unwrap().path()).unwrap();
    }
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn identity_bound_publication_replaces_the_stamped_preimage() {
    let root = std::env::temp_dir().join(format!(
        "sgt-intent-bound-{}-{}",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    let path = root.join("journal.json");
    write(&path, b"first state").unwrap();
    write(&path, b"second state").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"second state");
    let names = std::fs::read_dir(&root)
        .unwrap()
        .map(|item| item.unwrap().file_name().into_string().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["journal.json".to_string()]);
    cleanup(&path);
}

#[cfg(windows)]
#[test]
fn successful_publication_is_acknowledged_if_cleanup_is_temporarily_blocked() {
    use std::os::windows::fs::OpenOptionsExt as _;
    use windows::Win32::Storage::FileSystem::FILE_SHARE_READ;
    let root = std::env::temp_dir().join(format!(
        "sgt-intent-cleanup-{}-{}",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    let path = root.join("journal.json");
    std::fs::write(&path, b"old state").unwrap();
    let mut blocker = None;
    write_using(
        &path,
        b"new state",
        |path, temporary, backup, publication| {
            publish_bound(path, temporary, backup, publication)?;
            blocker = Some(
                std::fs::OpenOptions::new()
                    .read(true)
                    .share_mode(FILE_SHARE_READ.0)
                    .open(backup)?,
            );
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"new state");
    assert!(marker_path(&path).unwrap().exists());
    drop(blocker);
    reconcile(&path).unwrap();
    cleanup(&path);
}

#[cfg(windows)]
#[test]
fn shared_marker_observer_does_not_block_the_next_transaction() {
    use std::os::windows::fs::OpenOptionsExt as _;
    use windows::Win32::Storage::FileSystem::{
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };
    let root = std::env::temp_dir().join(format!(
        "sgt-intent-observer-{}-{}",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    let path = root.join("journal.json");
    let marker = marker_path(&path).unwrap();
    let mut observer = None;
    write_using(
        &path,
        b"first state",
        |path, temporary, backup, publication| {
            publish_bound(path, temporary, backup, publication)?;
            observer = Some(
                std::fs::OpenOptions::new()
                    .read(true)
                    .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0 | FILE_SHARE_DELETE.0)
                    .open(&marker)?,
            );
            Ok(())
        },
    )
    .unwrap();
    let namespace = std::fs::symlink_metadata(&marker);
    let reconciled = reconcile(&path);
    let next = write(&path, b"second state");
    let current = std::fs::read(&path).unwrap();
    let mut old_marker = Vec::new();
    observer
        .as_mut()
        .unwrap()
        .read_to_end(&mut old_marker)
        .unwrap();
    let saved: Transaction = serde_json::from_slice(&old_marker).unwrap();
    assert_eq!(
        saved.new.sha256,
        format!("{:x}", Sha256::digest(b"first state"))
    );
    drop(observer);
    reconcile(&path).unwrap();
    cleanup(&path);

    assert!(
        namespace
            .as_ref()
            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
        "marker namespace: {namespace:?}; reconciliation: {reconciled:?}; next write: {next:?}"
    );
    reconciled.unwrap();
    next.unwrap();
    assert_eq!(current, b"second state");
}
