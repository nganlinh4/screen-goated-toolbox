//! TTS (Text-to-Speech) related configuration types.

use serde::{Deserialize, Serialize};

// ============================================================================
// TTS METHOD
// ============================================================================

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub enum TtsMethod {
    #[default]
    GeminiLive, // Premium (Gemini Live)
    GoogleTranslate,    // Fast (Google Translate)
    EdgeTTS,            // Good (Edge TTS)
    FishAudioS2Pro,     // Deprecated/hidden: removed because S2 Pro needs 24GB+ VRAM
    StepAudioEditX,     // Step Audio EditX (local server)
    MagpieMultilingual, // NVIDIA Magpie-Multilingual 357M (local NIM-style server)
    Kokoro,             // Kokoro 82M v1.0 (Kokoro-FastAPI OpenAI-compat)
    VoxtralTts,         // Mistral Voxtral TTS (open weights / La Plateforme)
}

// ============================================================================
// TTS PLAYGROUND SETTINGS
// ============================================================================

/// Independent sandbox settings for the TTS Playground mini app.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct TtsPlaygroundSettings {
    pub method: TtsMethod,
    pub gemini_model: String,
    pub gemini_voice: String,
    pub gemini_speed: String,
    pub gemini_instruction: String,
    pub gemini_language_conditions: Vec<TtsLanguageCondition>,
    pub google_speed: String,
    pub edge_voice: String,
    pub edge_pitch: i32,
    pub edge_rate: i32,
    pub edge_settings: EdgeTtsSettings,
    pub step_audio_settings: StepAudioSettings,
    pub magpie_settings: MagpieSettings,
    pub kokoro_settings: KokoroSettings,
    pub voxtral_settings: VoxtralSettings,
    pub draft_text: String,
}

impl Default for TtsPlaygroundSettings {
    fn default() -> Self {
        Self {
            method: TtsMethod::GeminiLive,
            gemini_model: crate::model_config::DEFAULT_GEMINI_LIVE_TTS_MODEL.to_string(),
            gemini_voice: "Aoede".to_string(),
            gemini_speed: "Fast".to_string(),
            gemini_instruction: String::new(),
            gemini_language_conditions: default_tts_language_conditions(),
            google_speed: "Normal".to_string(),
            edge_voice: "en-US-AriaNeural".to_string(),
            edge_pitch: 0,
            edge_rate: 0,
            edge_settings: EdgeTtsSettings::default(),
            step_audio_settings: StepAudioSettings::default(),
            magpie_settings: MagpieSettings::default(),
            kokoro_settings: KokoroSettings::default(),
            voxtral_settings: VoxtralSettings::default(),
            draft_text: "Write anything here and test how it sounds.".to_string(),
        }
    }
}

// ============================================================================
// EDGE TTS VOICE CONFIG
// ============================================================================

/// Edge TTS voice configuration for a specific language
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EdgeTtsVoiceConfig {
    /// ISO 639-1 language code (e.g., "en", "vi", "ko")
    pub language_code: String,
    /// Human-readable language name
    pub language_name: String,
    /// Edge TTS voice name (e.g., "en-US-AriaNeural", "vi-VN-HoaiMyNeural")
    pub voice_name: String,
}

impl EdgeTtsVoiceConfig {
    pub fn new(language_code: &str, language_name: &str, voice_name: &str) -> Self {
        Self {
            language_code: language_code.to_string(),
            language_name: language_name.to_string(),
            voice_name: voice_name.to_string(),
        }
    }
}

// ============================================================================
// EDGE TTS SETTINGS
// ============================================================================

/// Edge TTS settings with pitch, rate, volume, and per-language voice configs
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EdgeTtsSettings {
    /// Pitch adjustment (-50 to +50 Hz, 0 = default)
    pub pitch: i32,
    /// Rate adjustment (-50 to +100 percent, 0 = default)
    pub rate: i32,
    /// Volume adjustment (-50 to +50 percent, 0 = default)
    pub volume: i32,
    /// Per-language voice configuration
    pub voice_configs: Vec<EdgeTtsVoiceConfig>,
}

