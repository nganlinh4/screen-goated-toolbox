use rusqlite::{params, Connection, Result};
use chrono::Local;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{channel, Sender, Receiver};
use std::thread;
use image::{ImageBuffer, Rgba};
use std::fs;

#[derive(Clone, Debug)]
pub enum HistoryType {
    Image,
    Audio,
}

impl ToString for HistoryType {
    fn to_string(&self) -> String {
        match self {
            HistoryType::Image => "image".to_string(),
            HistoryType::Audio => "audio".to_string(),
        }
    }
}

impl From<String> for HistoryType {
    fn from(s: String) -> Self {
        if s == "audio" { HistoryType::Audio } else { HistoryType::Image }
    }
}

#[derive(Clone, Debug)]
pub struct HistoryItem {
    pub id: i64,
    pub timestamp: String,
    pub item_type: HistoryType,
    pub text: String,
    pub media_path: String, // Relative filename
}

pub enum HistoryAction {
    SaveImage { img: ImageBuffer<Rgba<u8>, Vec<u8>>, text: String },
    SaveAudio { wav_data: Vec<u8>, text: String },
    Delete(i64),
    ClearAll, // NEW: Clear everything
    Prune(usize),
}

pub struct HistoryManager {
    tx: Sender<HistoryAction>,
    pub items: Arc<Mutex<Vec<HistoryItem>>>, // In-memory cache for UI
}

impl HistoryManager {
    pub fn new(max_items: usize) -> Self {
        let (tx, rx) = channel();
        let items = Arc::new(Mutex::new(Vec::new()));
        let items_clone = items.clone();

        // Initialize loading in main thread to populate UI immediately on start
        let initial_items = load_initial_items().unwrap_or_default();
        *items.lock().unwrap() = initial_items;

        thread::spawn(move || {
            let conn = setup_db().expect("Failed to setup DB");
            process_queue(conn, rx, items_clone, max_items);
        });

        Self { tx, items }
    }

    pub fn save_image(&self, img: ImageBuffer<Rgba<u8>, Vec<u8>>, text: String) {
        let _ = self.tx.send(HistoryAction::SaveImage { img, text });
    }

    pub fn save_audio(&self, wav_data: Vec<u8>, text: String) {
        let _ = self.tx.send(HistoryAction::SaveAudio { wav_data, text });
    }

    pub fn delete(&self, id: i64) {
        let _ = self.tx.send(HistoryAction::Delete(id));
        // Optimistically remove from cache
        let mut guard = self.items.lock().unwrap();
        if let Some(pos) = guard.iter().position(|x| x.id == id) {
            guard.remove(pos);
        }
    }

    // NEW: Public method to clear all
    pub fn clear_all(&self) {
        let _ = self.tx.send(HistoryAction::ClearAll);
        // Clear cache immediately
        let mut guard = self.items.lock().unwrap();
        guard.clear();
    }

    pub fn request_prune(&self, limit: usize) {
        let _ = self.tx.send(HistoryAction::Prune(limit));
    }
}

fn get_dirs() -> (PathBuf, PathBuf) {
    let config_dir = dirs::config_dir().unwrap_or_default().join("screen-grounded-translator");
    let media_dir = config_dir.join("history_media");
    let _ = fs::create_dir_all(&media_dir);
    (config_dir, media_dir)
}

fn setup_db() -> Result<Connection> {
    let (config_dir, _) = get_dirs();
    let db_path = config_dir.join("history.db");
    let conn = Connection::open(db_path)?;
    
    conn.execute(
        "CREATE TABLE IF NOT EXISTS history (
            id INTEGER PRIMARY KEY,
            timestamp TEXT NOT NULL,
            item_type TEXT NOT NULL,
            text TEXT NOT NULL,
            media_path TEXT NOT NULL
        )",
        [],
    )?;
    Ok(conn)
}

fn load_initial_items() -> Result<Vec<HistoryItem>> {
    let conn = setup_db()?;
    let mut stmt = conn.prepare("SELECT id, timestamp, item_type, text, media_path FROM history ORDER BY id DESC")?;
    let rows = stmt.query_map([], |row| {
        let type_str: String = row.get(2)?;
        Ok(HistoryItem {
            id: row.get(0)?,
            timestamp: row.get(1)?,
            item_type: HistoryType::from(type_str),
            text: row.get(3)?,
            media_path: row.get(4)?,
        })
    })?;

    let mut items = Vec::new();
    for row in rows {
        if let Ok(item) = row {
            items.push(item);
        }
    }
    Ok(items)
}

