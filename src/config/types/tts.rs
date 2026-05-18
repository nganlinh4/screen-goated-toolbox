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
    StepAudioEditX,     // Step Audio EditX (managed local sidecar)
    MagpieMultilingual, // NVIDIA Magpie-Multilingual 357M (local NIM-style server)
    Kokoro,             // Kokoro 82M v1.0 (Kokoro-FastAPI OpenAI-compat)
    Supertonic,         // Supertonic 3 (local sherpa-onnx)
    VieneuTts,          // VieNeu-TTS v2 (Vietnamese-first local clone TTS)
    VoxtralTts,         // Mistral Voxtral TTS (open weights / La Plateforme)
}

// ============================================================================
// TTS PLAYGROUND SETTINGS
// ============================================================================

/// Independent sandbox settings for the TTS Playground mini app.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct TtsPlaygroundSettings {
    pub mode: TtsPlaygroundMode,
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
    pub supertonic_settings: SupertonicSettings,
    pub vieneu_settings: VieneuSettings,
    pub voxtral_settings: VoxtralSettings,
    pub step_audio_edit_settings: StepAudioEditSettings,
    pub draft_text: String,
}

impl Default for TtsPlaygroundSettings {
    fn default() -> Self {
        Self {
            mode: TtsPlaygroundMode::TtsClone,
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
            supertonic_settings: SupertonicSettings::default(),
            vieneu_settings: VieneuSettings::default(),
            voxtral_settings: VoxtralSettings::default(),
            step_audio_edit_settings: StepAudioEditSettings::default(),
            draft_text: "Write anything here and test how it sounds.".to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub enum TtsPlaygroundMode {
    #[default]
    TtsClone,
    AudioEdit,
    ReferenceLibrary,
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
// OFFLINE OPEN-WEIGHTS TTS PROVIDERS
// ============================================================================
//
// These providers run fully on-device with model weights downloaded from
// Hugging Face (or ModelScope as the gated-region mirror) into the same cache
// directory layout used by Parakeet/Qwen3 ASR. No paid cloud APIs, no Bearer
// tokens — only voice/speed knobs and language hints that the local
// ONNX/Sherpa inference path consumes directly.
//
// Kokoro 82M v1.0 and Supertonic 3 run through sherpa-onnx. Magpie and Step
// Audio EditX run through managed Python sidecars because their public
// checkpoints depend on Python-native inference stacks. Mistral Voxtral is
// still deferred.

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

/// Supertonic 3 — local multilingual TTS via sherpa-onnx OfflineTts.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct SupertonicSettings {
    /// Legacy fallback speaker index inside `voice.bin`.
    pub speaker_id: i32,
    /// Speed multiplier (0.5 – 2.0; 1.0 = natural).
    pub speed: f32,
    /// Generation denoising steps. sherpa-onnx defaults to 5.
    pub num_steps: i32,
    /// Number of CPU threads to give the ONNX runtime.
    pub num_threads: i32,
    /// Legacy fixed ISO 639-1 language code. Empty means detect per request.
    pub lang: String,
    /// Per-language Supertonic voice routing. Language codes are normalized at
    /// runtime, so both ISO 639-1 and app language-detection codes work.
    pub voice_configs: Vec<SupertonicVoiceConfig>,
    /// Silence inserted between internally chunked text segments.
    pub silence_duration: f32,
    /// Deterministic seed. `-1` lets sherpa choose a random seed.
    pub seed: i32,
}

impl Default for SupertonicSettings {
    fn default() -> Self {
        Self {
            speaker_id: 0,
            speed: 1.0,
            num_steps: 5,
            num_threads: 2,
            lang: String::new(),
            voice_configs: default_supertonic_voice_configs(),
            silence_duration: 0.3,
            seed: -1,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SupertonicVoiceConfig {
    pub language_code: String,
    pub language_name: String,
    pub voice_id: String,
}

impl SupertonicVoiceConfig {
    pub fn new(language_code: &str, language_name: &str, voice_id: &str) -> Self {
        Self {
            language_code: language_code.to_string(),
            language_name: language_name.to_string(),
            voice_id: voice_id.to_string(),
        }
    }
}

pub fn default_supertonic_voice_configs() -> Vec<SupertonicVoiceConfig> {
    vec![
        SupertonicVoiceConfig::new("en", "English", "M1"),
        SupertonicVoiceConfig::new("vi", "Vietnamese", "F1"),
        SupertonicVoiceConfig::new("ko", "Korean", "F2"),
        SupertonicVoiceConfig::new("ja", "Japanese", "F3"),
        SupertonicVoiceConfig::new("es", "Spanish", "M2"),
        SupertonicVoiceConfig::new("fr", "French", "F4"),
        SupertonicVoiceConfig::new("pt", "Portuguese", "M3"),
    ]
}

/// VieNeu-TTS v2 — Vietnamese-first local TTS with zero-shot reference cloning.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct VieneuSettings {
    /// Runtime/model variant id from `tts_catalog::VIENEU_VARIANTS`.
    pub variant: String,
    /// SDK emotion preset. Current public SDK exposes `natural` and `storytelling`.
    pub emotion: String,
    /// Selected shared reference voice id. Empty uses the model's default voice.
    pub reference_voice_id: String,
    /// Use ad-hoc reference audio instead of the shared reference library.
    pub use_custom_reference: bool,
    pub reference_audio_path: String,
    /// Exact transcript of the reference audio. Required for standard/fast modes.
    pub reference_text: String,
    pub reference_label: String,
}

impl Default for VieneuSettings {
    fn default() -> Self {
        Self {
            variant: crate::config::tts_catalog::default_vieneu_variant_id().to_string(),
            emotion: "natural".to_string(),
            reference_voice_id: String::new(),
            use_custom_reference: false,
            reference_audio_path: String::new(),
            reference_text: String::new(),
            reference_label: String::new(),
        }
    }
}

/// Step Audio EditX — local managed Python/PyTorch sidecar.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct StepAudioSettings {
    /// Legacy fallback prompt voice id. Normal routing uses language detection
    /// and `voice_configs`.
    pub voice: String,
    /// Per-language prompt voice routing. Language codes use ISO 639-3 to
    /// match the existing app language detection path.
    pub voice_configs: Vec<StepAudioVoiceConfig>,
    /// Selected shared reference voice id. Empty falls back to bundled default
    /// reference audio.
    pub reference_voice_id: String,
    /// Use `reference_audio_path` and `reference_text` instead of bundled
    /// prompt voices.
    pub use_custom_reference: bool,
    /// App-managed or user-selected reference audio path for cloning.
    pub reference_audio_path: String,
    /// Exact transcript of `reference_audio_path`.
    pub reference_text: String,
    /// Optional user-facing label for the reference.
    pub reference_label: String,
    /// Optional reference prompt text override for advanced cloning tests.
    /// Deprecated; kept for config compatibility only.
    pub style_prompt: String,
}

impl Default for StepAudioSettings {
    fn default() -> Self {
        Self {
            voice: String::new(),
            voice_configs: default_step_audio_voice_configs(),
            reference_voice_id: String::new(),
            use_custom_reference: false,
            reference_audio_path: String::new(),
            reference_text: String::new(),
            reference_label: String::new(),
            style_prompt: String::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct StepAudioReferenceVoice {
    pub id: String,
    pub label: String,
    pub audio_path: String,
    pub transcript: String,
}

impl Default for StepAudioReferenceVoice {
    fn default() -> Self {
        Self {
            id: String::new(),
            label: "Reference voice".to_string(),
            audio_path: String::new(),
            transcript: String::new(),
        }
    }
}

impl StepAudioReferenceVoice {
    pub fn new(id: String, label: String) -> Self {
        Self {
            id,
            label,
            ..Self::default()
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct StepAudioEditSettings {
    pub source_audio_path: String,
    pub source_text: String,
    pub target_text: String,
    pub edit_type: String,
    pub edit_info: String,
}

impl Default for StepAudioEditSettings {
    fn default() -> Self {
        Self {
            source_audio_path: String::new(),
            source_text: String::new(),
            target_text: String::new(),
            edit_type: "emotion".to_string(),
            edit_info: "happy".to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StepAudioVoiceConfig {
    pub language_code: String,
    pub language_name: String,
    pub voice_id: String,
}

impl StepAudioVoiceConfig {
    pub fn new(language_code: &str, language_name: &str, voice_id: &str) -> Self {
        Self {
            language_code: language_code.to_string(),
            language_name: language_name.to_string(),
            voice_id: voice_id.to_string(),
        }
    }
}

pub fn default_step_audio_voice_configs() -> Vec<StepAudioVoiceConfig> {
    vec![
        StepAudioVoiceConfig::new("eng", "English", "default_en"),
        StepAudioVoiceConfig::new("cmn", "Mandarin Chinese", "default_zh"),
        StepAudioVoiceConfig::new("yue", "Cantonese", "default_zh"),
        StepAudioVoiceConfig::new("jpn", "Japanese", "default_en"),
        StepAudioVoiceConfig::new("kor", "Korean", "default_en"),
    ]
}

pub const STEP_AUDIO_SUPPORTED_LANGUAGE_HINT: &str = "Step Audio EditX TTS supports English, Mandarin, Sichuanese, Cantonese, Japanese, and Korean. Vietnamese text is not supported by this model.";

pub fn step_audio_tts_text_issue(text: &str) -> Option<&'static str> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }

    for ch in trimmed.chars() {
        if ch.is_ascii() || is_step_audio_supported_cjk(ch) || is_common_non_latin_punctuation(ch) {
            continue;
        }
        if ch.is_alphabetic() || is_combining_mark(ch) {
            return Some(STEP_AUDIO_SUPPORTED_LANGUAGE_HINT);
        }
    }

    None
}

fn is_step_audio_supported_cjk(ch: char) -> bool {
    let code = ch as u32;
    matches!(
        code,
        0x3040..=0x30ff
            | 0x3400..=0x4dbf
            | 0x4e00..=0x9fff
            | 0xac00..=0xd7af
            | 0xf900..=0xfaff
    )
}

fn is_combining_mark(ch: char) -> bool {
    let code = ch as u32;
    matches!(code, 0x0300..=0x036f | 0x1ab0..=0x1aff | 0x1dc0..=0x1dff)
}

fn is_common_non_latin_punctuation(ch: char) -> bool {
    matches!(
        ch,
        '，' | '。'
            | '、'
            | '？'
            | '！'
            | '；'
            | '：'
            | '“'
            | '”'
            | '‘'
            | '’'
            | '（'
            | '）'
            | '《'
            | '》'
            | '「'
            | '」'
            | '『'
            | '』'
            | 'ー'
            | '…'
    )
}

#[cfg(test)]
mod step_audio_tests {
    use super::step_audio_tts_text_issue;

    #[test]
    fn step_audio_text_issue_allows_verified_scripts() {
        assert!(step_audio_tts_text_issue("Hello there.").is_none());
        assert!(step_audio_tts_text_issue("你好，今天怎么样？").is_none());
        assert!(step_audio_tts_text_issue("[Japanese] こんにちは。").is_none());
        assert!(step_audio_tts_text_issue("[Korean] 안녕하세요.").is_none());
    }

    #[test]
    fn step_audio_text_issue_blocks_vietnamese_latin_text() {
        assert!(step_audio_tts_text_issue("Xin chào, tôi muốn đọc văn bản này.").is_some());
    }
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
