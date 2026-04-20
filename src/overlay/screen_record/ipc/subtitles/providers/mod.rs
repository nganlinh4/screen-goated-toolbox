mod groq;
mod qwen;

use crate::api::realtime_audio::qwen3::{Qwen3ModelVariant, assets, reference};
use crate::runtime_support::{RuntimeArch, environment_info};

use super::types::{CompactSubtitleSegment, SubtitleGenerationMethod, SubtitleMethodCapability};

pub struct SubtitleBackendProgress {
    pub completed_steps: usize,
    pub total_steps: usize,
    pub segments: Vec<CompactSubtitleSegment>,
}

pub trait SubtitleBackend {
    fn transcribe_clip(
        &mut self,
        audio_data: Vec<u8>,
        language_hint: Option<&str>,
        on_progress: &mut dyn FnMut(SubtitleBackendProgress) -> Result<(), String>,
    ) -> Result<Vec<CompactSubtitleSegment>, String>;
}

pub fn create_backend(
    method: SubtitleGenerationMethod,
) -> Result<Box<dyn SubtitleBackend + Send>, String> {
    match method {
        SubtitleGenerationMethod::GroqWhisperAccurate => {
            Ok(Box::new(groq::GroqSubtitleBackend::new()?))
        }
        SubtitleGenerationMethod::QwenLocal0_6B => {
            Ok(Box::new(qwen::QwenSubtitleBackend::new(Qwen3ModelVariant::Small)?))
        }
        SubtitleGenerationMethod::QwenLocal1_7B => {
            Ok(Box::new(qwen::QwenSubtitleBackend::new(Qwen3ModelVariant::Large)?))
        }
    }
}

pub fn capabilities() -> Vec<SubtitleMethodCapability> {
    vec![
        SubtitleMethodCapability {
            method: SubtitleGenerationMethod::GroqWhisperAccurate,
            available: true,
            reason: None,
        },
        qwen_local_capability(
            SubtitleGenerationMethod::QwenLocal0_6B,
            Qwen3ModelVariant::Small,
        ),
        qwen_local_capability(
            SubtitleGenerationMethod::QwenLocal1_7B,
            Qwen3ModelVariant::Large,
        ),
    ]
}

pub fn trimmed_language_hint(language_hint: Option<&str>) -> Option<&str> {
    language_hint
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "auto")
}

