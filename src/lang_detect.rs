//! Language detection via taalas LLM (llama-3.1-8B).
//!
//! Replaces the old local language-detector DLL approach. The LLM is more
//! accurate on short text and avoids a separate local detector dependency.
//! Only a bounded leading sample leaves the machine, under a short timeout.

use std::sync::Mutex;
use std::time::Duration;

/// Leading characters sent for detection: enough to identify a language,
/// without disclosing or waiting on whole documents.
const SAMPLE_CHARS: usize = 300;
/// Callers fall back to a default voice rather than wait on a slow detector.
const DETECT_TIMEOUT: Duration = Duration::from_secs(8);

/// Simple bounded cache to avoid repeated API calls for identical text.
static CACHE: Mutex<Option<Vec<(String, String)>>> = Mutex::new(None);
const CACHE_CAP: usize = 64;

/// Detect language of text, returns ISO 639-3 code (e.g. "eng", "vie", "kor").
/// Returns `None` on empty input or API failure.
pub fn detect_language(text: &str) -> Option<String> {
    let trimmed = detection_sample(text)?;
    if let Ok(guard) = CACHE.lock()
        && let Some(cache) = guard.as_ref()
        && let Some((_, code)) = cache.iter().find(|(key, _)| key == trimmed)
    {
        return Some(code.clone());
    }

    let prompt = format!(
        "What language is this text written in? Respond with ONLY the ISO 639-3 three-letter \
         code (e.g. eng, kor, vie, jpn, zho, spa, fra, deu). No other text.\n\n\"{}\"",
        trimmed
    );

    #[cfg(not(feature = "recorder-worker"))]
    let timeout = crate::api::client::RequestTimeouts::uniform(DETECT_TIMEOUT);
    #[cfg(feature = "recorder-worker")]
    let timeout = DETECT_TIMEOUT;
    let raw = crate::api::taalas::generate(&prompt, &None, Some(timeout)).ok()?;
    let code = parse_iso639_3(&raw)?;
    isolang::Language::from_639_3(&code)?;

    if let Ok(mut guard) = CACHE.lock() {
        let cache = guard.get_or_insert_with(Vec::new);
        if cache.len() >= CACHE_CAP {
            cache.remove(0);
        }
        cache.push((trimmed.to_string(), code.clone()));
    }

    Some(code)
}

/// The trimmed leading sample that identifies the text's language, cut on a
/// character boundary; `None` for blank text.
fn detection_sample(text: &str) -> Option<&str> {
    let trimmed = text.trim();
    let end = trimmed
        .char_indices()
        .nth(SAMPLE_CHARS)
        .map_or(trimmed.len(), |(index, _)| index);
    let sample = trimmed[..end].trim_end();
    (!sample.is_empty()).then_some(sample)
}

/// Read the detected language from the model's reply.
///
/// The model is asked for a bare ISO 639-3 code, but replies vary: a bare code
/// in any case or quoting, an ISO 639-1 code, an English language name, or a
/// code wrapped in prose. Ordinary English words can spell valid codes
/// themselves (`the` is one), so prose words never count as codes. When the
/// reply still names more than one language, there is no answer rather than a
/// guess, and callers fall back.
fn parse_iso639_3(raw: &str) -> Option<String> {
    parse_language_reply(raw).map(|language| language.to_639_3().to_string())
}

fn parse_language_reply(raw: &str) -> Option<isolang::Language> {
    let bare = raw.trim().trim_matches(|c: char| !c.is_alphanumeric());
    if let Some(language) = exact_language(bare) {
        return Some(language);
    }

    let words: Vec<&str> = raw
        .split(|c: char| !c.is_alphabetic())
        .filter(|word| !word.is_empty())
        .collect();
    let mut codes = Vec::new();
    let mut named = Vec::new();
    for word in &words {
        let lower = word.to_ascii_lowercase();
        if word.len() == 3
            && word.is_ascii()
            && !PROSE_WORDS.contains(&lower.as_str())
            && let Some(language) = isolang::Language::from_639_3(&lower)
            && !codes.contains(&language)
        {
            codes.push(language);
        }
        if word.chars().next().is_some_and(char::is_uppercase)
            && word.chars().count() >= 4
            && let Some(language) = isolang::Language::from_name(word)
            && !named.contains(&language)
        {
            named.push(language);
        }
    }

    let agreed: Vec<_> = codes.iter().filter(|code| named.contains(code)).collect();
    match (agreed.as_slice(), codes.as_slice(), named.as_slice()) {
        ([language], _, _) => Some(**language),
        ([], [language], _) | ([], [], [language]) => Some(*language),
        _ => None,
    }
}

