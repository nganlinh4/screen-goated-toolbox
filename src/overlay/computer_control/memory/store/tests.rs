use std::sync::{Arc, Barrier};

use super::*;

struct TestRoot(PathBuf);

impl TestRoot {
    fn new() -> Self {
        static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "sgt-memory-store-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn store(&self, limit: usize) -> Store {
        Store::new(self.0.clone(), limit)
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        // Every test owns one fresh flat directory; never recurse into unknown data.
        for entry in fs::read_dir(&self.0).unwrap() {
            let path = entry.unwrap().path();
            assert_eq!(path.parent(), Some(self.0.as_path()));
            fs::remove_file(path).unwrap();
        }
        fs::remove_dir(&self.0).unwrap();
    }
}

fn lines(text: &str) -> Vec<String> {
    vec![format!("User: {text}"), "Assistant: complete".into()]
}

fn image() -> Image {
    Image {
        jpeg: vec![1, 2, 3],
        turn_id: 7,
        source: serde_json::json!({"frame_id": 9}),
    }
}

fn backups(root: &TestRoot) -> Vec<PathBuf> {
    fs::read_dir(&root.0)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .contains(".corrupt-")
        })
        .collect()
}

#[test]
fn concurrent_saves_and_embedding_completions_preserve_all_records() {
    let root = TestRoot::new();
    let barrier = Arc::new(Barrier::new(4));
    let mut workers = Vec::new();
    for worker in 0..4 {
        let store = root.store(64);
        let barrier = Arc::clone(&barrier);
        workers.push(std::thread::spawn(move || {
            barrier.wait();
            let mut ids = Vec::new();
            for turn in 0..4 {
                let id = store
                    .create(&lines(&format!("{worker}/{turn}")), &[])
                    .unwrap();
                store
                    .set_embedding(id, vec![worker as f32, turn as f32])
                    .unwrap();
                ids.push(id);
            }
            ids
        }));
    }
    let mut ids = workers
        .into_iter()
        .flat_map(|worker| worker.join().unwrap())
        .collect::<Vec<_>>();
    ids.sort_unstable();
    ids.dedup();
    let index = root.store(64).snapshot().unwrap();
    assert_eq!(ids.len(), 16);
    assert_eq!(index.len(), 16);
    assert!(index.iter().all(|meta| meta.embedding.len() == 2));
    assert!(index.windows(2).all(|pair| pair[0].id > pair[1].id));
    for id in ids {
        assert!(root.store(64).open(id).unwrap().is_some());
    }
}

#[test]
fn independent_processes_preserve_all_records() {
    const ROOT: &str = "SGT_TEST_CONVERSATION_STORE_ROOT";
    const WORKER: &str = "SGT_TEST_CONVERSATION_STORE_WORKER";
    if let Some(root) = std::env::var_os(ROOT) {
        let store = Store::new(PathBuf::from(root), 64);
        let worker = std::env::var(WORKER).unwrap();
        for turn in 0..6 {
            let id = store
                .create(&lines(&format!("{worker}/{turn}")), &[])
                .unwrap();
            store.set_embedding(id, vec![turn as f32]).unwrap();
        }
        return;
    }
    let root = TestRoot::new();
    let mut children = (0..2).map(|worker| {
        std::process::Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg("overlay::computer_control::memory::store::tests::independent_processes_preserve_all_records")
            .env(ROOT, &root.0).env(WORKER, worker.to_string()).spawn().unwrap()
    }).collect::<Vec<_>>();
    for child in &mut children {
        assert!(child.wait().unwrap().success());
    }
    let index = root.store(64).snapshot().unwrap();
    assert_eq!(index.len(), 12);
    assert!(index.iter().all(|meta| meta.embedding.len() == 1));
}

#[test]
fn corrupt_index_is_preserved_and_sidecars_restore_embeddings_and_provenance() {
    let root = TestRoot::new();
    let store = root.store(10);
    let id = store.create(&lines("多语言 🧭"), &[image()]).unwrap();
    store.set_embedding(id, vec![0.25, 0.5]).unwrap();
    let path = root.0.join("index.json");
    fs::write(&path, b"{broken").unwrap();
    let index = store.snapshot().unwrap();
    assert_eq!(index.len(), 1);
    assert_eq!(index[0].embedding, [0.25, 0.5]);
    assert_eq!(index[0].title, "多语言 🧭");
    assert_eq!(backups(&root).len(), 1);
    assert_eq!(fs::read(&backups(&root)[0]).unwrap(), b"{broken");
    assert_eq!(
        files::read_record(&store.record_path(id)).unwrap().frames[0].source["frame_id"],
        9
    );
    store.snapshot().unwrap();
    assert_eq!(backups(&root).len(), 1);
    fs::remove_file(path).unwrap();
    assert_eq!(store.snapshot().unwrap(), index);
}

