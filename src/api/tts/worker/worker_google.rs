// Google Translate TTS handler.

use std::io::Read;
use std::sync::Arc;

use super::super::manager::TtsManager;
use super::super::types::AudioEvent;
use super::open_weights::{fail_request, send_pcm_samples};
use crate::api::client::UREQ_AGENT;
use isolang::Language;

const PROVIDER: &str = "Google";
/// The translate_tts endpoint rejects `q` values longer than about 200 characters.
const GOOGLE_TTS_MAX_CHARS: usize = 200;

/// Google Translate TTS integrated with the existing audio pipeline.
/// Splits text under the endpoint limit, then downloads, decodes and streams
/// each piece in order through the AudioEvent channel.
pub(super) fn handle_google_tts(
    manager: Arc<TtsManager>,
    request: super::super::types::QueuedRequest,
    tx: std::sync::mpsc::Sender<AudioEvent>,
) {
    let text = request.req.text.clone();
    let hwnd = request.req.hwnd;
    let speed = request
        .req
        .profile
        .as_ref()
        .map(|profile| profile.google_speed.as_str())
        .unwrap_or("Normal");

    // Detect language for Google TTS TL parameter, unless a batched caller
    // already supplied a stable language hint.
    let detected_code = request
        .req
        .profile
        .as_ref()
        .and_then(|profile| profile.language_code_override.clone())
        .unwrap_or_else(|| {
            crate::lang_detect::detect_language(&text).unwrap_or_else(|| "eng".to_string())
        });
    let tl = Language::from_639_3(&detected_code)
        .and_then(|l| l.to_639_1())
        .unwrap_or("en");
    let tts_speed = if speed == "Slow" { "0.75" } else { "1" };

    for piece in split_google_tts_text(&text, GOOGLE_TTS_MAX_CHARS) {
        if request.is_cancelled(&manager) {
            break;
        }
        let mp3_data = match fetch_google_tts_mp3(&piece, tl, tts_speed) {
            Ok(data) => data,
            Err(reason) => {
                fail_request(PROVIDER, hwnd, &tx, reason);
                return;
            }
        };
        let mut source_sample_rate = 24000u32;
        let mut samples: Vec<i16> = Vec::new();
        if !super::audio_utils::decode_mp3_to_pcm(
            mp3_data,
            &mut samples,
            &mut source_sample_rate,
            || request.is_cancelled(&manager),
        ) {
            // Interrupted mid-decode.
            break;
        }
        if !send_pcm_samples(&manager, &request, &tx, samples, source_sample_rate) {
            break;
        }
    }
    let _ = tx.send(AudioEvent::End);
}

fn fetch_google_tts_mp3(text: &str, tl: &str, tts_speed: &str) -> Result<Vec<u8>, String> {
    let url = format!(
        "https://translate.google.com/translate_tts?ie=UTF-8&q={}&tl={}&ttsspeed={}&client=tw-ob",
        urlencoding::encode(text),
        tl,
        tts_speed
    );
    let resp = UREQ_AGENT
        .get(&url)
        .call()
        .map_err(|error| format!("Google TTS request failed: {error}"))?;
    let status = resp.status().as_u16();
    if status != 200 {
        return Err(format!("Google TTS HTTP {status}"));
    }
    let mut mp3_data = Vec::new();
    resp.into_body()
        .into_reader()
        .read_to_end(&mut mp3_data)
        .map_err(|error| format!("Google TTS response read failed: {error}"))?;
    Ok(mp3_data)
}

/// Splits text into pieces of at most `max_chars` characters, preferring
/// sentence ends, then whitespace, and hard-splitting only unbroken runs.
fn split_google_tts_text(text: &str, max_chars: usize) -> Vec<String> {
    let max_chars = max_chars.max(1);
    let mut pieces = Vec::new();
    let mut rest = text.trim();
    while !rest.is_empty() {
        if rest.chars().count() <= max_chars {
            pieces.push(rest.to_string());
            break;
        }
        let window_end = rest
            .char_indices()
            .nth(max_chars)
            .map_or(rest.len(), |(index, _)| index);
        let window = &rest[..window_end];
        let sentence_end = window
            .char_indices()
            .filter(|(_, ch)| matches!(ch, '.' | '!' | '?' | '。' | '！' | '？' | '\n'))
            .map(|(index, ch)| index + ch.len_utf8())
            .next_back();
        let split_at = sentence_end
            .or_else(|| {
                window
                    .char_indices()
                    .filter(|(index, ch)| ch.is_whitespace() && *index > 0)
                    .map(|(index, _)| index)
                    .next_back()
            })
            .unwrap_or(window_end);
        let piece = rest[..split_at].trim();
        if !piece.is_empty() {
            pieces.push(piece.to_string());
        }
        rest = rest[split_at..].trim_start();
    }
    pieces
}

#[cfg(test)]
mod tests {
    use super::split_google_tts_text;

    #[test]
    fn short_text_is_one_request() {
        assert_eq!(
            split_google_tts_text("  Hello world.  ", 200),
            ["Hello world."]
        );
        assert!(split_google_tts_text("   ", 200).is_empty());
    }

    #[test]
    fn long_paragraph_splits_at_sentences_under_the_limit() {
        let sentence = "This sentence is about forty characters. ";
        let text = sentence.repeat(12);
        let pieces = split_google_tts_text(&text, 200);
        assert!(pieces.len() > 1);
        for piece in &pieces {
            assert!(piece.chars().count() <= 200, "{piece}");
            assert!(piece.ends_with('.'), "{piece}");
        }
        assert_eq!(pieces.join(" "), text.trim());
    }

    #[test]
    fn falls_back_to_word_then_hard_boundaries() {
        let words = "word ".repeat(80);
        for piece in split_google_tts_text(&words, 200) {
            assert!(piece.chars().count() <= 200);
            assert!(!piece.starts_with(' ') && !piece.ends_with(' '));
            assert!(piece.split(' ').all(|word| word == "word"));
        }
        let unbroken = "가".repeat(450);
        let pieces = split_google_tts_text(&unbroken, 200);
        assert_eq!(
            pieces.iter().map(|p| p.chars().count()).collect::<Vec<_>>(),
            [200, 200, 50]
        );
    }
}
