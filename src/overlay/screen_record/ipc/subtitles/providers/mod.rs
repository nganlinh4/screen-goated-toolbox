mod gemini;
mod gemini_segments;
mod gemini_stream;
mod groq;
mod parakeet_tdt;
mod qwen;

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crate::APP;
use crate::api::realtime_audio::parakeet_tdt_assets;
use crate::api::realtime_audio::qwen3::{Qwen3ModelVariant, assets, runtime};
use crate::model_config::get_model_by_id;
use crate::runtime_support::{RuntimeArch, environment_info};
use crate::unpack_dlls::{self, AiRuntimeStatus};

use super::media::PreparedSubtitleMedia;
use super::types::{CompactSubtitleSegment, SubtitleGenerationMethod, SubtitleMethodCapability};

pub struct SubtitleBackendProgress {
    pub completed_steps: usize,
    pub total_steps: usize,
    pub segments: Vec<CompactSubtitleSegment>,
}

pub struct SubtitleBackendRequest {
    pub media: PreparedSubtitleMedia,
    pub language_hint: Option<String>,
    pub gemini_prompt: Option<String>,
    pub groq_vocabulary: Vec<String>,
    pub cancel_token: Arc<AtomicBool>,
}

pub trait SubtitleBackend {
    fn transcribe_clip(
        &mut self,
        request: SubtitleBackendRequest,
        on_progress: &mut dyn FnMut(SubtitleBackendProgress) -> Result<(), String>,
    ) -> Result<Vec<CompactSubtitleSegment>, String>;
}