#[test]
fn legacy_sidecars_migrate_and_keep_available_index_embeddings() {
    let root = TestRoot::new();
    let store = root.store(10);
    let id = 123;
    let path = store.record_path(id);
    fs::write(
        &path,
        serde_json::to_vec(&serde_json::json!({"lines": lines("earlier")})).unwrap(),
    )
    .unwrap();
    fs::write(root.0.join(format!("img_{id}_0.jpg")), b"image").unwrap();
    crate::atomic_json::write_json_atomic(
        &root.0.join("index.json"),
        &vec![ConvMeta {
            id,
            timestamp: "earlier timestamp".into(),
            embedding: vec![1.0],
            ..ConvMeta::default()
        }],
    )
    .unwrap();
    let index = store.snapshot().unwrap();
    assert_eq!(index[0].embedding, [1.0]);
    assert_eq!(index[0].timestamp, "earlier timestamp");
    assert_eq!(index[0].images, [format!("img_{id}_0.jpg")]);
    assert_eq!(files::read_record(&path).unwrap().version, 1);
    fs::remove_file(root.0.join("index.json")).unwrap();
    assert_eq!(store.snapshot().unwrap(), index);
    assert!(store.open(id).unwrap().unwrap().contains("earlier"));
}

#[test]
fn corrupt_record_does_not_hide_healthy_records_or_erase_corrupt_bytes() {
    let root = TestRoot::new();
    let store = root.store(10);
    let healthy = store.create(&lines("healthy"), &[]).unwrap();
    let corrupt = store.create(&lines("corrupt"), &[image()]).unwrap();
    let path = store.record_path(corrupt);
    fs::write(&path, b"partial record").unwrap();
    assert_eq!(
        store
            .snapshot()
            .unwrap()
            .iter()
            .map(|meta| meta.id)
            .collect::<Vec<_>>(),
        [healthy]
    );
    assert_eq!(fs::read(path).unwrap(), b"partial record");
    assert_eq!(backups(&root).len(), 1);
    assert!(root.0.join(format!("img_{corrupt}_0.jpg")).exists());
    assert!(store.create(&lines("later"), &[]).unwrap() > corrupt);
}

#[test]
fn retention_and_late_embedding_never_resurrect_pruned_records() {
    let root = TestRoot::new();
    let store = root.store(1);
    let old = store.create(&lines("old"), &[image()]).unwrap();
    let newest = store.create(&lines("new"), &[]).unwrap();
    store.set_embedding(old, vec![1.0]).unwrap();
    let index = store.snapshot().unwrap();
    assert_eq!(
        index.iter().map(|meta| meta.id).collect::<Vec<_>>(),
        [newest]
    );
    assert!(store.open(old).unwrap().is_none());
    assert!(!store.record_path(old).exists());
    assert!(!root.0.join(format!("img_{old}_0.jpg")).exists());
}

#[test]
fn retirement_recovery_deletes_only_canonical_owned_images() {
    let root = TestRoot::new();
    let store = root.store(10);
    let id = store.create(&lines("retiring"), &[image()]).unwrap();
    let unrelated = root.0.join("user-export.jpg");
    fs::write(&unrelated, b"preserve").unwrap();
    let mut record = files::read_record(&store.record_path(id)).unwrap();
    record
        .metadata
        .as_mut()
        .unwrap()
        .images
        .extend(["../outside.jpg".into(), "user-export.jpg".into()]);
    let retired = root.0.join(format!("retired_{id}.json"));
    files::write_record(&retired, &record).unwrap();
    fs::remove_file(store.record_path(id)).unwrap();
    assert!(store.snapshot().unwrap().is_empty());
    assert!(!retired.exists());
    assert!(!root.0.join(format!("img_{id}_0.jpg")).exists());
    assert_eq!(fs::read(unrelated).unwrap(), b"preserve");
}

