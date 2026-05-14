//! Shared TTS provider and Kokoro option metadata.

use crate::config::TtsMethod;

#[derive(Clone, Debug)]
pub struct TtsProviderInfo {
    pub method: TtsMethod,
    pub id: &'static str,
    pub label: &'static str,
    pub narration_supported: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct KokoroVoiceOption {
    pub id: &'static str,
    pub label: &'static str,
    pub language_code: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub struct MagpieVoiceOption {
    pub id: &'static str,
    pub label: &'static str,
}

pub const TTS_PROVIDERS: &[TtsProviderInfo] = &[
    TtsProviderInfo {
        method: TtsMethod::GeminiLive,
        id: "GeminiLive",
        label: "Gemini Live",
        narration_supported: true,
    },
    TtsProviderInfo {
        method: TtsMethod::EdgeTTS,
        id: "EdgeTTS",
        label: "Edge TTS",
        narration_supported: true,
    },
    TtsProviderInfo {
        method: TtsMethod::GoogleTranslate,
        id: "GoogleTranslate",
        label: "Google Translate",
        narration_supported: true,
    },
    TtsProviderInfo {
        method: TtsMethod::Kokoro,
        id: "Kokoro",
        label: "Kokoro 82M v1.0",
        narration_supported: true,
    },
    TtsProviderInfo {
        method: TtsMethod::StepAudioEditX,
        id: "StepAudioEditX",
        label: "Step Audio EditX",
        narration_supported: false,
    },
    TtsProviderInfo {
        method: TtsMethod::MagpieMultilingual,
        id: "MagpieMultilingual",
        label: "NVIDIA Magpie-Multilingual 357M",
        narration_supported: true,
    },
    TtsProviderInfo {
        method: TtsMethod::VoxtralTts,
        id: "VoxtralTts",
        label: "Mistral Voxtral 4B TTS",
        narration_supported: false,
    },
];

pub const MAGPIE_VOICES: &[MagpieVoiceOption] = &[
    magpie_voice("John", "John"),
    magpie_voice("Sofia", "Sofia"),
    magpie_voice("Aria", "Aria"),
    magpie_voice("Jason", "Jason"),
    magpie_voice("Leo", "Leo"),
];

pub const MAGPIE_VOICE_LANGUAGES: &[(&str, &str)] = &[
    ("eng", "English"),
    ("spa", "Spanish"),
    ("deu", "German"),
    ("fra", "French"),
    ("vie", "Vietnamese"),
    ("ita", "Italian"),
    ("cmn", "Mandarin Chinese"),
    ("hin", "Hindi"),
    ("jpn", "Japanese"),
];

pub const KOKORO_VOICES: &[KokoroVoiceOption] = &[
    voice("af_alloy", "Alloy", "en-us"),
    voice("af_aoede", "Aoede", "en-us"),
    voice("af_bella", "Bella", "en-us"),
    voice("af_heart", "Heart", "en-us"),
    voice("af_jadzia", "Jadzia", "en-us"),
    voice("af_jessica", "Jessica", "en-us"),
    voice("af_kore", "Kore", "en-us"),
    voice("af_nicole", "Nicole", "en-us"),
    voice("af_nova", "Nova", "en-us"),
    voice("af_river", "River", "en-us"),
    voice("af_sarah", "Sarah", "en-us"),
    voice("af_sky", "Sky", "en-us"),
    voice("am_adam", "Adam", "en-us"),
    voice("am_echo", "Echo", "en-us"),
    voice("am_eric", "Eric", "en-us"),
    voice("am_fenrir", "Fenrir", "en-us"),
    voice("am_liam", "Liam", "en-us"),
    voice("am_michael", "Michael", "en-us"),
    voice("am_onyx", "Onyx", "en-us"),
    voice("am_puck", "Puck", "en-us"),
    voice("am_santa", "Santa", "en-us"),
    voice("bf_alice", "Alice", "en-gb"),
    voice("bf_emma", "Emma", "en-gb"),
    voice("bf_isabella", "Isabella", "en-gb"),
    voice("bf_lily", "Lily", "en-gb"),
    voice("bm_daniel", "Daniel", "en-gb"),
    voice("bm_fable", "Fable", "en-gb"),
    voice("bm_george", "George", "en-gb"),
    voice("bm_lewis", "Lewis", "en-gb"),
    voice("ef_dora", "Dora", "es"),
    voice("em_alex", "Alex", "es"),
    voice("em_santa", "Santa", "es"),
    voice("ff_siwis", "Siwis", "fr"),
    voice("hf_alpha", "Alpha", "hi"),
    voice("hf_beta", "Beta", "hi"),
    voice("hm_omega", "Omega", "hi"),
    voice("hm_psi", "Psi", "hi"),
    voice("if_sara", "Sara", "it"),
    voice("im_nicola", "Nicola", "it"),
    voice("jf_alpha", "Alpha", "ja"),
    voice("jf_gongitsune", "Gongitsune", "ja"),
    voice("jf_nezumi", "Nezumi", "ja"),
    voice("jf_tebukuro", "Tebukuro", "ja"),
    voice("jm_kumo", "Kumo", "ja"),
    voice("pf_dora", "Dora", "pt-br"),
    voice("pm_alex", "Alex", "pt-br"),
    voice("pm_santa", "Santa", "pt-br"),
    voice("zf_xiaobei", "Xiaobei", "zh"),
    voice("zf_xiaoni", "Xiaoni", "zh"),
    voice("zf_xiaoxiao", "Xiaoxiao", "zh"),
    voice("zf_xiaoyi", "Xiaoyi", "zh"),
    voice("zm_yunjian", "Yunjian", "zh"),
    voice("zm_yunxi", "Yunxi", "zh"),
    voice("zm_yunxia", "Yunxia", "zh"),
    voice("zm_yunyang", "Yunyang", "zh"),
];

pub const KOKORO_VOICE_LANGUAGES: &[(&str, &str)] = &[
    ("eng", "English"),
    ("cmn", "Mandarin Chinese"),
    ("jpn", "Japanese"),
    ("spa", "Spanish"),
    ("fra", "French"),
    ("hin", "Hindi"),
    ("ita", "Italian"),
    ("por", "Portuguese"),
];

const fn voice(
    id: &'static str,
    label: &'static str,
    language_code: &'static str,
) -> KokoroVoiceOption {
    KokoroVoiceOption {
        id,
        label,
        language_code,
    }
}

const fn magpie_voice(id: &'static str, label: &'static str) -> MagpieVoiceOption {
    MagpieVoiceOption { id, label }
}

pub fn tts_method_id(method: &TtsMethod) -> &'static str {
    TTS_PROVIDERS
        .iter()
        .find(|provider| &provider.method == method)
        .map(|provider| provider.id)
        .unwrap_or("GeminiLive")
}

pub fn narration_tts_providers() -> impl Iterator<Item = &'static TtsProviderInfo> {
    TTS_PROVIDERS
        .iter()
        .filter(|provider| provider.narration_supported)
}

