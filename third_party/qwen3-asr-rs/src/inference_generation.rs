use super::{AsrInference, ENDOFTEXT_TOKEN_ID, IM_END_TOKEN_ID};
use crate::tensor::Tensor;
use crate::text_decoder::DecoderState;
use anyhow::Result;

#[derive(Clone)]
pub(crate) struct PromptPrefixCache {
    pub(crate) state: DecoderState,
    pub(crate) rope: RopeCache,
}

#[derive(Clone)]
pub(crate) struct RopeCache {
    pub(crate) cos: Tensor,
    pub(crate) sin: Tensor,
}

impl RopeCache {
    pub(crate) fn len(&self) -> usize {
        self.cos.size()[0] as usize
    }

    pub(crate) fn slice(&self, start: usize, len: usize) -> (Tensor, Tensor) {
        (
            self.cos.narrow(0, start as i64, len as i64),
            self.sin.narrow(0, start as i64, len as i64),
        )
    }
}

pub(crate) struct GenerationSession<'a> {
    inference: &'a AsrInference,
    state: DecoderState,
    rope: RopeCache,
    next_logits: Tensor,
}

pub(crate) struct ParsedGeneration {
    pub(crate) language: String,
    pub(crate) raw_output: String,
    pub(crate) transcription: String,
    pub(crate) transcript_token_ids: Vec<i64>,
    pub(crate) transcript_generation_token_ids: Vec<i64>,
}

impl<'a> GenerationSession<'a> {
    pub(crate) fn from_prefilled_cache(
        inference: &'a AsrInference,
        prefix_cache: &PromptPrefixCache,
    ) -> Self {
        let state = prefix_cache.state.clone();
        let rope = prefix_cache.rope.clone();
        let next_logits = state
            .last_logits
            .as_ref()
            .expect("prefilled cache must include next logits")
            .shallow_clone();
        Self {
            inference,
            state,
            rope,
            next_logits,
        }
    }

    pub(crate) fn generate(&mut self, max_new_tokens: usize) -> Result<Vec<i64>> {
        let mut generated_ids = Vec::new();
        for _ in 0..max_new_tokens {
            let next_token = self.next_logits.argmax(-1, false).int64_value(&[0]);
            if [ENDOFTEXT_TOKEN_ID, IM_END_TOKEN_ID].contains(&next_token) {
                break;
            }

            generated_ids.push(next_token);
            let next_input = Tensor::from_slice_i64(&[next_token]).to_device(self.inference.device);
            let next_hidden = self.inference.text_decoder.embed(&next_input).unsqueeze(0);
            let (cos, sin) = self.rope.slice(self.state.next_position(), 1);
            self.next_logits = self
                .inference
                .text_decoder
                .decode_embedded(&next_hidden, &cos, &sin, &mut self.state)
                .squeeze_dim(1);
        }
        Ok(generated_ids)
    }
}

pub(crate) fn parse_language_prefix(raw: &str) -> String {
    let rest = raw.trim();
    if let Some(rest) = rest.strip_prefix("language ") {
        return rest.trim().to_string();
    }
    if rest.is_empty() {
        return "unknown".to_string();
    }
    rest.to_string()
}

pub(crate) fn parse_asr_output(raw: &str, language_forced: bool) -> (String, String) {
    if language_forced {
        return ("forced".to_string(), detect_and_fix_repetitions(raw.trim()));
    }

    let raw = detect_and_fix_repetitions(raw.trim());

    if let Some(rest) = raw.strip_prefix("language ") {
        if let Some(asr_pos) = rest.find("<asr_text>") {
            let lang = rest[..asr_pos].trim().to_string();
            let text = rest[asr_pos + "<asr_text>".len()..].trim().to_string();
            return (lang, text);
        }
        let mut lang_end = 0;
        for (i, c) in rest.char_indices() {
            if c.is_whitespace() || !c.is_alphabetic() {
                lang_end = i;
                break;
            }
            lang_end = i + c.len_utf8();
        }
        if lang_end > 0 {
            let lang = rest[..lang_end].to_string();
            let text = rest[lang_end..].trim().to_string();
            return (lang, text);
        }
    }

    ("unknown".to_string(), raw.to_string())
}

fn detect_and_fix_repetitions(text: &str) -> String {
    const THRESHOLD: usize = 20;
    const MAX_PATTERN_LEN: usize = 20;

    let without_char_repeats = fix_char_repeats(text, THRESHOLD);
    fix_pattern_repeats(&without_char_repeats, THRESHOLD, MAX_PATTERN_LEN)
}

fn fix_char_repeats(text: &str, threshold: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut output = String::new();
    let mut i = 0usize;

    while i < chars.len() {
        let mut count = 1usize;
        while i + count < chars.len() && chars[i + count] == chars[i] {
            count += 1;
        }

        if count > threshold {
            output.push(chars[i]);
        } else {
            for ch in &chars[i..i + count] {
                output.push(*ch);
            }
        }
        i += count;
    }

    output
}

fn fix_pattern_repeats(text: &str, threshold: usize, max_len: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    let min_repeat_chars = threshold * 2;
    if chars.len() < min_repeat_chars {
        return text.to_string();
    }

    let mut i = 0usize;
    let mut result = String::new();
    let mut found = false;

    while i + min_repeat_chars <= chars.len() {
        let mut matched_pattern = false;

        for k in 1..=max_len {
            if i + (k * threshold) > chars.len() {
                break;
            }

            let pattern = &chars[i..i + k];
            let valid = (1..threshold).all(|rep| {
                let start_idx = i + (rep * k);
                &chars[start_idx..start_idx + k] == pattern
            });

            if !valid {
                continue;
            }

            let mut end_index = i + (threshold * k);
            while end_index + k <= chars.len() && &chars[end_index..end_index + k] == pattern {
                end_index += k;
            }

            for ch in pattern {
                result.push(*ch);
            }
            let suffix: String = chars[end_index..].iter().collect();
            result.push_str(&fix_pattern_repeats(&suffix, threshold, max_len));
            found = true;
            matched_pattern = true;
            i = chars.len();
            break;
        }

        if matched_pattern {
            break;
        }

        result.push(chars[i]);
        i += 1;
    }

    if !found {
        for ch in &chars[i..] {
            result.push(*ch);
        }
    }

    result
}

pub(crate) fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}
