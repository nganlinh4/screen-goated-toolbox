use super::*;

fn test_claim(queue: &Path) -> io::Result<Option<ClaimedRequest>> {
    claim(queue, drop)
}

#[test]
fn replacement_during_claimed_request_read_survives_cleanup() {
    let root = std::env::temp_dir().join(format!(
        "sgt-lab-queue-{}-{}",
        std::process::id(),
        NEXT_CLAIM.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("request.json"), b"first").unwrap();
    let first = test_claim(&root).unwrap().unwrap();
    std::fs::write(root.join("request.json.tmp"), b"second").unwrap();
    std::fs::rename(root.join("request.json.tmp"), root.join("request.json")).unwrap();
    assert_eq!(first.read().unwrap(), b"first");
    drop(first);
    let second = test_claim(&root).unwrap().unwrap();
    assert_eq!(second.read().unwrap(), b"second");
    drop(second);
    assert!(test_claim(&root).unwrap().is_none());
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn claim_moves_opened_request_when_public_path_is_replaced() {
    let root = std::env::temp_dir().join(format!(
        "sgt-lab-queue-source-swap-{}-{}",
        std::process::id(),
        NEXT_CLAIM.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    let public = root.join("request.json");
    std::fs::write(&public, b"first").unwrap();
    let claimed = claim_using(
        &root,
        |opened, source, destination| {
            std::fs::rename(source, root.join("original-moved"))?;
            std::fs::write(source, b"second")?;
            rename_handle(opened, destination)
        },
        drop,
    )
    .unwrap()
    .unwrap();
    assert_eq!(claimed.read().unwrap(), b"first");
    assert_eq!(std::fs::read(&public).unwrap(), b"second");
    drop(claimed);
    std::fs::remove_file(public).unwrap();
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn oversized_claim_is_bounded_and_removed() {
    let root = std::env::temp_dir().join(format!(
        "sgt-lab-queue-limit-{}-{}",
        std::process::id(),
        NEXT_CLAIM.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    std::fs::write(
        root.join("request.json"),
        vec![b'x'; MAX_REQUEST_BYTES as usize + 1],
    )
    .unwrap();
    let request = test_claim(&root).unwrap().unwrap();
    assert_eq!(
        request.read().unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    drop(request);
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn temporary_read_failure_keeps_claim_for_retry() {
    let root = std::env::temp_dir().join(format!(
        "sgt-lab-queue-retry-{}-{}",
        std::process::id(),
        NEXT_CLAIM.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("request.json"), b"complete").unwrap();
    let request = test_claim(&root).unwrap().unwrap();
    let locked = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&request.path)
        .unwrap();
    assert!(request.read().is_err());
    drop(locked);
    assert_eq!(request.read().unwrap(), b"complete");
    drop(request);
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn unreadable_request_does_not_block_prior_work_or_newer_valid_request() {
    let root = std::env::temp_dir().join(format!(
        "sgt-lab-queue-stalled-{}-{}",
        std::process::id(),
        NEXT_CLAIM.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("request.json"), br#"{"id":"old"}"#).unwrap();
    let old = test_claim(&root).unwrap().unwrap();
    let locked = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&old.path)
        .unwrap();
    let mut pending = PendingRequests::default();
    pending.unread.push_back(old);
    let parse = |bytes: &[u8]| serde_json::from_slice(bytes).ok();
    pending.poll(&root, parse);
    assert_eq!(pending.unread.len(), 1);
    drop(validation_permit());
    std::fs::write(root.join("request.json"), b"{").unwrap();
    pending.poll(&root, parse);
    assert_eq!(pending.unread.len(), 1);
    assert!(!pending.is_ready());
    std::fs::write(root.join("request.json"), br#"{"id":"new"}"#).unwrap();
    pending.poll(&root, parse);
    assert_eq!(pending.take_ready().unwrap()["id"], "new");
    assert!(pending.unread.is_empty());
    assert_eq!(pending.retired.len(), 1);
    drop(locked);
    pending.poll(&root, parse);
    assert!(pending.retired.is_empty());
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn malformed_claim_cleanup_is_retried_after_lock_is_released() {
    let root = std::env::temp_dir().join(format!(
        "sgt-lab-queue-malformed-{}-{}",
        std::process::id(),
        NEXT_CLAIM.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("request.json"), b"{").unwrap();
    let request = test_claim(&root).unwrap().unwrap();
    let path = request.path.clone();
    let locked = std::cell::RefCell::new(None);
    let mut pending = PendingRequests::default();
    pending.unread.push_back(request);
    pending.poll(&root, |_| {
        *locked.borrow_mut() = Some(
            std::fs::OpenOptions::new()
                .read(true)
                .share_mode(0)
                .open(&path)
                .unwrap(),
        );
        None
    });
    assert!(!pending.is_ready());
    assert_eq!(pending.retired.len(), 1);
    assert!(path.exists());
    drop(locked.into_inner());
    pending.poll(&root, |_| None);
    assert!(pending.retired.is_empty());
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn dispatched_claim_cleanup_is_retried_after_lock_is_released() {
    let root = std::env::temp_dir().join(format!(
        "sgt-lab-queue-dispatched-{}-{}",
        std::process::id(),
        NEXT_CLAIM.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("request.json"), br#"{"id":"ready"}"#).unwrap();
    let mut pending = PendingRequests::default();
    pending.poll(&root, |bytes| serde_json::from_slice(bytes).ok());
    let path = pending.ready.as_ref().unwrap().0.path.clone();
    let locked = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&path)
        .unwrap();
    assert_eq!(pending.take_ready().unwrap()["id"], "ready");
    assert_eq!(pending.retired.len(), 1);
    assert!(path.exists());
    drop(locked);
    pending.poll(&root, |_| None);
    assert!(pending.retired.is_empty());
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn failed_claim_rename_retries_cleanup_without_removing_request() {
    let root = std::env::temp_dir().join(format!(
        "sgt-lab-queue-rename-{}-{}",
        std::process::id(),
        NEXT_CLAIM.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    let source = root.join("request.json");
    std::fs::write(&source, b"preserved").unwrap();
    let mut pending = PendingRequests::default();
    let mut unrelated = None;
    let result = claim_using(
        &root,
        |_, _, destination| {
            std::fs::write(destination, b"unrelated").unwrap();
            unrelated = Some(destination.to_owned());
            Err(io::Error::new(io::ErrorKind::PermissionDenied, "injected"))
        },
        |failed| pending.retire(failed),
    );
    assert_eq!(
        result.err().unwrap().kind(),
        io::ErrorKind::PermissionDenied
    );
    assert_eq!(pending.retired.len(), 1);
    assert_eq!(std::fs::read(&source).unwrap(), b"preserved");
    std::fs::remove_file(source).unwrap();
    pending.poll(&root, |_| None);
    assert_eq!(pending.retired.len(), 1);
    let unrelated = unrelated.unwrap();
    assert_eq!(std::fs::read(&unrelated).unwrap(), b"unrelated");
    std::fs::remove_file(unrelated).unwrap();
    pending.poll(&root, |_| None);
    assert!(pending.retired.is_empty());
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn completed_cleanup_disarms_drop_before_path_is_recreated() {
    let root = std::env::temp_dir().join(format!(
        "sgt-lab-queue-disarmed-{}-{}",
        std::process::id(),
        NEXT_CLAIM.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("request.json"), b"original").unwrap();
    let mut request = test_claim(&root).unwrap().unwrap();
    let directory = request.directory.clone();
    let path = request.path.clone();
    request.try_cleanup().unwrap();
    std::fs::create_dir(&directory).unwrap();
    std::fs::write(&path, b"replacement").unwrap();
    drop(request);
    assert_eq!(std::fs::read(&path).unwrap(), b"replacement");
    std::fs::remove_file(path).unwrap();
    std::fs::remove_dir(directory).unwrap();
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn partial_cleanup_never_removes_recreated_file_on_retry() {
    let root = std::env::temp_dir().join(format!(
        "sgt-lab-queue-partial-{}-{}",
        std::process::id(),
        NEXT_CLAIM.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("request.json"), b"original").unwrap();
    let mut request = test_claim(&root).unwrap().unwrap();
    let unrelated = request.directory.join("unrelated");
    std::fs::write(&unrelated, b"keep").unwrap();
    assert!(request.try_cleanup().is_err());
    std::fs::write(&request.path, b"replacement").unwrap();
    assert!(request.try_cleanup().is_err());
    assert_eq!(std::fs::read(&request.path).unwrap(), b"replacement");
    std::fs::remove_file(&request.path).unwrap();
    std::fs::remove_file(unrelated).unwrap();
    request.try_cleanup().unwrap();
    drop(request);
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn replacement_before_cleanup_is_not_deleted() {
    let root = std::env::temp_dir().join(format!(
        "sgt-lab-queue-replaced-{}-{}",
        std::process::id(),
        NEXT_CLAIM.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("request.json"), b"original").unwrap();
    let mut request = test_claim(&root).unwrap().unwrap();
    assert!(std::fs::rename(&request.directory, root.join("replacement-dir")).is_err());
    let moved = request.directory.join("moved");
    std::fs::rename(&request.path, &moved).unwrap();
    std::fs::write(&request.path, b"replacement").unwrap();
    assert!(request.try_cleanup().is_err());
    assert_eq!(std::fs::read(&request.path).unwrap(), b"replacement");
    std::fs::remove_file(&request.path).unwrap();
    std::fs::remove_file(moved).unwrap();
    request.try_cleanup().unwrap();
    drop(request);
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn deleted_original_identity_remains_pinned_until_cleanup() {
    let root = std::env::temp_dir().join(format!(
        "sgt-lab-queue-pinned-{}-{}",
        std::process::id(),
        NEXT_CLAIM.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("request.json"), b"original").unwrap();
    let mut request = test_claim(&root).unwrap().unwrap();
    let moved = request.directory.join("moved");
    std::fs::rename(&request.path, &moved).unwrap();
    std::fs::remove_file(moved).unwrap();
    std::fs::write(&request.path, b"replacement").unwrap();
    assert_ne!(
        file_identity(&std::fs::File::open(&request.path).unwrap()).unwrap(),
        request.file_identity.clone().unwrap()
    );
    assert!(request.try_cleanup().is_err());
    assert_eq!(std::fs::read(&request.path).unwrap(), b"replacement");
    std::fs::remove_file(&request.path).unwrap();
    request.try_cleanup().unwrap();
    drop(request);
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn changed_claim_target_is_retained_when_identity_cannot_be_proven() {
    let root = std::env::temp_dir().join(format!(
        "sgt-lab-queue-changed-{}-{}",
        std::process::id(),
        NEXT_CLAIM.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("request.json"), b"original").unwrap();
    let mut pending = PendingRequests::default();
    let mut preserved = None;
    let result = claim_using(
        &root,
        |source, _, destination| {
            rename_handle(source, destination)?;
            let moved = destination.with_file_name("original");
            std::fs::rename(destination, &moved)?;
            std::fs::write(destination, b"replacement")?;
            preserved = Some((destination.to_owned(), moved));
            Ok(())
        },
        |failed| pending.retire(failed),
    );
    assert!(result.is_err());
    assert_eq!(pending.retired.len(), 1);
    let (replacement, original) = preserved.unwrap();
    assert_eq!(std::fs::read(&replacement).unwrap(), b"replacement");
    assert_eq!(std::fs::read(&original).unwrap(), b"original");
    std::fs::remove_file(replacement).unwrap();
    std::fs::remove_file(original).unwrap();
    pending.poll(&root, |_| None);
    assert!(pending.retired.is_empty());
    std::fs::remove_dir(root).unwrap();
}
