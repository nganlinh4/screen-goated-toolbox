//! Bounded trusted content returned after a verified text-file commit.

use super::Replacement;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const EXACT_BYTES: usize = 4 * 1024;
const SAMPLE_COUNT: usize = 7;
const SAMPLE_BYTES: usize = 384;
const CONTEXT_SIDE_BYTES: usize = 96;
const RESULT_EDGE_BYTES: usize = 128;

pub(super) struct AppliedSpan {
    pub(super) replacement_index: usize,
    pub(super) start: usize,
    pub(super) end: usize,
}

pub(super) struct PostEditContent {
    pub(super) exact: Option<String>,
    pub(super) sample: Value,
    pub(super) replacements: Value,
    pub(super) truncated: bool,
}

pub(super) fn post_edit_content(
    text: &str,
    replacements: &[Replacement],
    spans: &[AppliedSpan],
) -> PostEditContent {
    let replacement_evidence = replacements
        .iter()
        .enumerate()
        .map(|(index, replacement)| replacement_proof(text, index, replacement, spans))
        .collect::<Vec<_>>();
    if text.len() <= EXACT_BYTES {
        return PostEditContent {
            exact: Some(text.to_string()),
            sample: Value::Null,
            replacements: Value::Array(replacement_evidence),
            truncated: false,
        };
    }
    PostEditContent {
        exact: None,
        sample: json!({
            "trusted_post_edit": true,
            "source_bytes": text.len(),
            "source_chars": text.chars().count(),
            "segments": sampled_segments(text),
        }),
        replacements: Value::Array(replacement_evidence),
        truncated: true,
    }
}

fn replacement_proof(
    text: &str,
    index: usize,
    replacement: &Replacement,
    spans: &[AppliedSpan],
) -> Value {
    let matching = spans
        .iter()
        .filter(|span| span.replacement_index == index)
        .collect::<Vec<_>>();
    let mut digest = Sha256::new();
    for span in &matching {
        digest.update((span.start as u64).to_le_bytes());
        digest.update((span.end as u64).to_le_bytes());
    }
    let mut selected = matching.first().copied().into_iter().collect::<Vec<_>>();
    if let Some(last) = matching.last().copied()
        && selected
            .first()
            .is_none_or(|first| first.start != last.start)
    {
        selected.push(last);
    }
    let result_exact =
        (replacement.new_text.len() <= RESULT_EDGE_BYTES * 2).then(|| replacement.new_text.clone());
    json!({
        "replacement_index": index,
        "occurrences": matching.len(),
        "expected_occurrences": replacement.expected_count,
        "old_text_sha256": sha256_hex(replacement.old_text.as_bytes()),
        "new_text_sha256": sha256_hex(replacement.new_text.as_bytes()),
        "new_text_bytes": replacement.new_text.len(),
        "new_text_chars": replacement.new_text.chars().count(),
        "result_exact": result_exact,
        "result_prefix": prefix(&replacement.new_text, RESULT_EDGE_BYTES),
        "result_suffix": suffix(&replacement.new_text, RESULT_EDGE_BYTES),
        "result_spans_sha256": digest.finalize().iter().map(|byte| format!("{byte:02x}")).collect::<String>(),
        "contexts": selected.into_iter().map(|span| span_context(text, span)).collect::<Vec<_>>(),
        "trusted_post_edit": true,
    })
}

fn span_context(text: &str, span: &AppliedSpan) -> Value {
    let left_start = next_boundary(text, span.start.saturating_sub(CONTEXT_SIDE_BYTES));
    let right_end = previous_boundary(
        text,
        span.end.saturating_add(CONTEXT_SIDE_BYTES).min(text.len()),
        span.end,
    );
    json!({
        "result_byte_start": span.start,
        "result_byte_end": span.end,
        "left": &text[left_start..span.start],
        "right": &text[span.end..right_end],
    })
}

fn sampled_segments(text: &str) -> Vec<Value> {
    let width = SAMPLE_BYTES.min(text.len());
    let max_start = text.len().saturating_sub(width);
    let slots = SAMPLE_COUNT.min(max_start.saturating_add(1));
    (0..slots)
        .map(|slot| {
            let nominal = if slots <= 1 {
                0
            } else {
                slot * max_start / (slots - 1)
            };
            let start = next_boundary(text, nominal);
            let end = previous_boundary(text, (start + width).min(text.len()), start);
            json!({
                "byte_start": start,
                "byte_end": end,
                "text": &text[start..end],
            })
        })
        .collect()
}

fn prefix(text: &str, bytes: usize) -> &str {
    let end = previous_boundary(text, bytes.min(text.len()), 0);
    &text[..end]
}

fn suffix(text: &str, bytes: usize) -> &str {
    let start = next_boundary(text, text.len().saturating_sub(bytes));
    &text[start..]
}

fn next_boundary(text: &str, mut index: usize) -> usize {
    while index < text.len() && !text.is_char_boundary(index) {
        index += 1;
    }
    index
}

fn previous_boundary(text: &str, mut index: usize, floor: usize) -> usize {
    while index > floor && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}
