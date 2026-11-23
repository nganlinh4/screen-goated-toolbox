use super::*;
use crate::history::HistoryManager;
use serde_json::Value;
use std::fs;
use std::sync::mpsc;
use std::time::Duration;

fn manager(
    cache: Arc<Mutex<Vec<HistoryItem>>>,
    epoch: Arc<AtomicU64>,
) -> (HistoryManager, Receiver<HistoryAction>) {
    let (tx, rx) = mpsc::channel();
    (
        HistoryManager {
            tx,
            items: cache,
            persistence_failed: Arc::new(AtomicBool::new(false)),
            load_failed: false,
            submission: Mutex::new(()),
            clear_epoch: epoch,
        },
        rx,
    )
}

fn root() -> std::path::PathBuf {
    let sequence = MEDIA_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "sgt-history-commit-{}-{sequence}",
        std::process::id()
    ));
    fs::create_dir(&root).unwrap();
    root
}

fn seed(root: &Path, count: usize) -> Vec<HistoryItem> {
    (0..count)
        .map(|index| {
            let mut media = write_bytes(root, "text", "txt", b"old-source").unwrap();
            let item = HistoryItem {
                id: index as i64,
                timestamp: String::new(),
                item_type: HistoryType::Text,
                text: format!("old-{index}"),
                media_path: media.filename.clone(),
                media_proof: media.proof.clone(),
            };
            media.committed = true;
            drop(media);
            item
        })
        .collect()
}

fn cleanup(root: &Path) {
    for entry in fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        let kind = entry.file_type().unwrap();
        assert!(kind.is_file() && !kind.is_symlink());
        let path = entry.path();
        assert_eq!(path.parent(), Some(root));
        let data = fs::read(&path).unwrap();
        assert!(data == b"old-source" || data == b"new-source");
        fs::remove_file(path).unwrap();
    }
    fs::remove_dir(root).unwrap();
}

