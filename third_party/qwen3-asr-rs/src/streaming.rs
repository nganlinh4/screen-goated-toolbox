use crate::inference::{AsrInference, PromptPrefixCache, DEFAULT_STREAMING_MAX_NEW_TOKENS};
use anyhow::Result;
use std::collections::VecDeque;

const DEFAULT_CHUNK_SIZE_MS: u32 = 2_000;
const DEFAULT_UNFIXED_CHUNK_NUM: usize = 2;
const DEFAULT_UNFIXED_TOKEN_NUM: usize = 5;
const DEFAULT_FINAL_MAX_NEW_TOKENS: usize = 512;
const BACKEND_LOG_HISTORY: usize = 4;
const REPLACEMENT_CHARACTER: char = '\u{fffd}';

#[derive(Clone, Debug)]
pub struct StreamingConfig {
    pub chunk_size_ms: u32,
    pub unfixed_chunk_num: usize,
    pub unfixed_token_num: usize,
}

impl Default for StreamingConfig {
    fn default() -> Self {
        Self {
            chunk_size_ms: DEFAULT_CHUNK_SIZE_MS,
            unfixed_chunk_num: DEFAULT_UNFIXED_CHUNK_NUM,
            unfixed_token_num: DEFAULT_UNFIXED_TOKEN_NUM,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct StreamingTranscript {
    pub language: String,
    pub fixed_text: String,
    pub draft_text: String,
    pub text: String,
}

pub struct StreamingState {
    config: StreamingConfig,
    chunk_samples: usize,
    buffer: Vec<i16>,
    audio_accum: Vec<i16>,
    chunk_id: usize,
    raw_decoded: String,
    language: String,
    text: String,
    fixed_text: String,
    draft_text: String,
    cached_base_prefix: Option<PromptPrefixCache>,
    cached_base_language: Option<String>,
    audio_prefix_cache: Option<PromptPrefixCache>,
    ab_compared: bool,
    recent_backend_logs: VecDeque<String>,
}

impl StreamingState {
    pub fn new(config: StreamingConfig) -> Self {
        let chunk_samples = ((config.chunk_size_ms as usize) * 16_000 / 1_000).max(1);
        Self {
            config,
            chunk_samples,
            buffer: Vec::new(),
            audio_accum: Vec::new(),
            chunk_id: 0,
            raw_decoded: String::new(),
            language: String::new(),
            text: String::new(),
            fixed_text: String::new(),
            draft_text: String::new(),
            cached_base_prefix: None,
            cached_base_language: None,
            audio_prefix_cache: None,
            ab_compared: false,
            recent_backend_logs: VecDeque::with_capacity(BACKEND_LOG_HISTORY),
        }
    }

    pub fn append_pcm16(&mut self, samples: &[i16]) {
        if samples.is_empty() {
            return;
        }
        self.buffer.extend_from_slice(samples);
    }

    pub fn transcribe(
        &mut self,
        model: &AsrInference,
        language: Option<&str>,
    ) -> Result<StreamingTranscript> {
        while self.buffer.len() >= self.chunk_samples {
            let chunk: Vec<i16> = self.buffer.drain(..self.chunk_samples).collect();
            self.audio_accum.extend_from_slice(&chunk);
            self.extend_audio_prefix(model, language, &chunk)?;
            self.run_decode_step(model, language, DEFAULT_STREAMING_MAX_NEW_TOKENS)?;
            self.chunk_id += 1;
        }
        Ok(self.snapshot(false))
    }

    pub fn finish(
        &mut self,
        model: &AsrInference,
        language: Option<&str>,
    ) -> Result<StreamingTranscript> {
        if !self.buffer.is_empty() {
            let tail = std::mem::take(&mut self.buffer);
            self.audio_accum.extend_from_slice(&tail);
            self.extend_audio_prefix(model, language, &tail)?;
            self.run_decode_step(model, language, DEFAULT_FINAL_MAX_NEW_TOKENS)?;
            self.chunk_id += 1;
        }
        Ok(self.snapshot(true))
    }

    pub fn kv_cache_bytes(&self) -> usize { 0 }
    pub fn kv_cache_dense_bytes(&self) -> usize { 0 }

    pub fn reset(&mut self) {
        self.buffer.clear();
        self.audio_accum.clear();
        self.chunk_id = 0;
        self.raw_decoded.clear();
        self.language.clear();
        self.text.clear();
        self.fixed_text.clear();
        self.draft_text.clear();
        self.cached_base_prefix = None;
        self.cached_base_language = None;
        self.audio_prefix_cache = None;
        self.ab_compared = false;
        self.recent_backend_logs.clear();
    }

    fn get_or_create_base_prefix(
        &mut self,
        model: &AsrInference,
        language: Option<&str>,
    ) -> Result<&PromptPrefixCache> {
        let lang_key = language.map(|s| s.to_string());
        if self.cached_base_prefix.is_none()
            || self.cached_base_language != lang_key
        {
            self.cached_base_prefix = Some(model.create_base_prefix_cache(language)?);
            self.cached_base_language = lang_key;
            self.audio_prefix_cache = None;
        }
        Ok(self.cached_base_prefix.as_ref().unwrap())
    }

    fn extend_audio_prefix(
        &mut self,
        model: &AsrInference,
        language: Option<&str>,
        chunk_audio: &[i16],
    ) -> Result<&PromptPrefixCache> {
        let chunk_f32: Vec<f32> = chunk_audio.iter().map(|s| *s as f32 / 32768.0).collect();
        let audio_embeds = model.encode_audio_samples(&chunk_f32)?;
        if self.audio_prefix_cache.is_none() {
            let base = self.get_or_create_base_prefix(model, language)?;
            self.audio_prefix_cache = Some(
                model.extend_prefix_cache_with_audio(base, &audio_embeds, language)?,
            );
        } else {
            let cache = self.audio_prefix_cache.take().unwrap();
            self.audio_prefix_cache = Some(
                model.extend_owned_prefix_cache_with_audio(cache, &audio_embeds, language)?,
            );
        }
        Ok(self.audio_prefix_cache.as_ref().unwrap())
    }

    fn run_decode_step(
        &mut self,
        model: &AsrInference,
        language: Option<&str>,
        max_new_tokens: usize,
    ) -> Result<()> {
        let prefix = self.build_prefix_text(model)?;
        // Incremental: encode only the latest chunk and extend the cached prefix.
        // The audio_prefix_cache was already extended in transcribe() before this call.
        let audio_prefix_cache = self.audio_prefix_cache.as_ref()
            .ok_or_else(|| anyhow::anyhow!("audio prefix cache not initialized"))?;
        let mut generation_prefix_cache =
            model.create_generation_prefix_cache(audio_prefix_cache, language)?;
        if !prefix.is_empty() {
            let prefix_token_ids = model.encode_text_tokens(&prefix)?;
            generation_prefix_cache = model.extend_generation_prefix_with_token_ids(
                &generation_prefix_cache,
                &prefix_token_ids,
            )?;
        }

        let result = model.continue_transcription_from_ref_generation_cache(
            &generation_prefix_cache,
            language.is_some(),
            self.audio_accum.len(),
            max_new_tokens,
        )?;

        let raw_decoded = if prefix.is_empty() {
            normalize_punctuation_spacing(&result.raw_output)
        } else {
            normalize_punctuation_spacing(&format!("{prefix}{}", result.raw_output))
        };
        let (parsed_language, parsed_text) =
            model.parse_streaming_raw_output(&raw_decoded, language);
        let (fixed_text, mut draft_text) = if prefix.is_empty() {
            (String::new(), parsed_text.clone())
        } else {
            let (_, parsed_fixed) = model.parse_streaming_raw_output(&prefix, language);
            split_fixed_and_draft(&parsed_text, &parsed_fixed)
        };
        // Always strip trailing sentence marks from draft text.
        // The model adds periods to "close" output at chunk boundaries.
        // Real periods survive into fixed_text when the next chunk confirms them.
        // This prevents the translation system from prematurely committing.
        let trimmed_draft = draft_text.trim_end();
        if trimmed_draft.ends_with('.') || trimmed_draft.ends_with('?') || trimmed_draft.ends_with('!') {
            draft_text = trimmed_draft
                .trim_end_matches(|c| c == '.' || c == '?' || c == '!')
                .to_string();
        }

        let backend_log = format!(
            "[QwenBackend] chunk_id={} audio_samples={} prefix_chars={} raw_output_tokens={} prefix_tail={:?} raw_output_tail={:?} raw_decoded_tail={:?} parsed_text_tail={:?} fixed_tail={:?} draft_tail={:?}",
            self.chunk_id,
            self.audio_accum.len(),
            prefix.chars().count(),
            result.raw_output_token_ids.len(),
            tail_preview(&prefix, 96),
            tail_preview(&result.raw_output, 96),
            tail_preview(&raw_decoded, 96),
            tail_preview(&parsed_text, 96),
            tail_preview(&fixed_text, 96),
            tail_preview(&draft_text, 96),
        );
        self.recent_backend_logs.push_back(backend_log);
        while self.recent_backend_logs.len() > BACKEND_LOG_HISTORY {
            self.recent_backend_logs.pop_front();
        }

        let ab_reason = if result.raw_output_token_ids.len() >= max_new_tokens
            && self.chunk_id >= self.config.unfixed_chunk_num
        {
            Some("max_new_tokens")
        } else if looks_suspicious(&result.raw_output) {
            Some("suspicious_output")
        } else {
            None
        };

        if !self.ab_compared && ab_reason.is_some() {
            for line in &self.recent_backend_logs {
                eprintln!("{line}");
            }
            match model.transcribe_pcm16(&self.audio_accum, language) {
                Ok(offline) => {
                    eprintln!(
                        "[QwenBackend] ab_reason={} chunk_id={} audio_samples={} streaming_raw_tail={:?} streaming_text_tail={:?} offline_raw_tail={:?} offline_text_tail={:?}",
                        ab_reason.unwrap_or("unknown"),
                        self.chunk_id,
                        self.audio_accum.len(),
                        tail_preview(&result.raw_output, 160),
                        tail_preview(&parsed_text, 160),
                        tail_preview(&offline.raw_output, 160),
                        tail_preview(&offline.text, 160),
                    );
                }
                Err(err) => {
                    eprintln!(
                        "[QwenBackend] ab_reason={} chunk_id={} audio_samples={} offline_error={:?}",
                        ab_reason.unwrap_or("unknown"),
                        self.chunk_id,
                        self.audio_accum.len(),
                        err.to_string(),
                    );
                }
            }
            self.ab_compared = true;
        }

        self.raw_decoded = raw_decoded;
        self.language = parsed_language;
        self.text = parsed_text;
        self.fixed_text = fixed_text;
        self.draft_text = draft_text;
        Ok(())
    }

    fn build_prefix_text(&self, model: &AsrInference) -> Result<String> {
        if self.chunk_id < self.config.unfixed_chunk_num || self.raw_decoded.is_empty() {
            return Ok(String::new());
        }

        let token_ids = model.encode_text_tokens(&self.raw_decoded)?;
        let mut rollback = self.config.unfixed_token_num;
        loop {
            let end_idx = token_ids.len().saturating_sub(rollback);
            if end_idx == 0 {
                return Ok(String::new());
            }
            let prefix = model.decode_text_tokens(&token_ids[..end_idx])?;
            if !prefix.contains(REPLACEMENT_CHARACTER) {
                return Ok(normalize_punctuation_spacing(&prefix));
            }
            rollback += 1;
        }
    }

    fn snapshot(&self, finalize: bool) -> StreamingTranscript {
        if finalize {
            return StreamingTranscript {
                language: self.language.clone(),
                fixed_text: self.text.clone(),
                draft_text: String::new(),
                text: self.text.clone(),
            };
        }

        StreamingTranscript {
            language: self.language.clone(),
            fixed_text: self.fixed_text.clone(),
            draft_text: self.draft_text.clone(),
            text: self.text.clone(),
        }
    }
}

/// Remove spaces before punctuation that the tokenizer round-trip introduces.
/// e.g., "hello , world" → "hello, world", "they 're" → "they're"
fn normalize_punctuation_spacing(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == ' ' && i + 1 < chars.len() {
            let next = chars[i + 1];
            // Skip space before punctuation or contractions
            if matches!(next, ',' | '.' | '!' | '?' | ':' | ';' | '\'' | '"' | ')' | ']') {
                i += 1;
                continue;
            }
        }
        result.push(chars[i]);
        i += 1;
    }
    result
}

fn split_fixed_and_draft(full_text: &str, fixed_candidate: &str) -> (String, String) {
    if fixed_candidate.is_empty() || full_text.is_empty() {
        return (String::new(), full_text.to_string());
    }

    let shared_prefix_len = shared_prefix_len(full_text, fixed_candidate);
    if shared_prefix_len == 0 {
        return (String::new(), full_text.to_string());
    }

    (
        full_text[..shared_prefix_len].to_string(),
        full_text[shared_prefix_len..].to_string(),
    )
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

fn tail_preview(text: &str, max_chars: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_chars {
        return text.to_string();
    }
    let tail: String = chars[chars.len().saturating_sub(max_chars)..].iter().collect();
    format!("...{tail}")
}

fn looks_suspicious(text: &str) -> bool {
    let lowered = text.to_lowercase();
    lowered.contains("it doesn't matter. it doesn't matter")
        || lowered.contains("and these. and these")
        || lowered.contains("i don't feel judged. and i don't feel judged")
        || lowered.contains("i can be bad at this, i can be bad at this")
        || has_repeated_word_ngram(&lowered, 4)
        || has_repeated_word_ngram(&lowered, 5)
}

fn has_repeated_word_ngram(text: &str, n: usize) -> bool {
    let words: Vec<String> = text
        .split(|c: char| !(c.is_alphanumeric() || c == '\''))
        .filter(|part| !part.is_empty())
        .map(|part| part.to_string())
        .collect();

    if words.len() < n * 2 {
        return false;
    }

    for start in 0..=words.len().saturating_sub(n) {
        let gram = &words[start..start + n];
        if gram.iter().all(|word| word.len() <= 2) {
            continue;
        }
        for other in start + n..=words.len().saturating_sub(n) {
            if words[other..other + n] == *gram {
                return true;
            }
        }
    }

    false
}

impl AsrInference {
    pub fn init_streaming_state(&self, config: StreamingConfig) -> StreamingState {
        StreamingState::new(config)
    }

    pub fn streaming_transcribe(
        &self,
        samples: &[i16],
        state: &mut StreamingState,
        language: Option<&str>,
    ) -> Result<StreamingTranscript> {
        state.append_pcm16(samples);
        state.transcribe(self, language)
    }

    pub fn finish_streaming_transcribe(
        &self,
        state: &mut StreamingState,
        language: Option<&str>,
    ) -> Result<StreamingTranscript> {
        state.finish(self, language)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_state(
        chunk_size_ms: u32,
        unfixed_chunk_num: usize,
        unfixed_token_num: usize,
    ) -> StreamingState {
        StreamingState::new(StreamingConfig {
            chunk_size_ms,
            unfixed_chunk_num,
            unfixed_token_num,
        })
    }

    #[test]
    fn appends_to_buffer_without_decoding() {
        let mut state = make_state(2_000, 2, 5);
        state.append_pcm16(&vec![1; 8_000]);
        assert_eq!(state.buffer.len(), 8_000);
        assert_eq!(state.audio_accum.len(), 0);
        assert_eq!(state.chunk_id, 0);
    }

    #[test]
    fn snapshot_uses_plain_text_during_live_updates() {
        let mut state = make_state(2_000, 2, 5);
        state.language = "english".to_string();
        state.text = "hello world".to_string();
        let transcript = state.snapshot(false);
        assert_eq!(transcript.fixed_text, "");
        assert_eq!(transcript.draft_text, "hello world");
        assert_eq!(transcript.text, "hello world");
    }

    #[test]
    fn snapshot_marks_finalize_as_fixed_text() {
        let mut state = make_state(2_000, 2, 5);
        state.language = "english".to_string();
        state.text = "hello world".to_string();
        let transcript = state.snapshot(true);
        assert_eq!(transcript.fixed_text, "hello world");
        assert_eq!(transcript.draft_text, "");
        assert_eq!(transcript.text, "hello world");
    }
}