#[test]
fn unknown_record_versions_fail_without_rewriting_the_index_or_record() {
    let root = TestRoot::new();
    let store = root.store(10);
    let id = store.create(&lines("future"), &[]).unwrap();
    let mut record = files::read_record(&store.record_path(id)).unwrap();
    record.version = 2;
    files::write_record(&store.record_path(id), &record).unwrap();
    let old_index = fs::read(root.0.join("index.json")).unwrap();
    let old_record = fs::read(store.record_path(id)).unwrap();
    assert!(store.snapshot().is_err());
    assert_eq!(fs::read(root.0.join("index.json")).unwrap(), old_index);
    assert_eq!(fs::read(store.record_path(id)).unwrap(), old_record);
}

#[test]
fn failed_record_publication_rolls_back_only_its_new_images() {
    let root = TestRoot::new();
    let store = root.store(10);
    let existing = store.create(&lines("existing"), &[image()]).unwrap();
    let mut oversize = image();
    oversize.source = serde_json::Value::String("x".repeat(16 * 1024 * 1024));
    assert!(store.create(&lines("oversize"), &[oversize]).is_err());
    assert_eq!(store.snapshot().unwrap().len(), 1);
    let images = fs::read_dir(&root.0)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .filter(|name| name.to_string_lossy().ends_with(".jpg"))
        .collect::<Vec<_>>();
    assert_eq!(
        images,
        [std::ffi::OsString::from(format!("img_{existing}_0.jpg"))]
    );
}

#[test]
fn recovery_preserves_canonical_zero_and_negative_legacy_identities() {
    let root = TestRoot::new();
    let store = root.store(10);
    for id in [0, -9] {
        fs::write(
            store.record_path(id),
            serde_json::to_vec(&serde_json::json!({"lines": lines("earlier clock")})).unwrap(),
        )
        .unwrap();
    }
    assert_eq!(
        store
            .snapshot()
            .unwrap()
            .iter()
            .map(|meta| meta.id)
            .collect::<Vec<_>>(),
        [0, -9]
    );
    for id in [0, -9] {
        assert!(store.open(id).unwrap().is_some());
    }
}

#[cfg(windows)]
#[test]
fn locked_retired_image_defers_cleanup_without_hiding_healthy_records() {
    use std::os::windows::fs::OpenOptionsExt;
    let root = TestRoot::new();
    let initial = root.store(2);
    let old = initial.create(&lines("old"), &[image()]).unwrap();
    let newest = initial.create(&lines("healthy"), &[]).unwrap();
    let image_path = root.0.join(format!("img_{old}_0.jpg"));
    let held = fs::OpenOptions::new()
        .read(true)
        .share_mode(windows::Win32::Storage::FileSystem::FILE_SHARE_READ.0)
        .open(&image_path)
        .unwrap();
    let store = root.store(1);
    assert_eq!(
        store
            .snapshot()
            .unwrap()
            .iter()
            .map(|meta| meta.id)
            .collect::<Vec<_>>(),
        [newest]
    );
    let retired = root.0.join(format!("retired_{old}.json"));
    assert!(retired.exists());
    assert!(store.open(newest).unwrap().is_some());
    drop(held);
    store.snapshot().unwrap();
    assert!(!retired.exists());
    assert!(!image_path.exists());
}

#[test]
fn clear_deletes_every_record_and_refuses_saves_admitted_before_it() {
    let root = TestRoot::new();
    let store = root.store(8);
    let saved = store.create(&lines("private"), &[image()]).unwrap();
    store.create(&lines("second"), &[image()]).unwrap();
    fs::write(root.0.join("index.json.corrupt-1"), b"{").unwrap();
    // A finished session admitted before the clear, still waiting to publish.
    let in_flight = root.store(8).admitted_at(clear_generation());

    store.clear().unwrap();

    let left = fs::read_dir(&root.0)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(left, [locking::LOCK_FILE]);
    assert!(in_flight.create(&lines("late"), &[image()]).is_err());
    // A late embedding for a cleared record does not resurrect it.
    store.set_embedding(saved, vec![1.0]).unwrap();
    assert!(store.snapshot().unwrap().is_empty());
    assert!(store.open(saved).unwrap().is_none());
    // Saves admitted after the clear publish normally.
    let fresh = root.store(8).admitted_at(clear_generation());
    assert!(fresh.create(&lines("after"), &[]).is_ok());
}