#[test]
fn shared_commit_boundaries_preserve_committed_state_and_media_on_failure() {
    let fixture: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/parity-fixtures/history-ui/commit-boundaries.json"
    )))
    .unwrap();
    let mut failures = Vec::new();
    for case in fixture["cases"].as_array().unwrap() {
        let root = root();
        let cache = Arc::new(Mutex::new(seed(
            &root,
            case["initial"].as_u64().unwrap() as usize,
        )));
        if case["duplicate_ids"].as_bool().unwrap_or(false) {
            for item in cache.lock().unwrap().iter_mut() {
                item.id = 0;
            }
        }
        let epoch = Arc::new(AtomicU64::new(0));
        let (manager, rx) = manager(cache.clone(), epoch.clone());
        let mut durable_snapshot = cache.lock().unwrap().clone();
        for action in case["actions"].as_array().unwrap() {
            match action.as_str().unwrap() {
                "save" => manager.save_text_cancellable(
                    "new".into(),
                    "new-source".into(),
                    crate::overlay::result::ChainCancelToken::new(),
                ),
                "delete" => manager.delete(0),
                "clear" => manager.clear_all(),
                "prune" => manager.request_prune(fixture["limit"].as_u64().unwrap() as usize),
                _ => panic!("unknown fixture action"),
            }
        }
        let failed = manager.persistence_failed.clone();
        drop(manager);
        let mut writes = 0;
        let mut committed = case["initial"].as_u64().unwrap() as usize;
        process_queue_with(
            rx,
            cache.clone(),
            case["initial_limit"]
                .as_u64()
                .unwrap_or(fixture["limit"].as_u64().unwrap()) as usize,
            epoch,
            &root,
            failed.clone(),
            |snapshot| {
                let success = case.get("write_results").map_or_else(
                    || case["write_success"].as_bool().unwrap(),
                    |values| values[writes].as_bool().unwrap(),
                );
                writes += 1;
                if success {
                    committed = snapshot.len();
                    durable_snapshot = snapshot.to_vec();
                }
                success
            },
        );
        let visible_snapshot = cache.lock().unwrap().clone();
        let count = visible_snapshot.len();
        let order: Vec<_> = visible_snapshot
            .iter()
            .map(|item| item.text.as_str())
            .collect();
        let expected_order: Vec<_> = case["expected_order"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect();
        let backing_matches = visible_snapshot.iter().all(|item| {
            let expected = if item.text == "new" {
                b"new-source".as_slice()
            } else {
                b"old-source".as_slice()
            };
            fs::read(root.join(&item.media_path)).unwrap() == expected
        });
        let media = fs::read_dir(&root).unwrap().count();
        if count != case["expected_count"].as_u64().unwrap() as usize
            || media != case["expected_media"].as_u64().unwrap() as usize
            || writes != case["expected_writes"].as_u64().unwrap() as usize
            || count != committed
            || visible_snapshot != durable_snapshot
            || order != expected_order
            || !backing_matches
            || failed.load(Ordering::Acquire) != case["expected_error"].as_bool().unwrap()
        {
            failures.push(format!(
                "{}:visible={count},durable={committed},media={media},writes={writes}",
                case["name"].as_str().unwrap()
            ));
        }
        cleanup(&root);
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn blocked_failed_clear_keeps_cache_readable_and_committed_items_visible() {
    let root = root();
    let cache = Arc::new(Mutex::new(seed(&root, 1)));
    let epoch = Arc::new(AtomicU64::new(0));
    let (manager, rx) = manager(cache.clone(), epoch.clone());
    manager.clear_all();
    let failed = manager.persistence_failed.clone();
    drop(manager);
    let (entered, ready) = mpsc::channel();
    let (release, blocked) = mpsc::channel();
    let worker_cache = cache.clone();
    let worker_root = root.clone();
    let worker = std::thread::spawn(move || {
        process_queue_with(rx, worker_cache, 10, epoch, &worker_root, failed, |_| {
            entered.send(()).unwrap();
            blocked.recv_timeout(Duration::from_secs(3)).unwrap();
            false
        })
    });
    ready.recv_timeout(Duration::from_secs(3)).unwrap();
    let visible = cache.try_lock().map(|items| items.len()).ok();
    release.send(()).unwrap();
    worker.join().unwrap();
    let media = fs::read_dir(&root).unwrap().count();
    cleanup(&root);
    assert_eq!(
        visible,
        Some(1),
        "persistence must neither lock nor preemptively change committed cache"
    );
    assert_eq!(media, 1);
}

#[test]
fn admitted_write_finishes_before_later_clear_without_blocking_admission() {
    let root = root();
    let cache = Arc::new(Mutex::new(seed(&root, 1)));
    let epoch = Arc::new(AtomicU64::new(0));
    let (manager, rx) = manager(cache.clone(), epoch.clone());
    let token = crate::overlay::result::ChainCancelToken::new();
    manager.save_text_cancellable("new".into(), "new-source".into(), token.clone());
    let failed = manager.persistence_failed.clone();
    let worker_cache = cache.clone();
    let worker_root = root.clone();
    let (entered, ready) = mpsc::channel();
    let (release, blocked) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let mut snapshots = Vec::new();
        process_queue_with(rx, worker_cache, 10, epoch, &worker_root, failed, |items| {
            snapshots.push(items.len());
            if snapshots.len() == 1 {
                entered.send(()).unwrap();
                blocked.recv_timeout(Duration::from_secs(3)).unwrap();
            }
            true
        });
        snapshots
    });
    ready.recv_timeout(Duration::from_secs(3)).unwrap();
    let visible = cache.try_lock().map(|items| items.len()).ok();
    manager.clear_all();
    token.cancel();
    drop(manager);
    release.send(()).unwrap();
    let snapshots = worker.join().unwrap();
    let count = cache.lock().unwrap().len();
    let media = fs::read_dir(&root).unwrap().count();
    cleanup(&root);
    assert_eq!(visible, Some(1));
    assert_eq!(snapshots, vec![2, 0]);
    assert_eq!(count, 0);
    assert_eq!(media, 0);
}

#[test]
fn unreadable_or_invalid_database_is_never_replaced_by_empty_history() {
    for directory in [false, true] {
        let root = root();
        let database = root.join("history.json");
        if directory {
            fs::create_dir(&database).unwrap();
        } else {
            fs::write(&database, b"{").unwrap();
        }
        let manager = HistoryManager::new_with_paths(10, database.clone(), root.clone());
        manager.save_text_cancellable(
            "new".into(),
            "new-source".into(),
            crate::overlay::result::ChainCancelToken::new(),
        );
        manager.clear_all();
        manager.request_prune(10);
        let failed = manager.persistence_failed();
        let read_failed = manager.load_failed();
        let visible = manager.snapshot();
        drop(manager);
        if directory {
            assert!(database.is_dir());
            fs::remove_dir(database).unwrap();
        } else {
            let bytes = fs::read(&database).unwrap();
            assert_eq!(bytes, b"{");
            fs::remove_file(database).unwrap();
        }
        cleanup(&root);
        assert!(failed && read_failed);
        assert!(visible.is_empty());
    }
}
