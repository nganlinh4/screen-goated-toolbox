//! Language detection via taalas LLM (llama-3.1-8B).
//!
//! Replaces the old local language-detector DLL approach. The LLM is more
//! accurate on short text and avoids a separate local detector dependency.
//! Replaces the old DLL-based language detection approach. The LLM is more
//! accurate on short text and avoids a separate local detector dependency.

use std::sync::Mutex;

/// Simple bounded cache to avoid repeated API calls for identical text.
static CACHE: Mutex<Option<Vec<(String, String)>>> = Mutex::new(None);
const CACHE_CAP: usize = 64;

/// Detect language of text, returns ISO 639-3 code (e.g. "eng", "vie", "kor").
/// Returns `None` on empty input or API failure.
pub fn detect_language(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
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

    let raw = crate::api::taalas::generate(&prompt)?;
    let code = parse_iso639_3(&raw)?;
    if isolang::Language::from_639_3(&code).is_none() {
        return None;
    }

    if let Ok(mut guard) = CACHE.lock() {
        let cache = guard.get_or_insert_with(Vec::new);
        if cache.len() >= CACHE_CAP {
            cache.remove(0);
        }
        cache.push((trimmed.to_string(), code.clone()));
    }

    Some(code)
}

fn parse_iso639_3(raw: &str) -> Option<String> {
    let trimmed = raw.trim().to_lowercase();
    if trimmed.len() == 3 && trimmed.chars().all(|c| c.is_ascii_lowercase()) {
        return Some(trimmed);
    }

    for word in trimmed.split(|c: char| !c.is_ascii_alphabetic()) {
        if word.len() == 3
            && word.chars().all(|c| c.is_ascii_lowercase())
            && isolang::Language::from_639_3(word).is_some()
        {
            return Some(word.to_string());
        }
    }

    None
}
