#[path = "../../../../../src/api/realtime_audio/s2s.rs"]
pub(crate) mod s2s;
#[path = "../../../../../src/api/realtime_audio/websocket.rs"]
pub(crate) mod websocket;
pub(crate) fn translate_with_google_gtx(text: &str, target_lang: &str) -> Option<String> {
    let target = target_lang.trim();
    let target_code = if (2..=3).contains(&target.len())
        && target
            .chars()
            .all(|character| character.is_ascii_alphabetic())
    {
        target.to_ascii_lowercase()
    } else {
        isolang::Language::from_name(target)
            .and_then(|language| language.to_639_1())
            .map(str::to_owned)
            .unwrap_or_else(|| "en".to_owned())
    };
    let url = format!(
        "https://translate.googleapis.com/translate_a/single?client=gtx&sl=auto&tl={target_code}&dt=t&q={}",
        urlencoding::encode(text)
    );
    let response = client::UREQ_AGENT
        .get(&url)
        .header("User-Agent", "Mozilla/5.0")
        .call()
        .ok()?;
    let value = response.into_body().read_json::<serde_json::Value>().ok()?;
    let translated = value
        .get(0)?
        .as_array()?
        .iter()
        .filter_map(|sentence| sentence.get(0)?.as_str())
        .collect::<String>();
    (!translated.is_empty()).then_some(translated)
}

use crate::api::client;