pub fn magpie_voice_by_id(id: &str) -> Option<&'static MagpieVoiceOption> {
    MAGPIE_VOICES
        .iter()
        .find(|voice| voice.id.eq_ignore_ascii_case(id.trim()))
}

pub fn normalize_magpie_lang(value: &str) -> Option<String> {
    let value = value.trim().to_ascii_lowercase().replace('_', "-");
    if value.is_empty() || value == "auto" {
        return None;
    }
    let normalized = match value.as_str() {
        "eng" | "en" | "en-us" | "en-gb" | "english" => "en",
        "spa" | "es" | "spanish" => "es",
        "deu" | "ger" | "de" | "german" => "de",
        "fra" | "fre" | "fr" | "french" => "fr",
        "vie" | "vi" | "vietnamese" => "vi",
        "ita" | "it" | "italian" => "it",
        "cmn" | "zho" | "chi" | "zh" | "zh-cn" | "mandarin" | "chinese" => "zh",
        "hin" | "hi" | "hindi" => "hi",
        "jpn" | "ja" | "jp" | "japanese" => "ja",
        _ => return None,
    };
    Some(normalized.to_string())
}

pub fn resolve_magpie_voice_for_lang(
    settings: &crate::config::MagpieSettings,
    source_language_code: Option<&str>,
) -> String {
    let Some(target_lang) = source_language_code.and_then(normalize_magpie_lang) else {
        return normalize_magpie_voice(&settings.voice);
    };
    settings
        .voice_configs
        .iter()
        .find(|config| {
            normalize_magpie_lang(&config.language_code).as_deref() == Some(&target_lang)
        })
        .map(|config| normalize_magpie_voice(&config.voice_id))
        .unwrap_or_else(|| default_magpie_voice_for_lang(&target_lang).to_string())
}

pub fn normalize_magpie_voice(value: &str) -> String {
    magpie_voice_by_id(value)
        .map(|voice| voice.id.to_string())
        .unwrap_or_else(|| "John".to_string())
}