impl Default for EdgeTtsSettings {
    fn default() -> Self {
        Self {
            pitch: 0,
            rate: 0,
            volume: 0,
            voice_configs: default_edge_tts_voice_configs(),
        }
    }
}

/// Default Edge TTS voice configurations for common languages
pub fn default_edge_tts_voice_configs() -> Vec<EdgeTtsVoiceConfig> {
    vec![
        EdgeTtsVoiceConfig::new("en", "English", "en-US-AriaNeural"),
        EdgeTtsVoiceConfig::new("vi", "Vietnamese", "vi-VN-HoaiMyNeural"),
        EdgeTtsVoiceConfig::new("ko", "Korean", "ko-KR-SunHiNeural"),
        EdgeTtsVoiceConfig::new("ja", "Japanese", "ja-JP-NanamiNeural"),
        EdgeTtsVoiceConfig::new("zh", "Chinese", "zh-CN-XiaoxiaoNeural"),
    ]
}

// ============================================================================
// TTS LANGUAGE CONDITION
// ============================================================================

/// A condition for TTS that applies a specific speaking instruction
/// when the detected language matches
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TtsLanguageCondition {
    /// ISO 639-3 language code (e.g., "vie" for Vietnamese, "kor" for Korean)
    pub language_code: String,
    /// Human-readable language name for display
    pub language_name: String,
    /// The speaking instruction to apply when this language is detected
    pub instruction: String,
}

impl TtsLanguageCondition {
    pub fn new(language_code: &str, language_name: &str, instruction: &str) -> Self {
        Self {
            language_code: language_code.to_string(),
            language_name: language_name.to_string(),
            instruction: instruction.to_string(),
        }
    }
}

/// Default TTS language conditions
pub fn default_tts_language_conditions() -> Vec<TtsLanguageCondition> {
    vec![TtsLanguageCondition::new(
        "vie",
        "Vietnamese",
        "Speak in a \"giọng miền Tây\" accent.",
    )]
}

// ============================================================================
// OFFLINE OPEN-WEIGHTS TTS PROVIDERS (Kokoro implemented, others deferred)
// ============================================================================
//
// These providers run fully on-device with model weights downloaded from
// Hugging Face (or ModelScope as the gated-region mirror) into the same cache
// directory layout used by Parakeet/Qwen3 ASR. No paid cloud APIs, no Bearer
// tokens — only voice/speed knobs and language hints that the local
// ONNX/Sherpa inference path consumes directly.
//
// Kokoro 82M v1.0 runs through sherpa-onnx. Magpie runs through a managed
// Python/NeMo sidecar because the upstream checkpoint is a `.nemo` model and
// needs NanoCodec. Step Audio EditX and Mistral Voxtral are still deferred.

/// Kokoro 82M v1.0 — runs locally via sherpa-onnx OfflineTts using model
/// files downloaded into `dirs::data_dir()/screen-goated-toolbox/models/kokoro/`.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct KokoroSettings {
    /// Voice id baked into the Kokoro voices.bin (e.g. `af_heart`, `am_adam`,
    /// `bf_emma`, `jf_alpha`). Empty string falls back to `af_heart`.
    pub voice: String,
    /// Speed multiplier (0.5 – 2.0; 1.0 = natural).
    pub speed: f32,
    /// BCP-47 language hint for phonemizer routing inside sherpa-onnx
    /// (e.g. `en-us`, `ja`, `zh`, `es`). Empty string lets the model auto-detect.
    pub lang: String,
    /// Number of CPU threads to give the ONNX runtime. 2 is the sherpa-onnx
    /// default and works well for Kokoro on consumer CPUs.
    pub num_threads: i32,
    /// Per-language Kokoro voice routing. Language codes use ISO 639-3 to
    /// match the existing app language detection path.
    pub voice_configs: Vec<KokoroVoiceConfig>,
}

