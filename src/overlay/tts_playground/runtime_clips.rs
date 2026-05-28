use crate::api::tts::TTS_MANAGER;

use super::file_dialogs;
use super::runtime::{PLAYBACK_TIMER, cache_get, cache_remove, persist_recent, sanitize};
use super::runtime_playback::start_pcm_playback;
use super::state::{self, CurrentClip};

pub(super) fn download_wav() -> Result<Option<String>, String> {
    let Some(current) = state::with_state(|s| s.current.clone()) else {
        return Ok(None);
    };
    let Some(audio) = cache_get(&current.id) else {
        return Err("Current clip has no cached audio".to_string());
    };
    let default = format!("tts-playground-{}.wav", sanitize(&current.voice_label));
    match file_dialogs::save_wav(&default, &audio.wav_data) {
        Ok(path) => {
            let path_str = path.display().to_string();
            state::with_state(|s| {
                s.status = format!("Saved {path_str}");
            });
            state::sync_to_webview();
            Ok(Some(path_str))
        }
        Err(err) if err == "Save cancelled" => Ok(None),
        Err(err) => Err(err),
    }
}

pub(super) fn start_mp3_export() {
    let Some(current) = state::with_state(|s| s.current.clone()) else {
        return;
    };
    let Some(audio) = cache_get(&current.id) else {
        return;
    };
    let id_seq: u64 = current
        .id
        .strip_prefix("clip-")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let default = format!("tts-playground-{}.mp3", sanitize(&current.voice_label));
    state::with_state(|s| {
        s.is_exporting = true;
    });
    state::sync_to_webview();
    std::thread::spawn(move || {
        let wav_bytes = audio.wav_data.clone();
        match file_dialogs::save_mp3(&default, &wav_bytes, id_seq) {
            Ok(path) => state::with_state(|s| {
                s.is_exporting = false;
                s.status = format!("Saved {}", path.display());
            }),
            Err(err) if err == "Save cancelled" => state::with_state(|s| {
                s.is_exporting = false;
            }),
            Err(err) => state::with_state(|s| {
                s.is_exporting = false;
                s.error = Some(err);
            }),
        }
        state::sync_to_webview();
    });
}

// ============================================================================
// Recent clips
// ============================================================================

pub(super) fn play_recent(id: &str) {
    let Some(audio) = cache_get(id) else {
        state::with_state(|s| {
            s.status = "Clip no longer cached".to_string();
        });
        state::sync_to_webview();
        return;
    };
    let recent = state::with_state(|s| s.recent.iter().find(|c| c.id == id).cloned());
    let Some(recent) = recent else { return };
    let current = CurrentClip {
        id: recent.id.clone(),
        text: recent.text.clone(),
        voice_label: recent.voice_label.clone(),
        created_label: recent.created_label.clone(),
        duration_sec: recent.duration_sec,
        sample_rate: audio.sample_rate,
    };
    state::with_state(|s| {
        s.current = Some(current);
        s.is_playing = true;
        s.paused = false;
        s.position_sec = 0.0;
    });
    let total = audio.pcm_samples.len();
    start_pcm_playback(audio.pcm_samples.clone(), audio.sample_rate, total, 0);
    state::sync_to_webview();
}

pub(super) fn delete_recent(id: &str) {
    cache_remove(id);
    state::with_state(|s| {
        s.recent.retain(|clip| clip.id != id);
        if s.current.as_ref().map(|c| c.id.as_str()) == Some(id) {
            s.current = None;
            s.position_sec = 0.0;
            s.is_playing = false;
            TTS_MANAGER.stop();
            *PLAYBACK_TIMER.lock().unwrap() = None;
        }
    });
    persist_recent();
    state::sync_to_webview();
}
