use super::*;

#[test]
fn a_collection_has_one_download_owner_at_a_time() {
    let id = "test-only-download-gate";
    let first = claim_download(id).expect("idle collection is claimable");
    assert!(is_downloading(id));
    assert!(claim_download(id).is_none(), "a second worker must wait");
    drop(first);
    assert!(!is_downloading(id));
    assert!(claim_download(id).is_some());
}

#[test]
fn deleting_all_packs_also_drops_their_rendered_previews() {
    let root = std::env::temp_dir().join(format!("sgt-pointer-clean-{}", std::process::id()));
    let spec = catalog_collections()[0];
    fs::create_dir_all(spec.local_dir(&root)).unwrap();
    fs::create_dir_all(root.join(PREVIEW_CACHE_DIR).join("v1")).unwrap();
    assert_eq!(delete_all_collections_in(&root).unwrap(), 1);
    assert!(!spec.local_dir(&root).exists());
    assert!(!root.join(PREVIEW_CACHE_DIR).exists());
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn clean_all_also_removes_unverified_legacy_packs() {
    let root = std::env::temp_dir().join(format!("sgt-pointer-legacy-{}", std::process::id()));
    let spec = catalog_collections()[0];
    fs::create_dir_all(spec.legacy_dir(&root)).unwrap();
    fs::write(spec.legacy_dir(&root).join("pointer.cur"), b"unverified").unwrap();
    assert_eq!(delete_all_collections_in(&root).unwrap(), 1);
    assert!(!spec.legacy_dir(&root).exists());
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_restarted_worker_never_shares_its_predecessors_flag() {
    let mut current = new_stop_signal();
    let previous = current.clone();
    let next = replace_stop_signal(&mut current);
    assert!(
        previous.load(Ordering::Relaxed),
        "old worker is told to stop"
    );
    assert!(!next.load(Ordering::Relaxed));
    assert!(Arc::ptr_eq(&next, &current));
    assert!(!Arc::ptr_eq(&next, &previous));
}