fn process_queue(
    conn: Connection, 
    rx: Receiver<HistoryAction>, 
    cache: Arc<Mutex<Vec<HistoryItem>>>,
    mut max_items: usize
) {
    let conn = conn;
    let (_, media_dir) = get_dirs();

    while let Ok(action) = rx.recv() {
        match action {
            HistoryAction::SaveImage { img, text } => {
                let now = Local::now();
                let timestamp = now.format("%Y-%m-%d %H:%M:%S").to_string();
                let filename = format!("img_{}.png", now.format("%Y%m%d_%H%M%S_%f"));
                let path = media_dir.join(&filename);
                
                // Heavy IO
                if img.save(&path).is_ok() {
                    let type_str = "image".to_string();
                    let _ = conn.execute(
                        "INSERT INTO history (timestamp, item_type, text, media_path) VALUES (?1, ?2, ?3, ?4)",
                        params![timestamp, type_str, text, filename],
                    );
                    
                    let id = conn.last_insert_rowid();
                    
                    // Update Cache
                    let mut guard = cache.lock().unwrap();
                    guard.insert(0, HistoryItem {
                        id,
                        timestamp,
                        item_type: HistoryType::Image,
                        text,
                        media_path: filename,
                    });
                }
                
                prune_db(&conn, &cache, &media_dir, max_items);
            },
            HistoryAction::SaveAudio { wav_data, text } => {
                let now = Local::now();
                let timestamp = now.format("%Y-%m-%d %H:%M:%S").to_string();
                let filename = format!("audio_{}.wav", now.format("%Y%m%d_%H%M%S_%f"));
                let path = media_dir.join(&filename);
                
                if fs::write(&path, wav_data).is_ok() {
                    let type_str = "audio".to_string();
                    let _ = conn.execute(
                        "INSERT INTO history (timestamp, item_type, text, media_path) VALUES (?1, ?2, ?3, ?4)",
                        params![timestamp, type_str, text, filename],
                    );
                    
                    let id = conn.last_insert_rowid();
                    
                    let mut guard = cache.lock().unwrap();
                    guard.insert(0, HistoryItem {
                        id,
                        timestamp,
                        item_type: HistoryType::Audio,
                        text,
                        media_path: filename,
                    });
                }

                prune_db(&conn, &cache, &media_dir, max_items);
            },
            HistoryAction::Delete(id) => {
                let filename: Result<String> = conn.query_row(
                    "SELECT media_path FROM history WHERE id = ?1",
                    params![id],
                    |row| row.get(0)
                );
                
                if let Ok(f) = filename {
                    let _ = fs::remove_file(media_dir.join(f));
                }
                
                let _ = conn.execute("DELETE FROM history WHERE id = ?1", params![id]);
            },
            // NEW: Implementation of ClearAll
            HistoryAction::ClearAll => {
                // 1. Delete all files in media directory
                if let Ok(entries) = fs::read_dir(&media_dir) {
                    for entry in entries.flatten() {
                        let _ = fs::remove_file(entry.path());
                    }
                }
                
                // 2. Clear Database
                let _ = conn.execute("DELETE FROM history", []);
                
                // 3. Sync cache (just in case)
                let mut guard = cache.lock().unwrap();
                guard.clear();
            },
            HistoryAction::Prune(new_limit) => {
                max_items = new_limit;
                prune_db(&conn, &cache, &media_dir, max_items);
            }
        }
    }
}

fn prune_db(conn: &Connection, cache: &Arc<Mutex<Vec<HistoryItem>>>, media_dir: &Path, limit: usize) {
    if limit == 0 { return; }
    
    // Check count
    let count: Result<usize> = conn.query_row(
        "SELECT COUNT(*) FROM history",
        [],
        |row| row.get(0)
    );
    
    if let Ok(c) = count {
        if c > limit {
            let overflow = c - limit;
            // Get IDs and files to delete
            let mut stmt = conn.prepare("SELECT id, media_path FROM history ORDER BY id ASC LIMIT ?1").unwrap();
            let rows = stmt.query_map(params![overflow], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            }).unwrap();
            
            let mut ids_to_del = Vec::new();
            for r in rows {
                if let Ok((id, fname)) = r {
                    ids_to_del.push(id);
                    let _ = fs::remove_file(media_dir.join(fname));
                }
            }
            
            // Execute Delete
            if !ids_to_del.is_empty() {
                // Simple loop to avoid complex IN clause construction
                for id in &ids_to_del {
                    let _ = conn.execute("DELETE FROM history WHERE id = ?1", params![id]);
                }
                
                // Sync Cache
                let mut guard = cache.lock().unwrap();
                guard.retain(|x| !ids_to_del.contains(&x.id));
            }
        }
    }
}
