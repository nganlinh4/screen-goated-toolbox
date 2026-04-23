use qwen3_asr_rs::streaming::StreamingTranscript;
use serde::Deserialize;
use serde_json::json;

pub(crate) const RUNTIME_ABI_VERSION: u32 = 2;
const MAX_RESUME_PREFIX_CHARS: usize = 240;

#[derive(Deserialize, Default)]
pub(crate) struct SessionConfig {
    #[serde(default = "default_sample_rate_hz")]
    pub(crate) sample_rate_hz: u32,
    #[serde(default = "default_chunk_ms")]
    pub(crate) chunk_size_ms: u32,
    #[serde(default = "default_unfixed_chunks")]
    pub(crate) unfixed_chunk_num: usize,
    #[serde(default = "default_unfixed_tokens")]
    pub(crate) unfixed_token_num: usize,
    #[serde(default)]
    pub(crate) language: String,
    #[serde(default)]
    pub(crate) resume_prefix_text: String,
}

fn default_sample_rate_hz() -> u32 {
    16_000
}

fn default_chunk_ms() -> u32 {
    2_000
}

fn default_unfixed_chunks() -> usize {
    2
}

fn default_unfixed_tokens() -> usize {
    5
}

fn shared_prefix_len(left: &str, right: &str) -> usize {
    let mut matched_bytes = 0usize;
    for ((left_idx, left_ch), (_, right_ch)) in left.char_indices().zip(right.char_indices()) {
        if left_ch != right_ch {
            break;
        }
        matched_bytes = left_idx + left_ch.len_utf8();
    }
    matched_bytes
}

fn join_transcript_segments(left: &str, right: &str) -> String {
    match (left.is_empty(), right.is_empty()) {
        (true, true) => String::new(),
        (true, false) => right.trim_start().to_string(),
        (false, true) => left.to_string(),
        (false, false) => {
            let left_has_space = left.chars().last().is_some_and(char::is_whitespace);
            let right_has_space = right.chars().next().is_some_and(char::is_whitespace);
            if left_has_space || right_has_space {
                format!("{left}{right}")
            } else {
                format!("{left} {right}")
            }
        }
    }
}

fn split_visible_and_local_text<'a>(visible_text: &'a str, context_prefix_text: &str) -> &'a str {
    if context_prefix_text.is_empty() {
        return visible_text;
    }
    visible_text
        .strip_prefix(context_prefix_text)
        .unwrap_or(visible_text)
}

fn take_prefix(text: &str, byte_len: usize) -> String {
    if byte_len >= text.len() {
        return text.to_string();
    }
    text[..byte_len].to_string()
}

fn visible_session_text(transcript: &StreamingTranscript, context_prefix_text: &str) -> String {
    if transcript.text.is_empty() {
        return join_transcript_segments(context_prefix_text, &transcript.draft_text);
    }
    if context_prefix_text.is_empty() || transcript.text.starts_with(context_prefix_text) {
        return transcript.text.clone();
    }
    join_transcript_segments(context_prefix_text, &transcript.text)
}

pub(crate) fn compute_resume_prefix_text(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let char_count = trimmed.chars().count();
    if char_count <= MAX_RESUME_PREFIX_CHARS {
        return trimmed.to_string();
    }
    let mut start_byte = 0usize;
    for (index, (byte_idx, _)) in trimmed.char_indices().enumerate() {
        if index + MAX_RESUME_PREFIX_CHARS >= char_count {
            start_byte = byte_idx;
            break;
        }
    }
    let mut suffix = trimmed[start_byte..].trim_start().to_string();
    if let Some(leading_space_idx) = suffix.find(char::is_whitespace) {
        if leading_space_idx < 24 {
            suffix = suffix[leading_space_idx..].trim_start().to_string();
        }
    }
    suffix
}

fn local_transcript_fields(
    transcript: &StreamingTranscript,
    context_prefix_text: &str,
) -> (String, String, String) {
    let visible_text = visible_session_text(transcript, context_prefix_text);
    let local_text = split_visible_and_local_text(&visible_text, context_prefix_text).to_string();
    let context_bytes = visible_text
        .strip_prefix(context_prefix_text)
        .map(|_| context_prefix_text.len())
        .unwrap_or(0);
    let fixed_visible_bytes = shared_prefix_len(&visible_text, &transcript.fixed_text);
    let local_fixed_bytes = fixed_visible_bytes.saturating_sub(context_bytes);
    let local_fixed = take_prefix(&local_text, local_fixed_bytes);
    let local_draft = local_text
        .strip_prefix(&local_fixed)
        .map(ToOwned::to_owned)
        .unwrap_or_default();
    (local_text, local_fixed, local_draft)
}

pub(crate) fn error_payload(message: impl AsRef<str>) -> String {
    json!({ "error": message.as_ref() }).to_string()
}

pub(crate) fn result_payload(
    transcript: &StreamingTranscript,
    context_prefix_text: &str,
    session_epoch: u64,
    audio_samples: usize,
    is_final: bool,
    latency_ms: u128,
    kv_cache_bytes: usize,
    kv_cache_dense_bytes: usize,
) -> String {
    let visible_text = visible_session_text(transcript, context_prefix_text);
    let resume_prefix_text = compute_resume_prefix_text(&visible_text);
    let (local_text, local_fixed, local_draft) =
        local_transcript_fields(transcript, context_prefix_text);
    json!({
        "language": transcript.language,
        "text": local_text,
        "fixed_text": local_fixed,
        "draft_text": local_draft,
        "session_epoch": session_epoch,
        "context_prefix_text": context_prefix_text,
        "resume_prefix_text": resume_prefix_text,
        "latency_ms": latency_ms,
        "audio_samples": audio_samples,
        "is_final": is_final,
        "kv_cache_bytes": kv_cache_bytes,
        "kv_cache_dense_bytes": kv_cache_dense_bytes,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::{local_transcript_fields, result_payload};
    use qwen3_asr_rs::streaming::StreamingTranscript;

    #[test]
    fn local_transcript_fields_do_not_strip_partial_context_matches() {
        let transcript = StreamingTranscript {
            text: "then the next bit".to_string(),
            fixed_text: "then".to_string(),
            draft_text: " the next bit".to_string(),
            ..StreamingTranscript::default()
        };

        let (local_text, local_fixed, local_draft) =
            local_transcript_fields(&transcript, "the previous suffix");
        assert_eq!(local_text, "then the next bit");
        assert_eq!(local_fixed, "then");
        assert_eq!(local_draft, " the next bit");
    }

    #[test]
    fn result_payload_keeps_local_fixed_text_when_context_only_partially_matches() {
        let transcript = StreamingTranscript {
            text: "then the next bit".to_string(),
            fixed_text: "then".to_string(),
            draft_text: " the next bit".to_string(),
            ..StreamingTranscript::default()
        };

        let payload =
            result_payload(&transcript, "the previous suffix", 2, 0, false, 0, 0, 0);
        assert!(payload.contains(r#""fixed_text":"then""#));
    }

    #[test]
    fn local_fixed_excludes_carried_context_prefix() {
        let transcript = StreamingTranscript {
            text: "there was more".to_string(),
            fixed_text: "there".to_string(),
            draft_text: " was more".to_string(),
            ..StreamingTranscript::default()
        };

        let (local_text, local_fixed, local_draft) =
            local_transcript_fields(&transcript, "the");
        assert_eq!(local_text, "re was more");
        assert_eq!(local_fixed, "re");
        assert_eq!(local_draft, " was more");
    }
}
