//! Shared language detection using lingua-rs.
//! Replaces whatlang with better short-text accuracy.

use std::sync::LazyLock;

use lingua::{Language, LanguageDetector, LanguageDetectorBuilder};

/// Global detector instance (low-accuracy mode for speed, all compiled languages).
static DETECTOR: LazyLock<LanguageDetector> = LazyLock::new(|| {
    LanguageDetectorBuilder::from_all_languages()
        .with_low_accuracy_mode()
        .build()
});

/// Detect language of text, returns ISO 639-3 code (e.g. "eng", "vie", "kor").
/// Works reliably even on short text and single words.
pub fn detect_language(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    DETECTOR
        .detect_language_of(trimmed)
        .map(|lang| lingua_to_iso639_3(lang).to_string())
}

/// Detect language, returning the lingua Language enum directly.
pub fn detect_language_enum(text: &str) -> Option<Language> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    DETECTOR.detect_language_of(trimmed)
}

/// Convert lingua Language to ISO 639-3 code string.
pub fn lingua_to_iso639_3(lang: Language) -> &'static str {
    match lang {
        Language::Afrikaans => "afr",
        Language::Albanian => "sqi",
        Language::Arabic => "ara",
        Language::Armenian => "hye",
        Language::Azerbaijani => "aze",
        Language::Basque => "eus",
        Language::Belarusian => "bel",
        Language::Bengali => "ben",
        Language::Bokmal => "nob",
        Language::Bosnian => "bos",
        Language::Bulgarian => "bul",
        Language::Catalan => "cat",
        Language::Chinese => "zho",
        Language::Croatian => "hrv",
        Language::Czech => "ces",
        Language::Danish => "dan",
        Language::Dutch => "nld",
        Language::English => "eng",
        Language::Esperanto => "epo",
        Language::Estonian => "est",
        Language::Finnish => "fin",
        Language::French => "fra",
        Language::Ganda => "lug",
        Language::Georgian => "kat",
        Language::German => "deu",
        Language::Greek => "ell",
        Language::Gujarati => "guj",
        Language::Hebrew => "heb",
        Language::Hindi => "hin",
        Language::Hungarian => "hun",
        Language::Icelandic => "isl",
        Language::Indonesian => "ind",
        Language::Irish => "gle",
        Language::Italian => "ita",
        Language::Japanese => "jpn",
        Language::Kazakh => "kaz",
        Language::Korean => "kor",
        Language::Latin => "lat",
        Language::Latvian => "lav",
        Language::Lithuanian => "lit",
        Language::Macedonian => "mkd",
        Language::Malay => "msa",
        Language::Maori => "mri",
        Language::Marathi => "mar",
        Language::Mongolian => "mon",
        Language::Nynorsk => "nno",
        Language::Persian => "fas",
        Language::Polish => "pol",
        Language::Portuguese => "por",
        Language::Punjabi => "pan",
        Language::Romanian => "ron",
        Language::Russian => "rus",
        Language::Serbian => "srp",
        Language::Shona => "sna",
        Language::Slovak => "slk",
        Language::Slovene => "slv",
        Language::Somali => "som",
        Language::Sotho => "sot",
        Language::Spanish => "spa",
        Language::Swahili => "swa",
        Language::Swedish => "swe",
        Language::Tagalog => "tgl",
        Language::Tamil => "tam",
        Language::Telugu => "tel",
        Language::Thai => "tha",
        Language::Tsonga => "tso",
        Language::Tswana => "tsn",
        Language::Turkish => "tur",
        Language::Ukrainian => "ukr",
        Language::Urdu => "urd",
        Language::Vietnamese => "vie",
        Language::Welsh => "cym",
        Language::Xhosa => "xho",
        Language::Yoruba => "yor",
        Language::Zulu => "zul",
    }
}

/// Convert ISO 639-3 code to lingua Language enum (for TTS language conditions).
pub fn iso639_3_to_lingua(code: &str) -> Option<Language> {
    match code {
        "afr" => Some(Language::Afrikaans),
        "sqi" => Some(Language::Albanian),
        "ara" => Some(Language::Arabic),
        "hye" => Some(Language::Armenian),
        "aze" => Some(Language::Azerbaijani),
        "eus" => Some(Language::Basque),
        "bel" => Some(Language::Belarusian),
        "ben" => Some(Language::Bengali),
        "nob" => Some(Language::Bokmal),
        "bos" => Some(Language::Bosnian),
        "bul" => Some(Language::Bulgarian),
        "cat" => Some(Language::Catalan),
        "zho" => Some(Language::Chinese),
        "hrv" => Some(Language::Croatian),
        "ces" => Some(Language::Czech),
        "dan" => Some(Language::Danish),
        "nld" => Some(Language::Dutch),
        "eng" => Some(Language::English),
        "epo" => Some(Language::Esperanto),
        "est" => Some(Language::Estonian),
        "fin" => Some(Language::Finnish),
        "fra" => Some(Language::French),
        "lug" => Some(Language::Ganda),
        "kat" => Some(Language::Georgian),
        "deu" => Some(Language::German),
        "ell" => Some(Language::Greek),
        "guj" => Some(Language::Gujarati),
        "heb" => Some(Language::Hebrew),
        "hin" => Some(Language::Hindi),
        "hun" => Some(Language::Hungarian),
        "isl" => Some(Language::Icelandic),
        "ind" => Some(Language::Indonesian),
        "gle" => Some(Language::Irish),
        "ita" => Some(Language::Italian),
        "jpn" => Some(Language::Japanese),
        "kaz" => Some(Language::Kazakh),
        "kor" => Some(Language::Korean),
        "lat" => Some(Language::Latin),
        "lav" => Some(Language::Latvian),
        "lit" => Some(Language::Lithuanian),
        "mkd" => Some(Language::Macedonian),
        "msa" => Some(Language::Malay),
        "mri" => Some(Language::Maori),
        "mar" => Some(Language::Marathi),
        "mon" => Some(Language::Mongolian),
        "nno" => Some(Language::Nynorsk),
        "fas" => Some(Language::Persian),
        "pol" => Some(Language::Polish),
        "por" => Some(Language::Portuguese),
        "pan" => Some(Language::Punjabi),
        "ron" => Some(Language::Romanian),
        "rus" => Some(Language::Russian),
        "srp" => Some(Language::Serbian),
        "sna" => Some(Language::Shona),
        "slk" => Some(Language::Slovak),
        "slv" => Some(Language::Slovene),
        "som" => Some(Language::Somali),
        "sot" => Some(Language::Sotho),
        "spa" => Some(Language::Spanish),
        "swa" => Some(Language::Swahili),
        "swe" => Some(Language::Swedish),
        "tgl" => Some(Language::Tagalog),
        "tam" => Some(Language::Tamil),
        "tel" => Some(Language::Telugu),
        "tha" => Some(Language::Thai),
        "tso" => Some(Language::Tsonga),
        "tsn" => Some(Language::Tswana),
        "tur" => Some(Language::Turkish),
        "ukr" => Some(Language::Ukrainian),
        "urd" => Some(Language::Urdu),
        "vie" => Some(Language::Vietnamese),
        "cym" => Some(Language::Welsh),
        "xho" => Some(Language::Xhosa),
        "yor" => Some(Language::Yoruba),
        "zul" => Some(Language::Zulu),
        _ => None,
    }
}
