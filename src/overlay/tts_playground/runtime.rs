//! Shared state + facade for the WRY TTS Playground runtime.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, LazyLock, Mutex};

use crate::api::tts::types::TtsCollectedAudio;
use crate::config::TtsMethod;
use crate::gui::locale::LocaleText;

use super::state::{self, CurrentClip, RecentClip};

pub(super) const MIN_RECENT_LIMIT: usize = 5;
pub(super) const MAX_RECENT_LIMIT: usize = 50;

pub(super) static CANCEL_FLAG: LazyLock<Mutex<Option<Arc<AtomicBool>>>> =
    LazyLock::new(|| Mutex::new(None));
type ClipCacheEntry = (String, Arc<TtsCollectedAudio>);

pub(super) static CLIP_CACHE: LazyLock<Mutex<Vec<ClipCacheEntry>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));
pub(super) static CLIP_METHODS: LazyLock<Mutex<Vec<(String, TtsMethod)>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));
pub(super) static PLAYBACK_TIMER: LazyLock<Mutex<Option<PlaybackTimer>>> =
    LazyLock::new(|| Mutex::new(None));
pub(super) static MIC_SESSION: LazyLock<Mutex<Option<MicSession>>> =
    LazyLock::new(|| Mutex::new(None));

pub(super) struct MicSession {
    pub(super) samples: Arc<Mutex<Vec<i16>>>,
    pub(super) stop_flag: Arc<AtomicBool>,
    pub(super) _stream: cpal::Stream,
}

// SAFETY: cpal marks every Stream !Send for portability. On Windows (WASAPI) the
// stream is a handle to its own audio thread, and dropping it from another
// thread only signals and joins that thread. MicSession lives in a Mutex, so
// it never needs Sync.
unsafe impl Send for MicSession {}

#[derive(Clone)]
pub(super) struct PlaybackTimer {
    pub(super) progress: Arc<crate::api::tts::manager::PlaybackProgress>,
    pub(super) start_sample: usize,
    pub(super) sample_rate: u32,
    pub(super) total_samples: usize,
}

pub(super) use super::runtime_clips::{delete_recent, download_wav, play_recent, start_mp3_export};
pub(super) use super::runtime_generation::{cancel_generation, clear_current, start_generation};
pub(super) use super::runtime_playback::{pause, play, replay, seek, stop, tick_position};
pub(super) use super::runtime_sources::{
    pick_source_audio, preview_voice, reset_provider, start_mic_recording, stop_mic_recording,
};

pub(super) fn next_clip_id() -> Result<String, String> {
    let mut random = [0_u8; 16];
    getrandom::fill(&mut random).map_err(|_| "Could not create a clip identity.".to_string())?;
    Ok(format!("clip-{}", u128::from_le_bytes(random)))
}

pub(super) fn recent_limit() -> usize {
    crate::APP
        .lock()
        .map(|app| {
            app.config
                .tts_playground
                .recent_clip_limit
                .clamp(MIN_RECENT_LIMIT, MAX_RECENT_LIMIT)
        })
        .unwrap_or(10)
}

pub(super) fn cache_put(id: String, audio: TtsCollectedAudio) {
    let limit = recent_limit();
    let mut cache = CLIP_CACHE.lock().unwrap();
    cache.retain(|(existing, _)| existing != &id);
    cache.insert(0, (id, Arc::new(audio)));
    while cache.len() > limit {
        cache.pop();
    }
}

pub(super) fn cache_method(id: String, method: TtsMethod) {
    let limit = recent_limit();
    let mut methods = CLIP_METHODS.lock().unwrap();
    methods.retain(|(existing, _)| existing != &id);
    methods.insert(0, (id, method));
    while methods.len() > limit {
        methods.pop();
    }
}

pub(super) fn cache_get(id: &str) -> Option<Arc<TtsCollectedAudio>> {
    let cache = CLIP_CACHE.lock().unwrap();
    cache.iter().find(|(k, _)| k == id).map(|(_, a)| a.clone())
}

pub(super) fn cache_remove(id: &str) {
    let mut cache = CLIP_CACHE.lock().unwrap();
    cache.retain(|(k, _)| k != id);
    CLIP_METHODS.lock().unwrap().retain(|(k, _)| k != id);
}

