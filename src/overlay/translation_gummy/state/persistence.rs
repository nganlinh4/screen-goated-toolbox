use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{TranslationGummyTranscriptItem, prune_transcripts};

#[derive(Serialize, Deserialize)]
struct PersistedTranscript {
    role: String,
    text: String,
    lang: String,
}

fn transcripts_path() -> PathBuf {
    let dir = crate::paths::app_runtime_config_dir();
    let _ = std::fs::create_dir_all(&dir);
    dir.join("translation_gummy_transcripts.json")
}

fn legacy_transcripts_path() -> PathBuf {
    let dir = crate::paths::app_runtime_config_dir();
    let _ = std::fs::create_dir_all(&dir);
    dir.join("bilingual_relay_transcripts.json")
}

fn snapshot(items: &[TranslationGummyTranscriptItem]) -> Vec<PersistedTranscript> {
    items
        .iter()
        .filter(|item| item.is_final)
        .map(|item| PersistedTranscript {
            role: item.role.to_string(),
            text: item.text.clone(),
            lang: item.lang.clone(),
        })
        .collect()
}

fn write_snapshot(path: &Path, items: &[PersistedTranscript]) -> anyhow::Result<()> {
    crate::atomic_json::write_json_atomic(path, &items)?;
    Ok(())
}

pub(in crate::overlay::translation_gummy) fn persist_transcripts() {
    static WRITE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _write = WRITE_LOCK.lock().unwrap();
    let items = {
        let state = super::UI_STATE.lock().unwrap();
        snapshot(&state.transcripts)
    };
    if let Err(error) = write_snapshot(&transcripts_path(), &items) {
        crate::log_info!("Could not save translation history: {error}");
    }
}

pub(super) fn load_persisted_transcripts(limit: usize) -> Vec<TranslationGummyTranscriptItem> {
    let path = if transcripts_path().exists() {
        transcripts_path()
    } else {
        legacy_transcripts_path()
    };
    load_at(&path, limit)
}

fn load_at(path: &Path, limit: usize) -> Vec<TranslationGummyTranscriptItem> {
    let data = match std::fs::read_to_string(path) {
        Ok(data) => data,
        Err(_) => return Vec::new(),
    };
    let items: Vec<PersistedTranscript> = match serde_json::from_str(&data) {
        Ok(items) => items,
        Err(_) => return Vec::new(),
    };
    let mut loaded = items
        .into_iter()
        .map(|item| {
            let role: &'static str = match item.role.as_str() {
                "input" => "input",
                "output" => "output",
                "separator" => "separator",
                _ => "input",
            };
            TranslationGummyTranscriptItem {
                id: super::super::runtime::next_transcript_id(),
                role,
                text: item.text,
                is_final: true,
                lang: item.lang,
            }
        })
        .collect();
    prune_transcripts(&mut loaded, limit);
    loaded
}

#[cfg(test)]
mod tests;
