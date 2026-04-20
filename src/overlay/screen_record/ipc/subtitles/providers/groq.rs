use serde::Deserialize;

use crate::APP;
use crate::api::client::UREQ_AGENT;
use crate::model_config::get_model_by_id;

use super::{
    SubtitleBackend, SubtitleBackendProgress, ends_sentence, join_word_tokens,
    normalize_groq_language_hint, normalize_subtitle_text,
};
use crate::overlay::screen_record::ipc::subtitles::audio::MIN_SUBTITLE_DURATION_SEC;
use crate::overlay::screen_record::ipc::subtitles::types::CompactSubtitleSegment;

const GROQ_AUDIO_TRANSCRIPT_URL: &str = "https://api.groq.com/openai/v1/audio/transcriptions";
const SENTENCE_BREAK_SILENCE_SEC: f64 = 0.45;

pub struct GroqSubtitleBackend {
    api_key: String,
    model_name: String,
}

impl GroqSubtitleBackend {
    pub fn new() -> Result<Self, String> {
        let (api_key, model_name) = {
            let app = APP.lock().map_err(|_| "APP lock poisoned".to_string())?;
            let model = get_model_by_id("whisper-accurate")
                .ok_or_else(|| "whisper-accurate model config missing".to_string())?;
            (app.config.api_key.clone(), model.full_name)
        };
        if api_key.trim().is_empty() {
            return Err("NO_API_KEY:groq".to_string());
        }

        Ok(Self {
            api_key,
            model_name,
        })
    }
}

impl SubtitleBackend for GroqSubtitleBackend {
    fn transcribe_clip(
        &mut self,
        audio_data: Vec<u8>,
        language_hint: Option<&str>,
        _on_progress: &mut dyn FnMut(SubtitleBackendProgress) -> Result<(), String>,
    ) -> Result<Vec<CompactSubtitleSegment>, String> {
        let response = transcribe_with_groq_verbose(
            &self.api_key,
            &self.model_name,
            audio_data,
            language_hint,
        )?;
        Ok(build_sentence_blocks(&response))
    }
}

#[derive(Deserialize)]
struct GroqVerboseResponse {
    segments: Option<Vec<GroqSegment>>,
    words: Option<Vec<GroqWord>>,
}

#[derive(Clone, Deserialize)]
struct GroqSegment {
    start: f64,
    end: f64,
    text: String,
}

#[derive(Clone, Deserialize)]
struct GroqWord {
    start: f64,
    end: f64,
    word: String,
}

fn transcribe_with_groq_verbose(
    api_key: &str,
    model_name: &str,
    audio_data: Vec<u8>,
    language_hint: Option<&str>,
) -> Result<GroqVerboseResponse, String> {
    let boundary = format!("----SGTSubtitle{}", chrono::Utc::now().timestamp_millis());
    let mut body = Vec::new();
    add_multipart_field(&mut body, &boundary, "model", model_name.as_bytes());
    add_multipart_field(&mut body, &boundary, "response_format", b"verbose_json");
    add_multipart_field(
        &mut body,
        &boundary,
        "timestamp_granularities[]",
        b"segment",
    );
    add_multipart_field(&mut body, &boundary, "timestamp_granularities[]", b"word");
    if let Some(language) = normalize_groq_language_hint(language_hint) {
        add_multipart_field(&mut body, &boundary, "language", language.as_bytes());
    }
    body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"file\"; filename=\"subtitle-source.wav\"\r\n",
    );
    body.extend_from_slice(b"Content-Type: audio/wav\r\n\r\n");
    body.extend_from_slice(&audio_data);
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(format!("--{}--\r\n", boundary).as_bytes());

    let response = UREQ_AGENT
        .post(GROQ_AUDIO_TRANSCRIPT_URL)
        .header("Authorization", &format!("Bearer {}", api_key))
        .header(
            "Content-Type",
            &format!("multipart/form-data; boundary={boundary}"),
        )
        .send(&body)
        .map_err(|e| format!("Groq subtitle request failed: {e}"))?;

    let json: serde_json::Value = response
        .into_body()
        .read_json()
        .map_err(|e| format!("Parse Groq subtitle response: {e}"))?;
    serde_json::from_value(json).map_err(|e| format!("Decode Groq verbose response: {e}"))
}

fn add_multipart_field(body: &mut Vec<u8>, boundary: &str, name: &str, value: &[u8]) {
    body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    body.extend_from_slice(
        format!("Content-Disposition: form-data; name=\"{}\"\r\n\r\n", name).as_bytes(),
    );
    body.extend_from_slice(value);
    body.extend_from_slice(b"\r\n");
}

fn build_sentence_blocks(response: &GroqVerboseResponse) -> Vec<CompactSubtitleSegment> {
    if let Some(words) = response.words.as_ref().filter(|words| !words.is_empty()) {
        return build_sentence_blocks_from_words(words);
    }

    response
        .segments
        .clone()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|segment| {
            let text = normalize_subtitle_text(&segment.text);
            if text.is_empty() {
                None
            } else {
                Some(CompactSubtitleSegment {
                    start_time: segment.start,
                    end_time: segment.end.max(segment.start + MIN_SUBTITLE_DURATION_SEC),
                    text,
                })
            }
        })
        .collect()
}

fn build_sentence_blocks_from_words(words: &[GroqWord]) -> Vec<CompactSubtitleSegment> {
    let mut blocks = Vec::new();
    let mut current_words: Vec<&GroqWord> = Vec::new();

    for word in words {
        if current_words.is_empty() {
            current_words.push(word);
            continue;
        }

        let previous = current_words[current_words.len() - 1];
        let gap = word.start - previous.end;
        let previous_text = normalize_subtitle_text(&join_word_tokens(
            &current_words
                .iter()
                .map(|entry| entry.word.as_str())
                .collect::<Vec<_>>(),
        ));

        if gap >= SENTENCE_BREAK_SILENCE_SEC || ends_sentence(&previous_text) {
            if let Some(segment) = finalize_word_block(&current_words) {
                blocks.push(segment);
            }
            current_words.clear();
        }

        current_words.push(word);
    }

    if let Some(segment) = finalize_word_block(&current_words) {
        blocks.push(segment);
    }

    blocks
}

fn finalize_word_block(words: &[&GroqWord]) -> Option<CompactSubtitleSegment> {
    let text = normalize_subtitle_text(&join_word_tokens(
        &words
            .iter()
            .map(|word| word.word.as_str())
            .collect::<Vec<_>>(),
    ));
    if text.is_empty() {
        return None;
    }
    let start_time = words.first()?.start;
    let end_time = words
        .last()?
        .end
        .max(start_time + MIN_SUBTITLE_DURATION_SEC);
    Some(CompactSubtitleSegment {
        start_time,
        end_time,
        text,
    })
}