pub(super) fn hydrate_recent_once() {
    let already_loaded = state::with_state(|s| !s.recent.is_empty() || s.current.is_some());
    if already_loaded {
        return;
    }
    let retained = crate::APP.lock().ok().map(|app| {
        app.config
            .tts_playground
            .source_audio
            .source_audio_path
            .clone()
    });
    if let Some(retained) = retained {
        super::library::prune_managed_audio(&[retained]);
    }
    let loaded = super::library::load_recent(recent_limit());
    if loaded.is_empty() {
        // Reconcile app-owned clip files even when the index is empty or every
        // indexed WAV is unreadable; otherwise those files can accumulate forever.
        super::library::save_recent(&[], &[]);
        return;
    }
    let mut recent = Vec::new();
    for (clip, audio) in loaded {
        cache_put(clip.id.clone(), audio);
        recent.push(RecentClip {
            id: clip.id.clone(),
            text: clip.text.clone(),
            voice_label: clip.voice_label.clone(),
            created_label: clip.created_label.clone(),
            duration_sec: clip.duration_sec,
        });
    }
    let current = recent.first().cloned().map(|clip| CurrentClip {
        id: clip.id,
        text: clip.text,
        voice_label: clip.voice_label,
        created_label: clip.created_label,
        duration_sec: clip.duration_sec,
        sample_rate: 24_000,
    });
    state::with_state(|s| {
        s.current = current;
        s.recent = recent;
    });
    persist_recent();
}

pub(super) fn persist_recent() {
    let recent = state::with_state(|s| s.recent.clone());
    let cache = CLIP_CACHE.lock().unwrap();
    let methods = CLIP_METHODS.lock().unwrap();
    let clips = recent
        .iter()
        .filter_map(|clip| {
            let audio = cache
                .iter()
                .find(|(id, _)| id == &clip.id)
                .map(|(_, audio)| audio.clone())?;
            Some((clip.clone(), audio))
        })
        .collect::<Vec<_>>();
    super::library::save_recent(&clips, &methods);
}

pub(super) fn set_recent_limit(limit: usize) {
    let limit = limit.clamp(MIN_RECENT_LIMIT, MAX_RECENT_LIMIT);
    if let Ok(mut app) = crate::APP.lock() {
        app.config.tts_playground.recent_clip_limit = limit;
        crate::config::save_config(&app.config);
    }
    trim_recent(limit);
    persist_recent();
    state::sync_to_webview();
}

pub(super) fn clear_recent() {
    let ids = state::with_state(|s| {
        let ids = s
            .recent
            .iter()
            .map(|clip| clip.id.clone())
            .collect::<Vec<_>>();
        s.recent.clear();
        s.current = None;
        s.position_sec = 0.0;
        s.is_playing = false;
        ids
    });
    for id in ids {
        cache_remove(&id);
    }
    super::runtime_playback::stop_owned_playback();
    *PLAYBACK_TIMER.lock().unwrap() = None;
    persist_recent();
    state::sync_to_webview();
}

fn trim_recent(limit: usize) {
    let removed = state::with_state(|s| {
        let start = limit.min(s.recent.len());
        let removed = s
            .recent
            .drain(start..)
            .map(|clip| clip.id)
            .collect::<Vec<_>>();
        if let Some(current) = &s.current
            && removed.contains(&current.id)
        {
            s.current = None;
        }
        removed
    });
    for id in removed {
        cache_remove(&id);
    }
}

pub(super) fn describe_voice(method: TtsMethod) -> String {
    let Ok(app) = crate::APP.lock() else {
        return "TTS".to_string();
    };
    let pg = &app.config.tts_playground;
    match method {
        TtsMethod::GeminiLive => pg.gemini_voice.clone(),
        TtsMethod::EdgeTTS => pg.edge_voice.clone(),
        TtsMethod::GoogleTranslate => "Google Translate".to_string(),
    }
}

pub(super) fn sanitize(label: &str) -> String {
    label
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect()
}

pub(super) fn random_preview_text(text: &LocaleText, speaker_name: &str) -> String {
    text.tts_settings
        .tts_preview_texts
        .first()
        .map(|template| template.replace("{}", speaker_name))
        .unwrap_or_else(|| format!("Hello, I am {speaker_name}. This is a voice preview."))
}

#[cfg(test)]
mod clip_id_tests {
    #[test]
    fn generated_clip_ids_are_distinct_and_supported_by_the_clip_store() {
        let mut ids = std::collections::HashSet::new();
        for _ in 0..1000 {
            let id = super::next_clip_id().unwrap();
            assert!(id.strip_prefix("clip-").unwrap().parse::<u128>().is_ok());
            assert!(ids.insert(id));
        }
    }
}