pub fn normalize_subtitle_text(text: &str) -> String {
    strip_qwen_control_tokens(text)
        .split_whitespace()
        .filter(|token| !(token.starts_with('<') && token.ends_with('>')))
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

pub fn normalize_groq_language_hint(language_hint: Option<&str>) -> Option<String> {
    normalize_language_alias(language_hint).map(|entry| entry.groq_code.to_string())
}

pub fn normalize_qwen_language_hint(language_hint: Option<&str>) -> Option<String> {
    normalize_language_alias(language_hint)
        .map(|entry| entry.qwen_name.to_string())
        .or_else(|| trimmed_language_hint(language_hint).map(ToOwned::to_owned))
}

pub fn ends_sentence(text: &str) -> bool {
    matches!(
        text.chars().last(),
        Some('.') | Some('!') | Some('?') | Some('…')
    )
}

pub fn join_word_tokens(tokens: &[&str]) -> String {
    let mut result = String::new();
    let mut previous: Option<&str> = None;
    for token in tokens {
        let trimmed = token.trim();
        if trimmed.is_empty() {
            continue;
        }
        if result.is_empty() {
            result.push_str(trimmed);
            previous = Some(trimmed);
            continue;
        }
        if should_attach_without_space(trimmed, previous) {
            result.push_str(trimmed);
        } else {
            result.push(' ');
            result.push_str(trimmed);
        }
        previous = Some(trimmed);
    }
    result
}

fn qwen_local_capability(
    method: SubtitleGenerationMethod,
    variant: Qwen3ModelVariant,
) -> SubtitleMethodCapability {
    let env = environment_info();
    let model_label = match variant {
        Qwen3ModelVariant::Small => "Qwen3-ASR 0.6B",
        Qwen3ModelVariant::Large => "Qwen3-ASR 1.7B",
    };
    if env.process_arch != RuntimeArch::X64 {
        return SubtitleMethodCapability {
            method,
            available: false,
            reason: Some(
                "Qwen Local subtitles currently require the x64 Windows build.".to_string(),
            ),
        };
    }
    if env.native_arch == RuntimeArch::Arm64 {
        return SubtitleMethodCapability {
            method,
            available: false,
            reason: Some(
                "Qwen Local subtitles are not supported on Windows-on-Arm yet.".to_string(),
            ),
        };
    }
    let model_downloaded = match variant {
        Qwen3ModelVariant::Small => assets::is_qwen3_model_downloaded(),
        Qwen3ModelVariant::Large => assets::is_qwen3_1_7b_model_downloaded(),
    };
    if !model_downloaded {
        return SubtitleMethodCapability {
            method,
            available: false,
            reason: Some(format!(
                "Install the {model_label} model from Downloaded Tools to use Qwen Local subtitles."
            )),
        };
    }
    if !reference::has_discoverable_server() {
        return SubtitleMethodCapability {
            method,
            available: false,
            reason: Some(
                "Install the Qwen3-ASR reference server from Downloaded Tools to use Qwen Local subtitles."
                    .to_string(),
            ),
        };
    }

    SubtitleMethodCapability {
        method,
        available: true,
        reason: None,
    }
}

fn should_attach_without_space(token: &str, previous: Option<&str>) -> bool {
    let leading = token.chars().next();
    if matches!(
        leading,
        Some('.') | Some(',') | Some('!') | Some('?') | Some(':') | Some(';') | Some('…')
    ) {
        return true;
    }
    if matches!(token, "'" | "’" | "\"" | ")" | "]" | "}" | "%") {
        return true;
    }
    if previous.is_some_and(|prev| matches!(prev, "(" | "[" | "{" | "\"" | "'")) {
        return true;
    }
    false
}

struct LanguageAlias {
    groq_code: &'static str,
    qwen_name: &'static str,
}

fn normalize_language_alias(language_hint: Option<&str>) -> Option<LanguageAlias> {
    let normalized = trimmed_language_hint(language_hint)?
        .trim()
        .to_ascii_lowercase()
        .replace('_', "-");
    let primary = normalized.split('-').next().unwrap_or("");
    Some(match primary {
        "afr" | "afrikaans" => LanguageAlias {
            groq_code: "af",
            qwen_name: "Afrikaans",
        },
        "sq" | "sqi" | "albanian" => LanguageAlias {
            groq_code: "sq",
            qwen_name: "Albanian",
        },
        "en" | "eng" | "english" => LanguageAlias {
            groq_code: "en",
            qwen_name: "English",
        },
        "az" | "aze" | "azerbaijani" => LanguageAlias {
            groq_code: "az",
            qwen_name: "Azerbaijani",
        },
        "be" | "bel" | "belarusian" => LanguageAlias {
            groq_code: "be",
            qwen_name: "Belarusian",
        },
        "bn" | "ben" | "bengali" => LanguageAlias {
            groq_code: "bn",
            qwen_name: "Bengali",
        },
        "bg" | "bul" | "bulgarian" => LanguageAlias {
            groq_code: "bg",
            qwen_name: "Bulgarian",
        },
        "my" | "mya" | "burmese" | "myanmar" => LanguageAlias {
            groq_code: "my",
            qwen_name: "Burmese",
        },
        "ca" | "cat" | "catalan" => LanguageAlias {
            groq_code: "ca",
            qwen_name: "Catalan",
        },
        "hr" | "hrv" | "croatian" => LanguageAlias {
            groq_code: "hr",
            qwen_name: "Croatian",
        },
        "cs" | "ces" | "cze" | "czech" => LanguageAlias {
            groq_code: "cs",
            qwen_name: "Czech",
        },
        "da" | "dan" | "danish" => LanguageAlias {
            groq_code: "da",
            qwen_name: "Danish",
        },
        "nl" | "nld" | "dut" | "dutch" => LanguageAlias {
            groq_code: "nl",
            qwen_name: "Dutch",
        },
        "eo" | "epo" | "esperanto" => LanguageAlias {
            groq_code: "eo",
            qwen_name: "Esperanto",
        },
        "et" | "est" | "estonian" => LanguageAlias {
            groq_code: "et",
            qwen_name: "Estonian",
        },
        "eu" | "eus" | "baq" | "basque" => LanguageAlias {
            groq_code: "eu",
            qwen_name: "Basque",
        },
        "fa" | "pes" | "per" | "fas" | "persian" | "farsi" => LanguageAlias {
            groq_code: "fa",
            qwen_name: "Persian",
        },
        "fi" | "fin" | "finnish" => LanguageAlias {
            groq_code: "fi",
            qwen_name: "Finnish",
        },
        "ko" | "kor" | "korean" => LanguageAlias {
            groq_code: "ko",
            qwen_name: "Korean",
        },
        "ja" | "jpn" | "japanese" => LanguageAlias {
            groq_code: "ja",
            qwen_name: "Japanese",
        },
        "zh" | "zho" | "cmn" | "chinese" | "mandarin" => LanguageAlias {
            groq_code: "zh",
            qwen_name: "Chinese",
        },
        "es" | "spa" | "spanish" => LanguageAlias {
            groq_code: "es",
            qwen_name: "Spanish",
        },
        "vi" | "vie" | "vietnamese" => LanguageAlias {
            groq_code: "vi",
            qwen_name: "Vietnamese",
        },
        "fr" | "fra" | "fre" | "french" => LanguageAlias {
            groq_code: "fr",
            qwen_name: "French",
        },
        "ka" | "kat" | "geo" | "georgian" => LanguageAlias {
            groq_code: "ka",
            qwen_name: "Georgian",
        },
        "de" | "deu" | "ger" | "german" => LanguageAlias {
            groq_code: "de",
            qwen_name: "German",
        },
        "el" | "ell" | "gre" | "greek" => LanguageAlias {
            groq_code: "el",
            qwen_name: "Greek",
        },
        "gu" | "guj" | "gujarati" => LanguageAlias {
            groq_code: "gu",
            qwen_name: "Gujarati",
        },
        "he" | "heb" | "hebrew" => LanguageAlias {
            groq_code: "he",
            qwen_name: "Hebrew",
        },
        "hi" | "hin" | "hindi" => LanguageAlias {
            groq_code: "hi",
            qwen_name: "Hindi",
        },
        "hu" | "hun" | "hungarian" => LanguageAlias {
            groq_code: "hu",
            qwen_name: "Hungarian",
        },
        "pt" | "por" | "portuguese" => LanguageAlias {
            groq_code: "pt",
            qwen_name: "Portuguese",
        },
        "ru" | "rus" | "russian" => LanguageAlias {
            groq_code: "ru",
            qwen_name: "Russian",
        },
        "it" | "ita" | "italian" => LanguageAlias {
            groq_code: "it",
            qwen_name: "Italian",
        },
        "kn" | "kan" | "kannada" => LanguageAlias {
            groq_code: "kn",
            qwen_name: "Kannada",
        },
        "la" | "lat" | "latin" => LanguageAlias {
            groq_code: "la",
            qwen_name: "Latin",
        },
        "lv" | "lav" | "latvian" => LanguageAlias {
            groq_code: "lv",
            qwen_name: "Latvian",
        },
        "lt" | "lit" | "lithuanian" => LanguageAlias {
            groq_code: "lt",
            qwen_name: "Lithuanian",
        },
        "ml" | "mal" | "malayalam" => LanguageAlias {
            groq_code: "ml",
            qwen_name: "Malayalam",
        },
        "mr" | "mar" | "marathi" => LanguageAlias {
            groq_code: "mr",
            qwen_name: "Marathi",
        },
        "mk" | "mkd" | "macedonian" => LanguageAlias {
            groq_code: "mk",
            qwen_name: "Macedonian",
        },
        "ne" | "nep" | "nepali" => LanguageAlias {
            groq_code: "ne",
            qwen_name: "Nepali",
        },
        "nb" | "nob" | "bokmal" | "bokmål" | "norwegian bokmal" | "norwegian bokmål" => {
            LanguageAlias {
                groq_code: "nb",
                qwen_name: "Norwegian Bokmål",
            }
        }
        "nn" | "nno" | "norwegian nynorsk" | "nynorsk" => LanguageAlias {
            groq_code: "nn",
            qwen_name: "Norwegian Nynorsk",
        },
        "or" | "ori" | "oriya" | "odia" => LanguageAlias {
            groq_code: "or",
            qwen_name: "Oriya",
        },
        "pa" | "pan" | "punjabi" => LanguageAlias {
            groq_code: "pa",
            qwen_name: "Punjabi",
        },
        "pl" | "pol" | "polish" => LanguageAlias {
            groq_code: "pl",
            qwen_name: "Polish",
        },
        "ro" | "ron" | "rum" | "romanian" => LanguageAlias {
            groq_code: "ro",
            qwen_name: "Romanian",
        },
        "sr" | "srp" | "serbian" => LanguageAlias {
            groq_code: "sr",
            qwen_name: "Serbian",
        },
        "si" | "sin" | "sinhala" => LanguageAlias {
            groq_code: "si",
            qwen_name: "Sinhala",
        },
        "sk" | "slk" | "slo" | "slovak" => LanguageAlias {
            groq_code: "sk",
            qwen_name: "Slovak",
        },
        "sl" | "slv" | "slovenian" => LanguageAlias {
            groq_code: "sl",
            qwen_name: "Slovenian",
        },
        "so" | "som" | "somali" => LanguageAlias {
            groq_code: "so",
            qwen_name: "Somali",
        },
        "sv" | "swe" | "swedish" => LanguageAlias {
            groq_code: "sv",
            qwen_name: "Swedish",
        },
        "ta" | "tam" | "tamil" => LanguageAlias {
            groq_code: "ta",
            qwen_name: "Tamil",
        },
        "tl" | "tgl" | "tagalog" => LanguageAlias {
            groq_code: "tl",
            qwen_name: "Tagalog",
        },
        "te" | "tel" | "telugu" => LanguageAlias {
            groq_code: "te",
            qwen_name: "Telugu",
        },
        "ar" | "ara" | "arabic" => LanguageAlias {
            groq_code: "ar",
            qwen_name: "Arabic",
        },
        "tr" | "tur" | "turkish" => LanguageAlias {
            groq_code: "tr",
            qwen_name: "Turkish",
        },
        "th" | "tha" | "thai" => LanguageAlias {
            groq_code: "th",
            qwen_name: "Thai",
        },
        "id" | "ind" | "indonesian" => LanguageAlias {
            groq_code: "id",
            qwen_name: "Indonesian",
        },
        "uk" | "ukr" | "ukrainian" => LanguageAlias {
            groq_code: "uk",
            qwen_name: "Ukrainian",
        },
        "ur" | "urd" | "urdu" => LanguageAlias {
            groq_code: "ur",
            qwen_name: "Urdu",
        },
        "uz" | "uzb" | "uzbek" => LanguageAlias {
            groq_code: "uz",
            qwen_name: "Uzbek",
        },
        "yi" | "yid" | "yiddish" => LanguageAlias {
            groq_code: "yi",
            qwen_name: "Yiddish",
        },
        _ => return None,
    })
}

fn strip_qwen_control_tokens(text: &str) -> String {
    text.replace("<asr_text>", " ")
        .replace("</asr_text>", " ")
        .replace("<|endoftext|>", " ")
        .replace("<|im_end|>", " ")
        .replace("<|im_start|>", " ")
}
