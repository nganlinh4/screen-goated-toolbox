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
    audio_accum_f32: Vec<f32>,
    chunk_id: usize,
    raw_decoded: String,
    raw_decoded_token_ids: Vec<i64>,
    raw_decoded_prefix_token_count: usize,
    raw_decoded_prefix_text: String,
    raw_decoded_prefix_fixed_text: String,
    language: String,
    text: String,
    fixed_text: String,
    draft_text: String,
    kv_cache_bytes: usize,
    kv_cache_dense_bytes: usize,
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
            audio_accum_f32: Vec::new(),
            chunk_id: 0,
            raw_decoded: String::new(),
            raw_decoded_token_ids: Vec::new(),
            raw_decoded_prefix_token_count: 0,
            raw_decoded_prefix_text: String::new(),
            raw_decoded_prefix_fixed_text: String::new(),
            language: String::new(),
            text: String::new(),
            fixed_text: String::new(),
            draft_text: String::new(),
            kv_cache_bytes: 0,
            kv_cache_dense_bytes: 0,
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
            self.run_decode_step(model, language, DEFAULT_STREAMING_MAX_NEW_TOKENS, &chunk)?;
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
            self.run_decode_step(model, language, DEFAULT_FINAL_MAX_NEW_TOKENS, &tail)?;
            self.chunk_id += 1;
        }
        Ok(self.snapshot(true))
    }

    pub fn kv_cache_bytes(&self) -> usize { self.kv_cache_bytes }
    pub fn kv_cache_dense_bytes(&self) -> usize { self.kv_cache_dense_bytes }

    pub fn reset(&mut self) {
        self.buffer.clear();
        self.audio_accum.clear();
        self.audio_accum_f32.clear();
        self.chunk_id = 0;
        self.raw_decoded.clear();
        self.raw_decoded_token_ids.clear();
        self.raw_decoded_prefix_token_count = 0;
        self.raw_decoded_prefix_text.clear();
        self.raw_decoded_prefix_fixed_text.clear();
        self.language.clear();
        self.text.clear();
        self.fixed_text.clear();
        self.draft_text.clear();
        self.kv_cache_bytes = 0;
        self.kv_cache_dense_bytes = 0;
        self.cached_base_prefix = None;
        self.cached_base_language = None;
        self.audio_prefix_cache = None;
        self.ab_compared = false;
        self.recent_backend_logs.clear();
    }

    pub fn kv_cache_bytes(&self) -> usize {
        self.kv_cache_bytes
    }

    pub fn kv_cache_dense_bytes(&self) -> usize {
        self.kv_cache_dense_bytes
    }

    fn run_decode_step(
        &mut self,
        model: &AsrInference,
        language: Option<&str>,
        max_new_tokens: usize,
        recent_chunk: &[i16],
    ) -> Result<()> {
        let prefix = self.build_prefix_text(model)?;
        // Encode only the recent chunk and extend the cached audio prefix.
        // This is O(chunk_size) per step instead of O(total_audio).
        let chunk_f32: Vec<f32> = recent_chunk.iter().map(|s| *s as f32 / 32768.0).collect();
        let audio_embeds = model.encode_audio_samples(&chunk_f32)?;
        let audio_prefix_cache = self.refresh_audio_prefix(model, language, &audio_embeds)?;
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
            result.raw_output.clone()
        } else {
            format!("{prefix}{}", result.raw_output)
        };
        let (parsed_language, parsed_text) =
            model.parse_streaming_raw_output(&raw_decoded, language);
        let (fixed_text, draft_text) = if prefix.is_empty() {
            (String::new(), parsed_text.clone())
        } else {
            split_fixed_and_draft(&parsed_text, &prefix_fixed)
        };

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
        self.raw_decoded_prefix_token_count = prefix_token_count;
        self.raw_decoded_prefix_text = prefix.clone();
        self.raw_decoded_prefix_fixed_text = prefix_fixed;
        self.raw_decoded_token_ids = if prefix_token_ids.is_empty() {
            result.raw_output_token_ids.clone()
        } else {
            let mut token_ids = prefix_token_ids.to_vec();
            token_ids.extend_from_slice(&result.raw_output_token_ids);
            token_ids
        };
        self.language = parsed_language;
        self.text = parsed_text;
        self.fixed_text = fixed_text;
        self.draft_text = draft_text;
        self.kv_cache_bytes = result.kv_cache_bytes;
        self.kv_cache_dense_bytes = result.kv_cache_dense_bytes;
        Ok(())
    }

    fn get_or_create_base_prefix(
        &mut self,
        model: &AsrInference,
        language: Option<&str>,
    ) -> Result<&PromptPrefixCache> {
        let lang_key = language.map(|s| s.to_string());
        let needs_refresh = self.cached_base_prefix.is_none()
            || self.cached_base_language != lang_key;
        if needs_refresh {
            self.cached_base_prefix = Some(model.create_base_prefix_cache(language)?);
            self.cached_base_language = lang_key;
            self.audio_prefix_cache = None;
        }
        Ok(self.cached_base_prefix.as_ref().unwrap())
    }

    fn refresh_audio_prefix(
        &mut self,
        model: &AsrInference,
        language: Option<&str>,
        audio_embeds: &crate::tensor::Tensor,
    ) -> Result<&PromptPrefixCache> {
        if self.audio_prefix_cache.is_none() {
            // First chunk: create from base prefix + audio embeddings
            let base = self.get_or_create_base_prefix(model, language)?;
            self.audio_prefix_cache = Some(
                model.extend_prefix_cache_with_audio(base, audio_embeds, language)?,
            );
        } else {
            // Subsequent chunks: extend existing prefix with new audio embeddings
            let cache = self.audio_prefix_cache.take().unwrap();
            self.audio_prefix_cache = Some(
                model.extend_owned_prefix_cache_with_audio(cache, audio_embeds, language)?,
            );
        }
        Ok(self.audio_prefix_cache.as_ref().unwrap())
    }

    fn build_prefix_text(&self, model: &AsrInference) -> Result<String> {
        if self.chunk_id < self.config.unfixed_chunk_num || self.raw_decoded.is_empty() {
            return Ok(String::new());
        }

        if self.audio_prefix_cache.is_none() {
            let base_prefix_cache = self.base_prefix_cache(model, language)?;
            self.audio_prefix_cache = Some(
                model.extend_prefix_cache_with_audio(base_prefix_cache, audio_embeds, language)?,
            );
        } else {
            let cache = self
                .audio_prefix_cache
                .take()
                .expect("audio prefix cache must exist");
            self.audio_prefix_cache = Some(model.extend_owned_prefix_cache_with_audio(
                cache,
                audio_embeds,
                language,
            )?);
        }

        Ok(self
            .audio_prefix_cache
            .as_ref()
            .expect("audio prefix cache must exist after refresh"))
    }

    fn base_prefix_cache<'a>(
        &'a mut self,
        model: &AsrInference,
        language: Option<&str>,
    ) -> Result<&'a PromptPrefixCache> {
        let normalized_language = normalize_language(language);
        let cache_needs_refresh = self.base_prefix_cache.is_none()
            || self.base_prefix_language.as_deref() != normalized_language.as_deref();
        if cache_needs_refresh {
            let cache = model.create_base_prefix_cache(normalized_language.as_deref())?;
            self.base_prefix_language = normalized_language;
            self.base_prefix_cache = Some(cache);
        }
        Ok(self
            .base_prefix_cache
            .as_ref()
            .expect("base prefix cache must exist after refresh"))
    }

    fn suffix_token_ids<'a>(
        &'a mut self,
        model: &AsrInference,
        language: Option<&str>,
    ) -> Result<&'a [i64]> {
        let normalized_language = normalize_language(language);
        let cache_needs_refresh = self.suffix_language.as_deref() != normalized_language.as_deref();
        if cache_needs_refresh {
            self.suffix_token_ids = model.suffix_prompt_token_ids(normalized_language.as_deref())?;
            self.suffix_language = normalized_language;
        }
        Ok(&self.suffix_token_ids)
    }

    fn build_prefix_text_with_tokens(&self, model: &AsrInference, language: Option<&str>) -> Result<(String, String, usize)> {
        if self.chunk_id < self.config.unfixed_chunk_num || self.raw_decoded_token_ids.is_empty() {
            return Ok((String::new(), String::new(), 0));
        }

        let token_ids = self.raw_decoded_token_ids.as_slice();
        let max_end = token_ids.len().saturating_sub(self.config.unfixed_token_num);
        if max_end == 0 {
            return Ok((String::new(), String::new(), 0));
        }

        let mut low = 0usize;
        let mut best_prefix = String::new();
        let mut best_fixed = String::new();
        if self.raw_decoded_prefix_token_count > 0 && self.raw_decoded_prefix_token_count <= max_end {
            // The cached prefix comes from the same immutable leading token slice and stays
            // valid until those leading tokens change, which this streaming path does not do.
            low = self.raw_decoded_prefix_token_count;
            best_prefix = self.raw_decoded_prefix_text.clone();
            best_fixed = self.raw_decoded_prefix_fixed_text.clone();
        }

        if low == max_end {
            return Ok((best_prefix, best_fixed, low));
        }

        let mut high = max_end;
        while low < high {
            let probe = low + (high - low + 1) / 2;
            let prefix = model.decode_text_tokens(&token_ids[..probe])?;
            if prefix.contains(REPLACEMENT_CHARACTER) {
                high = probe.saturating_sub(1);
            } else {
                low = probe;
                best_prefix = prefix;
                best_fixed = model.parse_streaming_raw_output(&best_prefix, language).1;
            }
        }

        if low == 0 {
            return Ok((String::new(), String::new(), 0));
        }

        Ok((best_prefix, best_fixed, low))
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

fn normalize_language(language: Option<&str>) -> Option<String> {
    language.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

fn chunk_audio_f32(chunk: &[i16]) -> Vec<f32> {
    chunk.iter().map(|sample| *sample as f32 / 32768.0).collect()
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