pub fn create_backend(
    method: SubtitleGenerationMethod,
) -> Result<Box<dyn SubtitleBackend>, String> {
    match method {
        SubtitleGenerationMethod::GroqWhisperAccurate => {
            Ok(Box::new(groq::GroqSubtitleBackend::new(method)?))
        }
        SubtitleGenerationMethod::GroqWhisperLargeV3Turbo => {
            Ok(Box::new(groq::GroqSubtitleBackend::new(method)?))
        }
        SubtitleGenerationMethod::Gemini3_1FlashLite => {
            Ok(Box::new(gemini::GeminiSubtitleBackend::new(method)?))
        }
        SubtitleGenerationMethod::Gemini3FlashPreview => {
            Ok(Box::new(gemini::GeminiSubtitleBackend::new(method)?))
        }
        SubtitleGenerationMethod::QwenLocal0_6B => Ok(Box::new(qwen::QwenSubtitleBackend::new(
            Qwen3ModelVariant::Small,
        )?)),
        SubtitleGenerationMethod::QwenLocal1_7B => Ok(Box::new(qwen::QwenSubtitleBackend::new(
            Qwen3ModelVariant::Large,
        )?)),
        SubtitleGenerationMethod::ParakeetTdt0_6BV3 => {
            Ok(Box::new(parakeet_tdt::ParakeetTdtSubtitleBackend::new()))
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
        SubtitleMethodCapability {
            method: SubtitleGenerationMethod::GroqWhisperLargeV3Turbo,
            available: true,
            reason: None,
        },
        gemini_subtitle_capability(SubtitleGenerationMethod::Gemini3_1FlashLite),
        gemini_subtitle_capability(SubtitleGenerationMethod::Gemini3FlashPreview),
        qwen_local_capability(
            SubtitleGenerationMethod::QwenLocal0_6B,
            Qwen3ModelVariant::Small,
        ),
        qwen_local_capability(
            SubtitleGenerationMethod::QwenLocal1_7B,
            Qwen3ModelVariant::Large,
        ),
        parakeet_tdt_capability(),
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
    if let Some(entry) = normalize_language_alias(language_hint) {
        return Some(entry.groq_code.to_string());
    }
    let primary = normalized_language_primary(language_hint)?;
    is_groq_whisper_language_code(&primary).then_some(primary)
}

pub fn normalize_qwen_language_hint(language_hint: Option<&str>) -> Option<String> {
    normalize_qwen_supported_language(language_hint)
        .map(ToOwned::to_owned)
        .or_else(|| {
            normalize_language_alias(language_hint).map(|entry| entry.qwen_name.to_string())
        })
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
    if !runtime::has_discoverable_qwen3_runtime() {
        return SubtitleMethodCapability {
            method,
            available: false,
            reason: Some(
                "Install the Qwen3-ASR CUDA Runtime from Downloaded Tools to use Qwen Local subtitles."
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

fn parakeet_tdt_capability() -> SubtitleMethodCapability {
    let method = SubtitleGenerationMethod::ParakeetTdt0_6BV3;
    let env = environment_info();
    if env.process_arch != RuntimeArch::X64 {
        return SubtitleMethodCapability {
            method,
            available: false,
            reason: Some(
                "Parakeet TDT subtitles currently require the x64 Windows build.".to_string(),
            ),
        };
    }
    if !parakeet_tdt_assets::is_parakeet_tdt_model_downloaded() {
        return SubtitleMethodCapability {
            method,
            available: false,
            reason: Some(
                "Install the Parakeet TDT 0.6B v3 model from Downloaded Tools to use Parakeet subtitles."
                    .to_string(),
            ),
        };
    }
    if !matches!(
        unpack_dlls::current_ai_runtime_status(),
        AiRuntimeStatus::Installed { .. }
    ) {
        return SubtitleMethodCapability {
            method,
            available: false,
            reason: Some(
                "Install the local AI runtime from Downloaded Tools to use Parakeet subtitles."
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

fn gemini_subtitle_capability(method: SubtitleGenerationMethod) -> SubtitleMethodCapability {
    let app = match APP.lock() {
        Ok(app) => app,
        Err(_) => {
            return SubtitleMethodCapability {
                method,
                available: false,
                reason: Some("APP lock poisoned".to_string()),
            };
        }
    };
    if !app.config.use_gemini {
        return SubtitleMethodCapability {
            method,
            available: false,
            reason: Some("Enable Gemini in Settings to use Gemini subtitles.".to_string()),
        };
    }
    if app.config.gemini_api_key.trim().is_empty() {
        return SubtitleMethodCapability {
            method,
            available: false,
            reason: Some("Add a Gemini API key in Settings to use Gemini subtitles.".to_string()),
        };
    }
    let model_id = gemini::gemini_subtitle_model_id(method);
    if get_model_by_id(model_id).is_none() {
        let model_label = match method {
            SubtitleGenerationMethod::Gemini3_1FlashLite => "Gemini 3.1 Flash Lite",
            SubtitleGenerationMethod::Gemini3FlashPreview => "Gemini 3 Flash Preview",
            _ => "Gemini",
        };
        return SubtitleMethodCapability {
            method,
            available: false,
            reason: Some(format!("{model_label} subtitle model config is missing.")),
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

fn normalized_language_primary(language_hint: Option<&str>) -> Option<String> {
    let normalized = trimmed_language_hint(language_hint)?
        .trim()
        .to_ascii_lowercase()
        .replace('_', "-");
    Some(normalized.split('-').next().unwrap_or("").to_string())
}

fn is_groq_whisper_language_code(code: &str) -> bool {
    matches!(
        code,
        "af" | "am"
            | "ar"
            | "as"
            | "az"
            | "ba"
            | "be"
            | "bg"
            | "bn"
            | "bo"
            | "br"
            | "bs"
            | "ca"
            | "cs"
            | "cy"
            | "da"
            | "de"
            | "el"
            | "en"
            | "es"
            | "et"
            | "eu"
            | "fa"
            | "fi"
            | "fo"
            | "fr"
            | "gl"
            | "gu"
            | "ha"
            | "haw"
            | "he"
            | "hi"
            | "hr"
            | "ht"
            | "hu"
            | "hy"
            | "id"
            | "is"
            | "it"
            | "ja"
            | "jw"
            | "ka"
            | "kk"
            | "km"
            | "kn"
            | "ko"
            | "la"
            | "lb"
            | "ln"
            | "lo"
            | "lt"
            | "lv"
            | "mg"
            | "mi"
            | "mk"
            | "ml"
            | "mn"
            | "mr"
            | "ms"
            | "mt"
            | "my"
            | "ne"
            | "nl"
            | "nn"
            | "no"
            | "oc"
            | "pa"
            | "pl"
            | "ps"
            | "pt"
            | "ro"
            | "ru"
            | "sa"
            | "sd"
            | "si"
            | "sk"
            | "sl"
            | "sn"
            | "so"
            | "sq"
            | "sr"
            | "su"
            | "sv"
            | "sw"
            | "ta"
            | "te"
            | "tg"
            | "th"
            | "tk"
            | "tl"
            | "tr"
            | "tt"
            | "uk"
            | "ur"
            | "uz"
            | "vi"
            | "yi"
            | "yo"
            | "yue"
            | "zh"
    )
}

fn normalize_qwen_supported_language(language_hint: Option<&str>) -> Option<&'static str> {
    let normalized = trimmed_language_hint(language_hint)?
        .trim()
        .to_ascii_lowercase()
        .replace('_', "-");
    match normalized.as_str() {
        "qwen-dialect-anhui" | "anhui" => return Some("Anhui"),
        "qwen-dialect-dongbei" | "dongbei" => return Some("Dongbei"),
        "qwen-dialect-fujian" | "fujian" => return Some("Fujian"),
        "qwen-dialect-gansu" | "gansu" => return Some("Gansu"),
        "qwen-dialect-guizhou" | "guizhou" => return Some("Guizhou"),
        "qwen-dialect-hebei" | "hebei" => return Some("Hebei"),
        "qwen-dialect-henan" | "henan" => return Some("Henan"),
        "qwen-dialect-hubei" | "hubei" => return Some("Hubei"),
        "qwen-dialect-hunan" | "hunan" => return Some("Hunan"),
        "qwen-dialect-jiangxi" | "jiangxi" => return Some("Jiangxi"),
        "qwen-dialect-ningxia" | "ningxia" => return Some("Ningxia"),
        "qwen-dialect-shandong" | "shandong" => return Some("Shandong"),
        "qwen-dialect-shaanxi" | "shaanxi" => return Some("Shaanxi"),
        "qwen-dialect-shanxi" | "shanxi" => return Some("Shanxi"),
        "qwen-dialect-sichuan" | "sichuan" => return Some("Sichuan"),
        "qwen-dialect-tianjin" | "tianjin" => return Some("Tianjin"),
        "qwen-dialect-yunnan" | "yunnan" => return Some("Yunnan"),
        "qwen-dialect-zhejiang" | "zhejiang" => return Some("Zhejiang"),
        "qwen-dialect-cantonese-hk" | "cantonese-hk" | "hong-kong-cantonese" => {
            return Some("Cantonese (Hong Kong)");
        }
        "qwen-dialect-cantonese-gd" | "cantonese-gd" | "guangdong-cantonese" => {
            return Some("Cantonese (Guangdong)");
        }
        "qwen-dialect-wu" | "wu" | "shanghainese" => return Some("Wu language"),
        "qwen-dialect-minnan" | "minnan" | "hokkien" => return Some("Minnan language"),
        _ => {}
    }
    let primary = normalized.split('-').next().unwrap_or("");
    Some(match primary {
        "ar" | "ara" | "arabic" => "Arabic",
        "cs" | "ces" | "cze" | "czech" => "Czech",
        "da" | "dan" | "danish" => "Danish",
        "de" | "deu" | "ger" | "german" => "German",
        "el" | "ell" | "gre" | "greek" => "Greek",
        "en" | "eng" | "english" => "English",
        "es" | "spa" | "spanish" => "Spanish",
        "fa" | "pes" | "per" | "fas" | "persian" | "farsi" => "Persian",
        "fi" | "fin" | "finnish" => "Finnish",
        "fil" | "tl" | "tgl" | "tagalog" | "filipino" => "Filipino",
        "fr" | "fra" | "fre" | "french" => "French",
        "hi" | "hin" | "hindi" => "Hindi",
        "hu" | "hun" | "hungarian" => "Hungarian",
        "id" | "ind" | "indonesian" => "Indonesian",
        "it" | "ita" | "italian" => "Italian",
        "ja" | "jpn" | "japanese" => "Japanese",
        "ko" | "kor" | "korean" => "Korean",
        "mk" | "mkd" | "macedonian" => "Macedonian",
        "ms" | "msa" | "may" | "malay" => "Malay",
        "nl" | "nld" | "dut" | "dutch" => "Dutch",
        "pl" | "pol" | "polish" => "Polish",
        "pt" | "por" | "portuguese" => "Portuguese",
        "ro" | "ron" | "rum" | "romanian" => "Romanian",
        "ru" | "rus" | "russian" => "Russian",
        "sv" | "swe" | "swedish" => "Swedish",
        "th" | "tha" | "thai" => "Thai",
        "tr" | "tur" | "turkish" => "Turkish",
        "vi" | "vie" | "vietnamese" => "Vietnamese",
        "yue" | "cantonese" => "Cantonese",
        "zh" | "zho" | "cmn" | "chinese" | "mandarin" => "Chinese",
        _ => return None,
    })
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
                groq_code: "no",
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