impl Default for KokoroSettings {
    fn default() -> Self {
        Self {
            voice: "af_heart".to_string(),
            speed: 1.0,
            lang: String::new(),
            num_threads: 2,
            voice_configs: default_kokoro_voice_configs(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KokoroVoiceConfig {
    pub language_code: String,
    pub language_name: String,
    pub voice_id: String,
}

impl KokoroVoiceConfig {
    pub fn new(language_code: &str, language_name: &str, voice_id: &str) -> Self {
        Self {
            language_code: language_code.to_string(),
            language_name: language_name.to_string(),
            voice_id: voice_id.to_string(),
        }
    }
}

pub fn default_kokoro_voice_configs() -> Vec<KokoroVoiceConfig> {
    vec![
        KokoroVoiceConfig::new("eng", "English", "af_heart"),
        KokoroVoiceConfig::new("cmn", "Mandarin Chinese", "zf_xiaoxiao"),
        KokoroVoiceConfig::new("jpn", "Japanese", "jf_alpha"),
        KokoroVoiceConfig::new("spa", "Spanish", "ef_dora"),
        KokoroVoiceConfig::new("fra", "French", "ff_siwis"),
        KokoroVoiceConfig::new("hin", "Hindi", "hf_alpha"),
        KokoroVoiceConfig::new("ita", "Italian", "if_sara"),
        KokoroVoiceConfig::new("por", "Portuguese", "pf_dora"),
    ]
}

/// Step Audio EditX — deferred offline. The 3B-param checkpoint ships as
/// PyTorch only; no community ONNX export, no sherpa-onnx support yet.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct StepAudioSettings {
    /// Speaker id reserved for the future offline worker.
    pub voice: String,
    /// Edit / style prompt reserved for the future offline worker.
    pub style_prompt: String,
}

/// NVIDIA Magpie-Multilingual 357M — local managed Python/NeMo sidecar.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct MagpieSettings {
    /// Legacy fallback speaker id. Normal routing uses language detection and
    /// `voice_configs`.
    pub voice: String,
    /// Per-language Magpie speaker routing. Language codes use ISO 639-3 to
    /// match the existing app language detection path.
    pub voice_configs: Vec<MagpieVoiceConfig>,
}

impl Default for MagpieSettings {
    fn default() -> Self {
        Self {
            voice: String::new(),
            voice_configs: default_magpie_voice_configs(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MagpieVoiceConfig {
    pub language_code: String,
    pub language_name: String,
    pub voice_id: String,
}

impl MagpieVoiceConfig {
    pub fn new(language_code: &str, language_name: &str, voice_id: &str) -> Self {
        Self {
            language_code: language_code.to_string(),
            language_name: language_name.to_string(),
            voice_id: voice_id.to_string(),
        }
    }
}

pub fn default_magpie_voice_configs() -> Vec<MagpieVoiceConfig> {
    vec![
        MagpieVoiceConfig::new("eng", "English", "Sofia"),
        MagpieVoiceConfig::new("spa", "Spanish", "Sofia"),
        MagpieVoiceConfig::new("deu", "German", "John"),
        MagpieVoiceConfig::new("fra", "French", "Aria"),
        MagpieVoiceConfig::new("vie", "Vietnamese", "Sofia"),
        MagpieVoiceConfig::new("ita", "Italian", "Leo"),
        MagpieVoiceConfig::new("cmn", "Mandarin Chinese", "Aria"),
        MagpieVoiceConfig::new("hin", "Hindi", "Jason"),
        MagpieVoiceConfig::new("jpn", "Japanese", "Sofia"),
    ]
}

/// Mistral Voxtral TTS — deferred offline. The 4B-param open-weights checkpoint
/// requires Mistral's reference PyTorch runtime; no ONNX bindings yet.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct VoxtralSettings {
    /// Voice id reserved for the future offline worker.
    pub voice: String,
}