/// The whole reply is one code or one language name.
fn exact_language(reply: &str) -> Option<isolang::Language> {
    let lower = reply.to_ascii_lowercase();
    match reply.len() {
        2 => isolang::Language::from_639_1(&lower),
        3 => isolang::Language::from_639_3(&lower),
        _ => title_case(reply).and_then(|name| isolang::Language::from_name(&name)),
    }
}

fn title_case(word: &str) -> Option<String> {
    let mut chars = word.chars();
    let first = chars.next()?;
    Some(
        first
            .to_uppercase()
            .chain(chars.flat_map(char::to_lowercase))
            .collect(),
    )
}

/// Common English words that are also valid ISO 639-3 codes, or could be.
const PROSE_WORDS: &[&str] = &[
    "the", "and", "for", "are", "but", "not", "you", "all", "any", "can", "had", "her", "was",
    "one", "our", "out", "has", "his", "how", "its", "may", "new", "now", "old", "see", "two",
    "who", "did", "get", "him", "let", "put", "say", "she", "too", "use", "way", "yes", "yet",
    "per", "via", "non", "off", "own", "few", "why", "ago", "iso", "code", "lang",
];

#[cfg(test)]
mod tests {
    use super::{SAMPLE_CHARS, detection_sample, parse_iso639_3};

    #[test]
    fn only_a_bounded_leading_sample_is_sent() {
        let document = "Xin chào thế giới. ".repeat(500);
        let sample = detection_sample(&document).unwrap();
        assert!(sample.chars().count() <= SAMPLE_CHARS);
        assert!(document.trim().starts_with(sample));
        assert_eq!(detection_sample("  short text \n"), Some("short text"));
        assert_eq!(detection_sample(" \n\t "), None);
        let multibyte = "한".repeat(SAMPLE_CHARS + 5);
        assert_eq!(
            detection_sample(&multibyte).unwrap().chars().count(),
            SAMPLE_CHARS
        );
    }

    #[test]
    fn the_privacy_policy_discloses_the_detector_and_its_sample_bound() {
        let policy = include_str!("../privacy-policy.html")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        assert!(policy.contains("chatjimmy.ai"));
        assert!(policy.contains(&format!("at most the first {SAMPLE_CHARS} characters")));
    }

    fn parsed(reply: &str) -> Option<String> {
        parse_iso639_3(reply)
    }

    #[test]
    fn bare_codes_in_any_case_or_quoting_are_accepted() {
        for reply in ["kor", " KOR \n", "`kor`", "\"kor\".", "'Kor'", "ko", "KO."] {
            assert_eq!(parsed(reply).as_deref(), Some("kor"), "{reply:?}");
        }
    }

    #[test]
    fn a_bare_language_name_is_accepted() {
        assert_eq!(parsed("Korean").as_deref(), Some("kor"));
        assert_eq!(parsed("vietnamese.").as_deref(), Some("vie"));
    }

    #[test]
    fn prose_words_that_spell_codes_never_outrank_the_answer() {
        assert_eq!(
            parsed("The language is Korean (kor).").as_deref(),
            Some("kor")
        );
        assert_eq!(
            parsed("kor. The text is written in Korean.").as_deref(),
            Some("kor")
        );
        assert_eq!(parsed("The answer is vie").as_deref(), Some("vie"));
        assert_eq!(parsed("Language: vie").as_deref(), Some("vie"));
        assert_eq!(parsed("ISO 639-3 code: jpn").as_deref(), Some("jpn"));
        assert_eq!(
            parsed("The text is written in Japanese.").as_deref(),
            Some("jpn")
        );
    }

    #[test]
    fn an_ambiguous_or_empty_reply_is_no_answer() {
        assert_eq!(parsed("eng or fra"), None);
        assert_eq!(parsed(""), None);
        assert_eq!(parsed("I cannot tell."), None);
    }
}
