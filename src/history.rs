use chrono::Local;
use image::{ImageBuffer, Rgba};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum HistoryType {
    Image,
    Audio,
    Text, // NEW: Text-only history entries (no media file)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HistoryItem {
    pub id: i64,
    pub timestamp: String,
    pub item_type: HistoryType,
    pub text: String,
    pub media_path: String, // Empty for Text type
}

pub enum HistoryAction {
    SaveImage {
        img: ImageBuffer<Rgba<u8>, Vec<u8>>,
        text: String,
    },
    SaveAudio {
        wav_data: Vec<u8>,
        text: String,
    },
    SaveText {
        result_text: String,
        input_text: String,
    }, // NEW: Save text-only entry
    Delete {
        media_path: String,
    },
    ClearAll,
    Prune(usize),
}

pub struct HistoryManager {
    tx: Sender<HistoryAction>,
    pub items: Arc<Mutex<Vec<HistoryItem>>>,
}

impl HistoryManager {
    pub fn new(max_items: usize) -> Self {
        let (tx, rx) = channel();
        // Load initial items
        let (_, db_path, _) = get_paths();
        let initial_items = if db_path.exists() {
            let file = fs::File::open(&db_path).ok();
            if let Some(f) = file {
                serde_json::from_reader(f).unwrap_or_default()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        let items = Arc::new(Mutex::new(initial_items));
        let items_clone = items.clone();

        thread::spawn(move || {
            process_queue(rx, items_clone, max_items);
        });

        Self { tx, items }
    }

    pub fn save_image(&self, img: ImageBuffer<Rgba<u8>, Vec<u8>>, text: String) {
        let _ = self.tx.send(HistoryAction::SaveImage { img, text });
    }

    pub fn save_audio(&self, wav_data: Vec<u8>, text: String) {
        let _ = self.tx.send(HistoryAction::SaveAudio { wav_data, text });
    }

    pub fn save_text(&self, result_text: String, input_text: String) {
        if !result_text.trim().is_empty() {
            let _ = self.tx.send(HistoryAction::SaveText {
                result_text,
                input_text,
            });
        }
    }

    pub fn delete(&self, id: i64) {
        // Remove from the shared cache for instant UI feedback, and hand the
        // item's media path to the worker so it can delete the file and persist
        // the new state. The worker must NOT re-look-up the item in the cache —
        // it's already gone — or it would skip the file delete AND the DB save,
        // leaking the file and resurrecting the entry on the next launch.
        let media_path = {
            let mut guard = self.items.lock().unwrap();
            match guard.iter().position(|x| x.id == id) {
                Some(pos) => guard.remove(pos).media_path,
                None => return,
            }
        };
        let _ = self.tx.send(HistoryAction::Delete { media_path });
    }

    pub fn clear_all(&self) {
        let _ = self.tx.send(HistoryAction::ClearAll);
        let mut guard = self.items.lock().unwrap();
        guard.clear();
    }

    pub fn request_prune(&self, limit: usize) {
        let _ = self.tx.send(HistoryAction::Prune(limit));
    }
}

fn get_paths() -> (PathBuf, PathBuf, PathBuf) {
    let config_dir = dirs::config_dir()
        .unwrap_or_default()
        .join("screen-goated-toolbox");
    let media_dir = config_dir.join("history_media");
    let db_path = config_dir.join("history.json");
    let _ = fs::create_dir_all(&media_dir);
    (config_dir, db_path, media_dir)
}

fn save_db(items: &Vec<HistoryItem>) {
    let (_, db_path, _) = get_paths();
    if let Ok(file) = fs::File::create(db_path) {
        let _ = serde_json::to_writer_pretty(file, items);
    }
}

fn process_queue(
    rx: Receiver<HistoryAction>,
    cache: Arc<Mutex<Vec<HistoryItem>>>,
    mut max_items: usize,
) {
    let (_, _, media_dir) = get_paths();

    while let Ok(action) = rx.recv() {
        let mut should_save = false;
        let mut items = cache.lock().unwrap();

        match action {
            HistoryAction::SaveImage { img, text } => {
                let now = Local::now();
                let timestamp = now.format("%Y-%m-%d %H:%M:%S").to_string();
                let filename = format!("img_{}.png", now.format("%Y%m%d_%H%M%S_%f"));
                let path = media_dir.join(&filename);
                let id = now.timestamp_nanos_opt().unwrap_or(0);

                if img.save(&path).is_ok() {
                    items.insert(
                        0,
                        HistoryItem {
                            id,
                            timestamp,
                            item_type: HistoryType::Image,
                            text,
                            media_path: filename,
                        },
                    );
                    should_save = true;
                }
            }
            HistoryAction::SaveAudio { wav_data, text } => {
                let now = Local::now();
                let timestamp = now.format("%Y-%m-%d %H:%M:%S").to_string();
                let id = now.timestamp_nanos_opt().unwrap_or(0);

                if wav_data.is_empty() {
                    // Transcript-only audio (e.g. Gemini Live) has no captured WAV.
                    // Store the text entry with no media file so the panel omits the
                    // "Listen" button instead of opening an empty/unplayable file.
                    items.insert(
                        0,
                        HistoryItem {
                            id,
                            timestamp,
                            item_type: HistoryType::Audio,
                            text,
                            media_path: String::new(),
                        },
                    );
                    should_save = true;
                } else {
                    let filename = format!("audio_{}.wav", now.format("%Y%m%d_%H%M%S_%f"));
                    let path = media_dir.join(&filename);
                    if fs::write(&path, wav_data).is_ok() {
                        items.insert(
                            0,
                            HistoryItem {
                                id,
                                timestamp,
                                item_type: HistoryType::Audio,
                                text,
                                media_path: filename,
                            },
                        );
                        should_save = true;
                    }
                }
            }
            HistoryAction::SaveText {
                result_text,
                input_text,
            } => {
                let now = Local::now();
                let timestamp = now.format("%Y-%m-%d %H:%M:%S").to_string();
                let filename = format!("text_{}.txt", now.format("%Y%m%d_%H%M%S_%f"));
                let path = media_dir.join(&filename);
                let id = now.timestamp_nanos_opt().unwrap_or(0);

                if fs::write(&path, &input_text).is_ok() {
                    items.insert(
                        0,
                        HistoryItem {
                            id,
                            timestamp,
                            item_type: HistoryType::Text,
                            text: result_text,
                            media_path: filename,
                        },
                    );
                    should_save = true;
                }
            }
            HistoryAction::Delete { media_path } => {
                // The cache entry was already removed by `HistoryManager::delete`.
                // Here we only clean up the backing file and persist the new state.
                if !media_path.is_empty() {
                    let _ = fs::remove_file(media_dir.join(&media_path));
                }
                should_save = true;
            }
            HistoryAction::ClearAll => {
                if let Ok(entries) = fs::read_dir(&media_dir) {
                    for entry in entries.flatten() {
                        let _ = fs::remove_file(entry.path());
                    }
                }
                items.clear();
                should_save = true;
            }
            HistoryAction::Prune(new_limit) => {
                max_items = new_limit;
                if items.len() > max_items {
                    while items.len() > max_items {
                        if let Some(item) = items.pop() {
                            let _ = fs::remove_file(media_dir.join(item.media_path));
                        }
                    }
                    should_save = true;
                }
            }
        }

        // Handle pruning after saves
        if items.len() > max_items {
            while items.len() > max_items {
                if let Some(item) = items.pop() {
                    let _ = fs::remove_file(media_dir.join(item.media_path));
                }
            }
            should_save = true;
        }

        if should_save {
            save_db(&items);
        }
    }
}
