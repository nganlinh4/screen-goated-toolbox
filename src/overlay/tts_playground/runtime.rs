//! Shared state + facade for the WRY TTS Playground runtime.

use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Instant;

use crate::api::tts::types::TtsCollectedAudio;
use crate::config::TtsMethod;
use crate::gui::locale::LocaleText;

use super::state::{self, CurrentClip, RecentClip};

pub(super) const RECENT_LIMIT: usize = 5;

pub(super) static ID_GEN: LazyLock<AtomicU64> = LazyLock::new(|| AtomicU64::new(0));
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
pub(super) static REFERENCE_MIC_TARGET: LazyLock<Mutex<Option<String>>> =
    LazyLock::new(|| Mutex::new(None));

pub(super) struct MicSession {
    pub(super) samples: Arc<Mutex<Vec<i16>>>,
    pub(super) stop_flag: Arc<AtomicBool>,
    pub(super) _stream: cpal::Stream,
}

unsafe impl Send for MicSession {}
unsafe impl Sync for MicSession {}

#[derive(Clone)]
pub(super) struct PlaybackTimer {
    pub(super) start: Instant,
    pub(super) start_sample: usize,
    pub(super) sample_rate: u32,
    pub(super) total_samples: usize,
}

pub(super) use super::runtime_clips::{delete_recent, download_wav, play_recent, start_mp3_export};
pub(super) use super::runtime_generation::{cancel_generation, clear_current, start_generation};
pub(super) use super::runtime_playback::{pause, play, replay, seek, stop, tick_position};
pub(super) use super::runtime_sources::{
    add_reference, delete_reference, pick_reference_audio, pick_source_audio, play_reference,
    preview_voice, recognize_reference, reset_provider, start_mic_recording, start_reference_mic,
    stop_mic_recording, stop_reference_mic, update_reference, use_current_as_source, use_reference,
};

pub(super) fn next_clip_id() -> String {
    let id = ID_GEN.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
    format!("clip-{id}")
}

pub(super) fn cache_put(id: String, audio: TtsCollectedAudio) {
    let mut cache = CLIP_CACHE.lock().unwrap();
    cache.retain(|(existing, _)| existing != &id);
    cache.insert(0, (id, Arc::new(audio)));
    while cache.len() > RECENT_LIMIT {
        cache.pop();
    }
}

pub(super) fn cache_method(id: String, method: TtsMethod) {
    let mut methods = CLIP_METHODS.lock().unwrap();
    methods.retain(|(existing, _)| existing != &id);
    methods.insert(0, (id, method));
    while methods.len() > RECENT_LIMIT {
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
    let loaded = super::library::load_recent();
    if loaded.is_empty() {
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

pub(super) fn describe_voice(method: TtsMethod) -> String {
    let Ok(app) = crate::APP.lock() else {
        return "TTS".to_string();
    };
    let pg = &app.config.tts_playground;
    match method {
        TtsMethod::GeminiLive => pg.gemini_voice.clone(),
        TtsMethod::EdgeTTS => pg.edge_voice.clone(),
        TtsMethod::GoogleTranslate => "Google Translate".to_string(),
        TtsMethod::StepAudioEditX => pg
            .step_audio_settings
            .reference_label
            .clone()
            .or_default()
            .unwrap_or_else(|| pg.step_audio_settings.voice.clone()),
        TtsMethod::MagpieMultilingual => pg.magpie_settings.voice.clone(),
        TtsMethod::Kokoro => pg.kokoro_settings.voice.clone(),
        TtsMethod::Supertonic => format!("Supertonic spk{}", pg.supertonic_settings.speaker_id),
        TtsMethod::VieneuTts => pg
            .vieneu_settings
            .reference_label
            .clone()
            .or_default()
            .unwrap_or_else(|| pg.vieneu_settings.variant.clone()),
        _ => "TTS".to_string(),
    }
}

pub(super) fn sanitize(label: &str) -> String {
    label
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect()
}

pub(super) fn random_preview_text(text: &LocaleText, speaker_name: &str) -> String {
    text.tts_preview_texts
        .first()
        .map(|template| template.replace("{}", speaker_name))
        .unwrap_or_else(|| format!("Hello, I am {speaker_name}. This is a voice preview."))
}

pub(super) fn reference_transcription_preset() -> crate::config::Preset {
    if let Ok(app) = crate::APP.lock() {
        if let Some(preset) = app
            .config
            .presets
            .get(app.config.active_preset_idx)
            .filter(|preset| preset.blocks.iter().any(is_audio_processing_block))
            .cloned()
        {
            return preset;
        }
        if let Some(preset) = app
            .config
            .presets
            .iter()
            .find(|preset| preset.blocks.iter().any(is_audio_processing_block))
            .cloned()
        {
            return preset;
        }
    }
    crate::config::Preset {
        id: "step_audio_reference_transcribe".to_string(),
        name: "Step Audio reference transcript".to_string(),
        blocks: vec![crate::config::ProcessingBlock {
            block_type: "audio".to_string(),
            model: crate::model_config::PRESET_AUDIO_TRANSCRIBE_MODEL_ID.to_string(),
            prompt: "Transcribe the audio exactly. Output ONLY the transcript.".to_string(),
            selected_language: "Auto".to_string(),
            show_overlay: false,
            streaming_enabled: false,
            ..crate::config::ProcessingBlock::default()
        }],
        preset_type: "audio".to_string(),
        audio_source: "mic".to_string(),
        audio_processing_mode: "record_then_process".to_string(),
        ..crate::config::Preset::default()
    }
}

fn is_audio_processing_block(block: &crate::config::ProcessingBlock) -> bool {
    block.block_type == "audio"
}

trait StringOrDefault {
    fn or_default(self) -> Option<String>;
}

impl StringOrDefault for String {
    fn or_default(self) -> Option<String> {
        if self.trim().is_empty() {
            None
        } else {
            Some(self)
        }
    }
}
