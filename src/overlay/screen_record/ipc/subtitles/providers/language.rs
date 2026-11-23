pub fn trimmed_language_hint(language_hint: Option<&str>) -> Option<&str> {
    language_hint
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "auto")
}

pub fn normalize_subtitle_text(text: &str) -> String {
    text.split_whitespace()
        .filter(|token| !(token.starts_with('<') && token.ends_with('>')))
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

pub fn normalize_groq_language_hint(language_hint: Option<&str>) -> Option<String> {
    if let Some(entry) = normalize_language_alias(language_hint) {
        return crate::config::speech_languages::whisper_language_code(Some(entry))
            .map(ToOwned::to_owned);
    }
    let primary = normalized_language_primary(language_hint)?;
    crate::config::speech_languages::whisper_language_code(Some(&primary)).map(ToOwned::to_owned)
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

fn normalized_language_primary(language_hint: Option<&str>) -> Option<String> {
    let normalized = trimmed_language_hint(language_hint)?
        .trim()
        .to_ascii_lowercase()
        .replace('_', "-");
    Some(normalized.split('-').next().unwrap_or("").to_string())
}

fn normalize_language_alias(language_hint: Option<&str>) -> Option<&'static str> {
    let normalized = trimmed_language_hint(language_hint)?
        .trim()
        .to_ascii_lowercase()
        .replace('_', "-");
    let primary = normalized.split('-').next().unwrap_or("");
    Some(match primary {
        "afr" | "afrikaans" => "af",
        "sq" | "sqi" | "albanian" => "sq",
        "en" | "eng" | "english" => "en",
        "az" | "aze" | "azerbaijani" => "az",
        "be" | "bel" | "belarusian" => "be",
        "bn" | "ben" | "bengali" => "bn",
        "bg" | "bul" | "bulgarian" => "bg",
        "my" | "mya" | "burmese" | "myanmar" => "my",
        "ca" | "cat" | "catalan" => "ca",
        "hr" | "hrv" | "croatian" => "hr",
        "cs" | "ces" | "cze" | "czech" => "cs",
        "da" | "dan" | "danish" => "da",
        "nl" | "nld" | "dut" | "dutch" => "nl",
        "eo" | "epo" | "esperanto" => "eo",
        "et" | "est" | "estonian" => "et",
        "eu" | "eus" | "baq" | "basque" => "eu",
        "fa" | "pes" | "per" | "fas" | "persian" | "farsi" => "fa",
        "fi" | "fin" | "finnish" => "fi",
        "ko" | "kor" | "korean" => "ko",
        "ja" | "jpn" | "japanese" => "ja",
        "zh" | "zho" | "cmn" | "chinese" | "mandarin" => "zh",
        "es" | "spa" | "spanish" => "es",
        "vi" | "vie" | "vietnamese" => "vi",
        "fr" | "fra" | "fre" | "french" => "fr",
        "ka" | "kat" | "geo" | "georgian" => "ka",
        "de" | "deu" | "ger" | "german" => "de",
        "el" | "ell" | "gre" | "greek" => "el",
        "gu" | "guj" | "gujarati" => "gu",
        "he" | "heb" | "hebrew" => "he",
        "hi" | "hin" | "hindi" => "hi",
        "hu" | "hun" | "hungarian" => "hu",
        "pt" | "por" | "portuguese" => "pt",
        "ru" | "rus" | "russian" => "ru",
        "it" | "ita" | "italian" => "it",
        "kn" | "kan" | "kannada" => "kn",
        "la" | "lat" | "latin" => "la",
        "lv" | "lav" | "latvian" => "lv",
        "lt" | "lit" | "lithuanian" => "lt",
        "ml" | "mal" | "malayalam" => "ml",
        "mr" | "mar" | "marathi" => "mr",
        "mk" | "mkd" | "macedonian" => "mk",
        "ne" | "nep" | "nepali" => "ne",
        "nb" | "nob" | "bokmal" | "bokmål" | "norwegian bokmal" | "norwegian bokmål" => "no",
        "nn" | "nno" | "norwegian nynorsk" | "nynorsk" => "nn",
        "or" | "ori" | "oriya" | "odia" => "or",
        "pa" | "pan" | "punjabi" => "pa",
        "pl" | "pol" | "polish" => "pl",
        "ro" | "ron" | "rum" | "romanian" => "ro",
        "sr" | "srp" | "serbian" => "sr",
        "si" | "sin" | "sinhala" => "si",
        "sk" | "slk" | "slo" | "slovak" => "sk",
        "sl" | "slv" | "slovenian" => "sl",
        "so" | "som" | "somali" => "so",
        "sv" | "swe" | "swedish" => "sv",
        "ta" | "tam" | "tamil" => "ta",
        "tl" | "tgl" | "tagalog" => "tl",
        "te" | "tel" | "telugu" => "te",
        "ar" | "ara" | "arabic" => "ar",
        "tr" | "tur" | "turkish" => "tr",
        "th" | "tha" | "thai" => "th",
        "id" | "ind" | "indonesian" => "id",
        "uk" | "ukr" | "ukrainian" => "uk",
        "ur" | "urd" | "urdu" => "ur",
        "uz" | "uzb" | "uzbek" => "uz",
        "yi" | "yid" | "yiddish" => "yi",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::speech_languages::{WHISPER_LANGUAGES, whisper_language_code};

    #[test]
    fn subtitle_language_hints_stay_within_the_supported_catalog() {
        for language in WHISPER_LANGUAGES.iter() {
            assert_eq!(
                normalize_groq_language_hint(Some(&language.value)).as_deref(),
                Some(language.value.as_str())
            );
        }
        for language in (0..10000).filter_map(isolang::Language::from_usize) {
            let label = language.to_name();
            if let Some(code) = normalize_groq_language_hint(Some(label)) {
                assert!(
                    whisper_language_code(Some(&code)).is_some(),
                    "{label}: {code}"
                );
            }
        }
        assert_eq!(normalize_groq_language_hint(Some("auto")), None);
        assert_eq!(normalize_groq_language_hint(None), None);
    }
}
