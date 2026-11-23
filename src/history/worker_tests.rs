use super::*;
use std::fs;
use std::sync::mpsc;
use std::time::Duration;

fn item(id: i64, name: &str) -> HistoryItem {
    HistoryItem {
        id,
        timestamp: String::new(),
        item_type: HistoryType::Text,
        text: name.into(),
        media_path: name.into(),
        media_proof: None,
    }
}

#[test]
fn a_prepared_save_cannot_reappear_after_clear_or_cancellation() {
    let cache = Mutex::new(Vec::new());
    let epoch = AtomicU64::new(1);
    let context = CommitContext {
        cache: &cache,
        epoch: &epoch,
        max_items: 10,
        media_dir: Path::new("unused"),
        failed: &AtomicBool::new(false),
    };
    assert!(transactions::insert(item(1, "before"), None, 0, &context).is_none());
    assert!(cache.lock().unwrap().is_empty());
    let token = crate::overlay::result::ChainCancelToken::new();
    token.cancel();
    assert!(transactions::insert(item(2, "cancelled"), Some(token), 1, &context).is_none());
    assert!(cache.lock().unwrap().is_empty());
    let change = transactions::insert(item(3, "after"), None, 1, &context).unwrap();
    assert!(cache.lock().unwrap().is_empty());
    assert!(transactions::publish_with(&cache, context.failed, change, |_| true, || {}).is_some());
    assert_eq!(cache.lock().unwrap()[0].id, 3);
}

#[test]
fn a_blocked_database_write_does_not_hold_the_cache() {
    let cache = Arc::new(Mutex::new(vec![item(1, "original")]));
    let (entered, ready) = mpsc::channel();
    let (release, blocked) = mpsc::channel();
    let worker_cache = cache.clone();
    let worker = std::thread::spawn(move || {
        let change = transactions::clear(&worker_cache);
        transactions::publish_with(
            &worker_cache,
            &AtomicBool::new(false),
            change,
            |_| {
                entered.send(()).unwrap();
                blocked.recv_timeout(Duration::from_secs(2)).unwrap();
                true
            },
            || {},
        )
        .is_some()
    });
    ready.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(
        cache.try_lock().is_ok(),
        "disk persistence held the UI cache lock"
    );
    release.send(()).unwrap();
    assert!(worker.join().unwrap());
}

#[test]
fn uncommitted_media_removes_only_its_owned_file() {
    let id = MEDIA_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let directory =
        std::env::temp_dir().join(format!("sgt-history-test-{}-{id}", std::process::id()));
    fs::create_dir(&directory).unwrap();
    let sentinel = directory.join("unrelated.txt");
    fs::write(&sentinel, b"keep").unwrap();
    let path = {
        let mut media = PendingMedia::create(&directory, "img", "png").unwrap();
        media.file.write_all(b"partial").unwrap();
        directory.join(&media.filename)
    };
    assert!(!path.exists());
    assert_eq!(fs::read(&sentinel).unwrap(), b"keep");
    fs::remove_file(sentinel).unwrap();
    fs::remove_dir(directory).unwrap();
}

#[test]
fn clear_during_media_preparation_never_resurrects_the_older_save() {
    let id = MEDIA_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let directory = std::env::temp_dir().join(format!(
        "sgt-history-clear-test-{}-{id}",
        std::process::id()
    ));
    fs::create_dir(&directory).unwrap();
    let cache = Arc::new(Mutex::new(Vec::new()));
    let epoch = Arc::new(AtomicU64::new(0));
    let (tx, _rx) = mpsc::channel();
    let manager = super::super::HistoryManager {
        tx,
        items: cache.clone(),
        persistence_failed: Arc::new(AtomicBool::new(false)),
        load_failed: false,
        submission: Mutex::new(()),
        clear_epoch: epoch.clone(),
    };
    let (prepared, ready) = mpsc::channel();
    let (release, blocked) = mpsc::channel();
    let worker_cache = cache.clone();
    let worker_epoch = epoch.clone();
    let worker_dir = directory.clone();
    let worker = std::thread::spawn(move || {
        let media = write_bytes(&worker_dir, "text", "txt", b"pending").unwrap();
        prepared.send(worker_dir.join(&media.filename)).unwrap();
        blocked.recv_timeout(Duration::from_secs(2)).unwrap();
        let context = CommitContext {
            cache: &worker_cache,
            epoch: &worker_epoch,
            max_items: 10,
            media_dir: &worker_dir,
            failed: &AtomicBool::new(false),
        };
        let committed = transactions::insert(item(1, &media.filename), None, 0, &context).is_some();
        drop(media);
        committed
    });
    let path = ready.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(cache.try_lock().is_ok());
    manager.clear_all();
    release.send(()).unwrap();
    assert!(!worker.join().unwrap());
    assert!(cache.lock().unwrap().is_empty());
    assert!(!path.exists());
    fs::remove_dir(directory).unwrap();
}

#[test]
fn only_unchanged_proven_media_is_removed() {
    let id = MEDIA_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let directory = std::env::temp_dir().join(format!(
        "sgt-history-proof-test-{}-{id}",
        std::process::id()
    ));
    fs::create_dir(&directory).unwrap();
    let mut owned = write_bytes(&directory, "text", "txt", b"original").unwrap();
    let path = directory.join(&owned.filename);
    let proof = owned.proof.clone().expect("file identity");
    owned.committed = true;
    drop(owned);
    fs::write(&path, b"modified").unwrap();
    assert!(remove_verified_file(&path, &proof).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"modified");
    fs::remove_file(&path).unwrap();
    fs::write(&path, b"original").unwrap();
    assert!(remove_verified_file(&path, &proof).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"original");
    let cache = Mutex::new(Vec::new());
    remove_unreferenced(
        &directory,
        &cache,
        &[MediaRef {
            filename: path.file_name().unwrap().to_string_lossy().into_owned(),
            proof: None,
        }],
    );
    assert!(path.exists(), "legacy or unproven media must be retained");
    fs::remove_file(&path).unwrap();
    let mut fresh = write_bytes(&directory, "text", "txt", b"owned").unwrap();
    let fresh_path = directory.join(&fresh.filename);
    let fresh_proof = fresh.proof.clone().unwrap();
    fresh.committed = true;
    drop(fresh);
    remove_verified_file(&fresh_path, &fresh_proof).unwrap();
    assert!(!fresh_path.exists());
    fs::remove_dir(directory).unwrap();
}
