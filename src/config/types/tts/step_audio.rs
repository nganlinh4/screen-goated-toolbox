use serde::{Deserialize, Serialize};

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
mod tests {
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
