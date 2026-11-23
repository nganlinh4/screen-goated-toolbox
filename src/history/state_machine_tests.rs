use super::*;
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::sync::mpsc;

struct Fixture {
    root: PathBuf,
    media: PathBuf,
    db: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "sgt-history-state-{}-{}",
            std::process::id(),
            MEDIA_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let media = root.join("media");
        fs::create_dir(&media).unwrap();
        Self {
            db: root.join("history.json"),
            root,
            media,
        }
    }

    fn run(
        &self,
        cache: &Arc<Mutex<Vec<HistoryItem>>>,
        limit: usize,
        action: Option<HistoryAction>,
    ) {
        let (tx, rx) = mpsc::channel();
        if let Some(action) = action {
            tx.send(action).unwrap();
        }
        drop(tx);
        let failed = Arc::new(AtomicBool::new(false));
        process_queue(
            rx,
            cache.clone(),
            limit,
            Arc::new(AtomicU64::new(0)),
            failed.clone(),
            self.db.clone(),
            self.media.clone(),
        );
        assert!(!failed.load(Ordering::Acquire));
        assert_eq!(
            super::super::load_items(&self.db).unwrap(),
            *cache.lock().unwrap()
        );
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        for directory in [&self.media, &self.root] {
            for entry in fs::read_dir(directory).unwrap() {
                let entry = entry.unwrap();
                assert!(entry.file_type().unwrap().is_file());
                fs::remove_file(entry.path()).unwrap();
            }
            fs::remove_dir(directory).unwrap();
        }
    }
}

fn item(value: &Value) -> HistoryItem {
    assert_eq!(value["itemType"], "TEXT");
    HistoryItem {
        id: value["id"].as_i64().unwrap(),
        timestamp: value["timestamp"].as_str().unwrap().into(),
        item_type: HistoryType::Text,
        text: value["text"].as_str().unwrap().into(),
        media_path: value["mediaPath"].as_str().unwrap().into(),
        media_proof: None,
    }
}

#[test]
fn shared_history_state_machine_replays_worker_storage_and_search() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../parity-fixtures/history-ui/state-machine.json"
    ))
    .unwrap();
    let default = crate::config::Config::default().max_history_items;
    assert_eq!(default as u64, fixture["defaultMaxItems"].as_u64().unwrap());
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        match name {
            "legacy_android_default_200_migrates_to_windows_default_50" => continue,
            "settings_reset_restores_history_default_50" => {
                assert_eq!(
                    default as u64,
                    case["expectedNormalizedMaxItems"].as_u64().unwrap()
                );
                continue;
            }
            "search_matches_text_and_timestamp_only" => {
                let items: Vec<_> = case["items"].as_array().unwrap().iter().map(item).collect();
                let query = case["query"].as_str().unwrap().to_lowercase();
                assert_eq!(
                    items
                        .iter()
                        .filter(|item| item.matches_query(&query))
                        .count() as u64,
                    case["expectedMatchCount"].as_u64().unwrap()
                );
                for item in items {
                    assert!(item.matches_query(&item.text.to_lowercase()));
                    assert!(!item.matches_query(&item.media_path.to_lowercase()));
                }
                continue;
            }
            _ => {}
        }
        let files = Fixture::new();
        let initial = case["storedItems"]
            .as_array()
            .map_or_else(Vec::new, |items| items.iter().map(item).collect::<Vec<_>>());
        for item in &initial {
            fs::write(files.media.join(&item.media_path), b"legacy source").unwrap();
        }
        if let Some(orphans) = case["orphanMediaFiles"].as_array() {
            for orphan in orphans {
                fs::write(files.media.join(orphan.as_str().unwrap()), b"unowned").unwrap();
            }
        }
        assert!(save_db_at(&files.db, &initial));
        let cache = Arc::new(Mutex::new(super::super::load_items(&files.db).unwrap()));
        let mut limit = case["storedSettings"]["maxItems"]
            .as_u64()
            .unwrap_or(default as u64) as usize;
        files.run(&cache, limit, None);
        let operations = case["operations"]
            .as_array()
            .cloned()
            .unwrap_or_else(|| case.get("operation").cloned().into_iter().collect());
        for operation in operations {
            let action = match operation["type"].as_str().unwrap() {
                "save_text" => HistoryAction::SaveText {
                    result_text: operation["resultText"].as_str().unwrap().into(),
                    input_text: operation["sourceText"].as_str().unwrap().into(),
                    cancel_token: None,
                    clear_epoch: 0,
                },
                "set_max_items" => {
                    limit = operation["value"].as_u64().unwrap() as usize;
                    HistoryAction::Prune(limit)
                }
                "delete_by_result_text" => HistoryAction::Delete {
                    item: cache
                        .lock()
                        .unwrap()
                        .iter()
                        .find(|item| item.text == operation["resultText"].as_str().unwrap())
                        .unwrap()
                        .clone(),
                },
                "clear_all" => HistoryAction::ClearAll,
                _ => panic!("unhandled history operation: {operation}"),
            };
            files.run(&cache, limit, Some(action));
        }
        let items = cache.lock().unwrap();
        if let Some(order) = case["expectedOrder"].as_array() {
            assert_eq!(
                serde_json::to_value(items.iter().map(|item| &item.text).collect::<Vec<_>>())
                    .unwrap(),
                Value::Array(order.clone()),
                "{name}"
            );
        }
        if let Some(count) = case["expectedCount"].as_u64() {
            assert_eq!(items.len() as u64, count, "{name}");
        }
        if case["expectedUniqueMediaPaths"].as_bool() == Some(true) {
            assert_eq!(
                items
                    .iter()
                    .map(|item| &item.media_path)
                    .collect::<HashSet<_>>()
                    .len(),
                items.len(),
                "{name}"
            );
        }
        if case.get("expectedItemType").is_some() {
            assert_eq!(case["expectedItemType"], "TEXT");
            assert_eq!(items[0].item_type, HistoryType::Text);
            assert_eq!(items[0].text, case["expectedVisibleText"].as_str().unwrap());
            assert_eq!(
                fs::read_to_string(files.media.join(&items[0].media_path)).unwrap(),
                case["expectedBackingText"].as_str().unwrap()
            );
        }
        if let Some(backing) = case["expectedBackingTextsByResult"].as_object() {
            for item in items.iter() {
                assert_eq!(
                    fs::read_to_string(files.media.join(&item.media_path)).unwrap(),
                    backing[&item.text].as_str().unwrap(),
                    "{name}"
                );
            }
        }
        for key in ["expectedRetainedMediaPaths", "expectedRemainingMediaFiles"] {
            if let Some(paths) = case[key].as_array() {
                for path in paths {
                    assert!(files.media.join(path.as_str().unwrap()).is_file(), "{name}");
                }
            }
        }
        if let Some(paths) = case["expectedDeletedMediaPaths"].as_array() {
            for path in paths {
                assert!(!files.media.join(path.as_str().unwrap()).exists(), "{name}");
            }
        }
    }
}