pub fn default_magpie_voice_for_lang(lang: &str) -> &'static str {
    match normalize_magpie_lang(lang).as_deref() {
        Some("en") => "Sofia",
        Some("es") => "Sofia",
        Some("de") => "John",
        Some("fr") => "Aria",
        Some("vi") => "Sofia",
        Some("it") => "Leo",
        Some("zh") => "Aria",
        Some("hi") => "Jason",
        Some("ja") => "Sofia",
        _ => "John",
    }
}

pub fn kokoro_voice_by_id(id: &str) -> Option<&'static KokoroVoiceOption> {
    KOKORO_VOICES
        .iter()
        .find(|voice| voice.id.eq_ignore_ascii_case(id.trim()))
}

pub fn default_kokoro_voice_for_lang(lang: &str) -> &'static str {
    match normalize_kokoro_lang(lang).as_deref() {
        Some("en-gb") => "bf_emma",
        Some("zh") => "zf_xiaoxiao",
        Some("ja") => "jf_alpha",
        Some("es") => "ef_dora",
        Some("fr") => "ff_siwis",
        Some("hi") => "hf_alpha",
        Some("it") => "if_sara",
        Some("pt-br") => "pf_dora",
        _ => "af_heart",
    }
}

pub fn kokoro_language_for_voice(voice: &str) -> Option<&'static str> {
    kokoro_voice_by_id(voice).map(|option| option.language_code)
}

pub fn kokoro_voice_language_for_condition(language_code: &str) -> Option<&'static str> {
    match language_code.trim().to_ascii_lowercase().as_str() {
        "eng" | "en" | "en-us" => Some("en-us"),
        "cmn" | "zho" | "zh" => Some("zh"),
        "jpn" | "ja" => Some("ja"),
        "spa" | "es" => Some("es"),
        "fra" | "fre" | "fr" => Some("fr"),
        "hin" | "hi" => Some("hi"),
        "ita" | "it" => Some("it"),
        "por" | "pt" | "pt-br" => Some("pt-br"),
        _ => None,
    }
}

pub fn normalize_kokoro_lang(value: &str) -> Option<String> {
    let value = value.trim().to_ascii_lowercase().replace('_', "-");
    if value.is_empty() || value == "auto" {
        return None;
    }
    let normalized = match value.as_str() {
        "eng" | "en" | "en-us" | "english" | "american english" => "en-us",
        "en-gb" | "en-uk" | "british english" => "en-gb",
        "cmn" | "zho" | "zh" | "zh-cn" | "zh-hans" | "chinese" | "mandarin" => "zh",
        "jpn" | "ja" | "jp" | "japanese" => "ja",
        "spa" | "es" | "es-es" | "spanish" => "es",
        "fra" | "fre" | "fr" | "french" => "fr",
        "hin" | "hi" | "hindi" => "hi",
        "ita" | "it" | "italian" => "it",
        "por" | "pt" | "pt-br" | "portuguese" | "brazilian portuguese" => "pt-br",
        _ => return None,
    };
    Some(normalized.to_string())
}

pub fn resolve_kokoro_lang(
    configured_lang: &str,
    source_language_code: Option<&str>,
    voice: &str,
) -> String {
    normalize_kokoro_lang(configured_lang)
        .or_else(|| source_language_code.and_then(normalize_kokoro_lang))
        .or_else(|| kokoro_language_for_voice(voice).map(str::to_string))
        .unwrap_or_else(|| "en-us".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kokoro_lang_normalization_only_returns_supported_languages() {
        assert_eq!(normalize_kokoro_lang("kor"), None);
        assert_eq!(normalize_kokoro_lang("Vietnamese"), None);
        assert_eq!(normalize_kokoro_lang("jpn").as_deref(), Some("ja"));
        assert_eq!(normalize_kokoro_lang("pt_BR").as_deref(), Some("pt-br"));
    }

    #[test]
    fn magpie_lang_normalization_only_returns_supported_languages() {
        assert_eq!(normalize_magpie_lang("kor"), None);
        assert_eq!(normalize_magpie_lang("vie").as_deref(), Some("vi"));
        assert_eq!(normalize_magpie_lang("jpn").as_deref(), Some("ja"));
        assert_eq!(normalize_magpie_lang("deu").as_deref(), Some("de"));
    }

    #[test]
    fn auto_kokoro_lang_prefers_supported_source_before_voice() {
        assert_eq!(
            resolve_kokoro_lang("", Some("jpn"), "af_heart"),
            "ja".to_string()
        );
        assert_eq!(
            resolve_kokoro_lang("", Some("kor"), "af_heart"),
            "en-us".to_string()
        );
    }
}
