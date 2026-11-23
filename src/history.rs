use image::{ImageBuffer, Rgba};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum HistoryType {
    Image,
    Audio,
    Text,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct HistoryItem {
    pub id: i64,
    pub timestamp: String,
    pub item_type: HistoryType,
    pub text: String,
    pub media_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    media_proof: Option<MediaProof>,
}

impl HistoryItem {
    pub(crate) fn matches_query(&self, lowercase_query: &str) -> bool {
        lowercase_query.is_empty()
            || self.text.to_lowercase().contains(lowercase_query)
            || self.timestamp.contains(lowercase_query)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct MediaProof {
    file_identity: String,
    bytes: u64,
    sha256: String,
}

#[derive(Clone)]
struct MediaRef {
    filename: String,
    proof: Option<MediaProof>,
}

impl From<&HistoryItem> for MediaRef {
    fn from(item: &HistoryItem) -> Self {
        Self {
            filename: item.media_path.clone(),
            proof: item.media_proof.clone(),
        }
    }
}

enum HistoryAction {
    SaveImage {
        img: ImageBuffer<Rgba<u8>, Vec<u8>>,
        text: String,
        cancel_token: Option<Arc<crate::overlay::result::ChainCancelToken>>,
        clear_epoch: u64,
    },
    SaveAudio {
        wav_data: Arc<[u8]>,
        text: String,
        cancel_token: Option<Arc<crate::overlay::result::ChainCancelToken>>,
        clear_epoch: u64,
    },
    SaveText {
        result_text: String,
        input_text: String,
        cancel_token: Option<Arc<crate::overlay::result::ChainCancelToken>>,
        clear_epoch: u64,
    },
    Delete {
        item: HistoryItem,
    },
    ClearAll,
    Prune(usize),
}

pub struct HistoryManager {
    tx: Sender<HistoryAction>,
    items: Arc<Mutex<Vec<HistoryItem>>>,
    persistence_failed: Arc<AtomicBool>,
    load_failed: bool,
    submission: Mutex<()>,
    clear_epoch: Arc<AtomicU64>,
}

mod worker;

fn history_media_leaf(name: &str) -> bool {
    if name.trim().is_empty()
        || name == "."
        || name == ".."
        || name
            .chars()
            .any(|character| matches!(character, '/' | '\\' | ':') || (character as u32) < 32)
    {
        return false;
    }
    let mut components = Path::new(name).components();
    matches!(
        (components.next(), components.next()),
        (Some(Component::Normal(_)), None)
    )
}

fn history_media_name_matches(item_type: &HistoryType, name: &str) -> bool {
    if !history_media_leaf(name) {
        return false;
    }
    let expected = match item_type {
        HistoryType::Image => "png",
        HistoryType::Audio => "wav",
        HistoryType::Text => "txt",
    };
    Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case(expected))
}

impl HistoryManager {
    pub fn new(max_items: usize) -> Self {
        let (_, db_path, media_dir) = get_paths();
        Self::new_with_paths(max_items, db_path, media_dir)
    }

    fn new_with_paths(max_items: usize, db_path: PathBuf, media_dir: PathBuf) -> Self {
        let (tx, rx) = channel();
        let (initial_items, load_failed) = match load_items(&db_path) {
            Ok(items) => (items, false),
            Err(error) => {
                crate::log_info!("[history] failed to load history db: {:?}", error.kind());
                (Vec::new(), true)
            }
        };
        let items = Arc::new(Mutex::new(initial_items));
        let clear_epoch = Arc::new(AtomicU64::new(0));
        let persistence_failed = Arc::new(AtomicBool::new(load_failed));
        if load_failed {
            drop(rx);
        } else {
            let worker_items = items.clone();
            let worker_epoch = clear_epoch.clone();
            let worker_failed = persistence_failed.clone();
            thread::spawn(move || {
                worker::process_queue(
                    rx,
                    worker_items,
                    max_items.clamp(10, 200),
                    worker_epoch,
                    worker_failed,
                    db_path,
                    media_dir,
                )
            });
        }
        Self {
            tx,
            items,
            persistence_failed,
            load_failed,
            submission: Mutex::new(()),
            clear_epoch,
        }
    }

    /// Identifies the published list without copying it, so a view can reuse
    /// what it derived from the last snapshot until the list changes.
    pub(crate) fn content_key(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let items = self.items.lock().unwrap_or_else(|error| error.into_inner());
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        items.len().hash(&mut hasher);
        for item in items.iter() {
            (item.id, item.text.len(), item.media_path.len()).hash(&mut hasher);
        }
        hasher.finish()
    }

    pub fn snapshot(&self) -> Vec<HistoryItem> {
        self.items
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    pub(crate) fn media_path_for(&self, directory: &Path, item: &HistoryItem) -> Option<PathBuf> {
        if self.persistence_failed()
            || !history_media_name_matches(&item.item_type, &item.media_path)
        {
            return None;
        }
        let items = self.items.lock().unwrap_or_else(|error| error.into_inner());
        let mut matches = items.iter().filter(|current| current.id == item.id);
        if matches.next() != Some(item) || matches.next().is_some() {
            return None;
        }
        Some(directory.join(&item.media_path))
    }

    pub fn persistence_failed(&self) -> bool {
        self.persistence_failed.load(Ordering::Acquire)
    }

    pub fn load_failed(&self) -> bool {
        self.load_failed
    }

    fn enqueue_save(&self, make: impl FnOnce(u64) -> HistoryAction) {
        let _submission = self
            .submission
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if self
            .tx
            .send(make(self.clear_epoch.load(Ordering::Acquire)))
            .is_err()
        {
            self.persistence_failed.store(true, Ordering::Release);
            drop(_submission);
            notify_ui();
        }
    }

    pub(crate) fn save_image_cancellable(
        &self,
        img: ImageBuffer<Rgba<u8>, Vec<u8>>,
        text: String,
        token: Arc<crate::overlay::result::ChainCancelToken>,
    ) {
        self.enqueue_save(|clear_epoch| HistoryAction::SaveImage {
            img,
            text,
            cancel_token: Some(token),
            clear_epoch,
        });
    }

    pub fn save_audio(&self, wav_data: Vec<u8>, text: String) {
        self.enqueue_save(|clear_epoch| HistoryAction::SaveAudio {
            wav_data: wav_data.into(),
            text,
            cancel_token: None,
            clear_epoch,
        });
    }

    pub(crate) fn save_audio_shared_cancellable(
        &self,
        wav_data: Arc<[u8]>,
        text: String,
        cancel_token: Arc<crate::overlay::result::ChainCancelToken>,
    ) {
        self.enqueue_save(|clear_epoch| HistoryAction::SaveAudio {
            wav_data,
            text,
            cancel_token: Some(cancel_token),
            clear_epoch,
        });
    }

    pub(crate) fn save_text_cancellable(
        &self,
        result_text: String,
        input_text: String,
        token: Arc<crate::overlay::result::ChainCancelToken>,
    ) {
        if !result_text.trim().is_empty() {
            self.enqueue_save(|clear_epoch| HistoryAction::SaveText {
                result_text,
                input_text,
                cancel_token: Some(token),
                clear_epoch,
            });
        }
    }

    pub fn delete(&self, id: i64) {
        let _submission = self
            .submission
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let items = self.items.lock().unwrap_or_else(|error| error.into_inner());
        let mut matches = items.iter().filter(|item| item.id == id);
        let Some(item) = matches.next().cloned() else {
            return;
        };
        let ambiguous = matches.next().is_some();
        drop(matches);
        drop(items);
        if ambiguous {
            self.persistence_failed.store(true, Ordering::Release);
            drop(_submission);
            notify_ui();
            return;
        }
        if self.tx.send(HistoryAction::Delete { item }).is_err() {
            self.persistence_failed.store(true, Ordering::Release);
            drop(_submission);
            notify_ui();
        }
    }

    pub fn clear_all(&self) {
        let _submission = self
            .submission
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let Some(next) = self.clear_epoch.load(Ordering::Acquire).checked_add(1) else {
            self.persistence_failed.store(true, Ordering::Release);
            drop(_submission);
            notify_ui();
            return;
        };
        if self.tx.send(HistoryAction::ClearAll).is_ok() {
            self.clear_epoch.store(next, Ordering::Release);
        } else {
            self.persistence_failed.store(true, Ordering::Release);
            drop(_submission);
            notify_ui();
        }
    }

    pub fn request_prune(&self, limit: usize) {
        let _submission = self
            .submission
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if self
            .tx
            .send(HistoryAction::Prune(limit.clamp(10, 200)))
            .is_err()
        {
            self.persistence_failed.store(true, Ordering::Release);
            drop(_submission);
            notify_ui();
        }
    }
}

fn get_paths() -> (PathBuf, PathBuf, PathBuf) {
    let config_dir = crate::paths::app_runtime_config_dir();
    let media_dir = config_dir.join("history_media");
    let db_path = config_dir.join("history.json");
    let _ = fs::create_dir_all(&media_dir);
    (config_dir, db_path, media_dir)
}

fn load_items(path: &Path) -> std::io::Result<Vec<HistoryItem>> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    serde_json::from_reader(file)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

fn save_db_at(path: &Path, items: &[HistoryItem]) -> bool {
    if let Err(error) = crate::atomic_json::write_json_atomic(path, &items) {
        crate::log_info!("[history] failed to save history db: {error}");
        false
    } else {
        true
    }
}

fn notify_ui() {
    let context = crate::gui::GUI_CONTEXT
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone();
    if let Some(context) = context {
        context.request_repaint();
    }
}

fn run_history_action(
    token: Option<Arc<crate::overlay::result::ChainCancelToken>>,
    commit: impl FnOnce(),
) {
    if let Some(token) = token {
        let _ = token.run_if_active(commit);
    } else {
        commit();
    }
}

#[cfg(test)]
mod cancellation_tests {
    use super::*;

    #[test]
    fn shared_media_name_cases_reject_non_leaf_paths() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../parity-fixtures/history-ui/media-names.json"
        ))
        .unwrap();
        assert_eq!(fixture["schema_version"], 1);
        for case in fixture["cases"].as_array().unwrap() {
            assert_eq!(
                history_media_leaf(case["value"].as_str().unwrap()),
                case["valid"].as_bool().unwrap(),
                "{}",
                case["name"].as_str().unwrap()
            );
        }
    }

    #[test]
    fn shared_media_type_cases_reject_wrong_file_types() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../parity-fixtures/history-ui/media-types.json"
        ))
        .unwrap();
        assert_eq!(fixture["schema_version"], 1);
        for case in fixture["cases"].as_array().unwrap() {
            let item_type = match case["type"].as_str().unwrap() {
                "IMAGE" => HistoryType::Image,
                "AUDIO" => HistoryType::Audio,
                "TEXT" => HistoryType::Text,
                _ => panic!("unknown media type"),
            };
            assert_eq!(
                history_media_name_matches(&item_type, case["name"].as_str().unwrap()),
                case["valid"].as_bool().unwrap(),
                "{}",
                case["name"].as_str().unwrap()
            );
        }
    }

    #[test]
    fn media_open_requires_one_unchanged_current_item() {
        let item = HistoryItem {
            id: 7,
            timestamp: String::new(),
            item_type: HistoryType::Text,
            text: "current".into(),
            media_path: "source.txt".into(),
            media_proof: None,
        };
        let (tx, _rx) = channel();
        let history = HistoryManager {
            tx,
            items: Arc::new(Mutex::new(vec![item.clone()])),
            persistence_failed: Arc::new(AtomicBool::new(false)),
            load_failed: false,
            submission: Mutex::new(()),
            clear_epoch: Arc::new(AtomicU64::new(0)),
        };
        let directory = Path::new("history_media");
        assert_eq!(
            history.media_path_for(directory, &item),
            Some(directory.join("source.txt"))
        );
        let mut stale = item.clone();
        stale.text = "stale".into();
        assert_eq!(history.media_path_for(directory, &stale), None);
        let mut executable = item.clone();
        executable.media_path = "source.txt.exe".into();
        *history.items.lock().unwrap() = vec![executable.clone()];
        assert_eq!(history.media_path_for(directory, &executable), None);
        *history.items.lock().unwrap() = vec![item.clone()];
        history.items.lock().unwrap().push(stale);
        assert_eq!(history.media_path_for(directory, &item), None);
        history.items.lock().unwrap().pop();
        history.persistence_failed.store(true, Ordering::Release);
        assert_eq!(history.media_path_for(directory, &item), None);
    }

    #[test]
    fn cancelled_queued_action_is_not_committed() {
        let token = crate::overlay::result::ChainCancelToken::new();
        let (sender, receiver) = std::sync::mpsc::channel();
        sender.send(token.clone()).unwrap();
        token.cancel();
        let mut committed = false;
        run_history_action(Some(receiver.recv().unwrap()), || committed = true);
        assert!(!committed);
    }

    #[test]
    fn clear_orders_queued_saves_without_changing_committed_cache() {
        let (tx, rx) = channel();
        let items = Arc::new(Mutex::new(vec![HistoryItem {
            id: 1,
            timestamp: String::new(),
            item_type: HistoryType::Image,
            text: String::new(),
            media_path: "old.png".into(),
            media_proof: None,
        }]));
        let history = HistoryManager {
            tx,
            items: items.clone(),
            persistence_failed: Arc::new(AtomicBool::new(false)),
            load_failed: false,
            submission: Mutex::new(()),
            clear_epoch: Arc::new(AtomicU64::new(0)),
        };
        history.save_audio(vec![1], "before".into());
        history.clear_all();
        history.save_audio(vec![2], "after".into());
        assert_eq!(items.lock().unwrap().len(), 1);
        assert!(matches!(
            rx.recv().unwrap(),
            HistoryAction::SaveAudio { clear_epoch: 0, .. }
        ));
        assert!(matches!(rx.recv().unwrap(), HistoryAction::ClearAll));
        assert!(matches!(
            rx.recv().unwrap(),
            HistoryAction::SaveAudio { clear_epoch: 1, .. }
        ));
    }
}
