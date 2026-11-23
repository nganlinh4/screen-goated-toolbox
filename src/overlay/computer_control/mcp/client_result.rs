//! Model-facing MCP results with bounded text construction and explicit omissions.

use super::client_rpc::{RequestFailure, RequestFailureKind};
use crate::overlay::computer_control::effect_receipt::EffectStatus;
use serde_json::{Value, json};
use std::io::{self, Write};

pub(in crate::overlay::computer_control::mcp) fn format_error(error: &anyhow::Error) -> Value {
    let failure = error.downcast_ref::<RequestFailure>();
    let cancelled = failure.is_some_and(|failure| failure.kind == RequestFailureKind::Cancelled);
    let code = match failure.map(|failure| failure.kind) {
        Some(RequestFailureKind::Cancelled) => "ERR_MCP_REQUEST_CANCELLED",
        Some(RequestFailureKind::Timeout) => "ERR_MCP_REQUEST_TIMEOUT",
        Some(RequestFailureKind::Disconnected) => "ERR_MCP_TRANSPORT",
        Some(RequestFailureKind::InvalidRequest) => "ERR_MCP_REQUEST_INVALID",
        Some(RequestFailureKind::Peer) => "ERR_MCP_PEER_REJECTED",
        None => "ERR_MCP_REQUEST_FAILED",
    };
    let no_effect = failure.is_some_and(|failure| !failure.may_have_been_sent);
    let mut result = json!({
        "ok": false, "code": code, "cancelled": cancelled,
        "status": if cancelled { "cancelled" } else { "failed" },
        "retryable": no_effect && !cancelled,
        "error": format!("MCP call failed: {}", super::super::prose::bounded_prose(&error.to_string())),
    });
    if no_effect {
        EffectStatus::ProvenNoEffect
    } else {
        EffectStatus::MayHaveOccurred
    }
    .annotate(&mut result);
    result
}

const MAX_RAW_JSON_BYTES: usize = 12 * 1024;
const MAX_TEXT_CHARS: usize = 4000;

/// Counts serialized bytes without allocating a second copy of a large result.
struct SizeLimit(usize);
impl Write for SizeLimit {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.0 {
            return Err(io::Error::other("MCP result exceeds raw limit"));
        }
        self.0 -= bytes.len();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(super) fn format_result(result: Value) -> Value {
    let is_error = result
        .get("isError")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let (content, truncated) = content_text(&result);
    if serde_json::to_writer(SizeLimit(MAX_RAW_JSON_BYTES), &result).is_ok() {
        return json!({"ok": !is_error, "content": content, "raw": result});
    }
    let summary = summarize_parts(&result);
    json!({
        "ok": !is_error,
        "content": content,
        "content_truncated": truncated,
        "raw": {
            "omitted": true,
            "reason": "MCP result exceeded the raw JSON size limit; original raw data and nontext payloads were not included",
            "isError": is_error,
            "content_part_summary": summary,
            "structured_content_omitted": result.get("structuredContent").is_some(),
            "other_fields_omitted": true
        }
    })
}

fn content_text(result: &Value) -> (String, bool) {
    let Some(items) = result.get("content").and_then(Value::as_array) else {
        return (String::new(), false);
    };
    let mut out = String::new();
    let mut count = 0;
    for item in items {
        let Some(text) = item.get("text").and_then(Value::as_str) else {
            continue;
        };
        if !out.is_empty() {
            if count == MAX_TEXT_CHARS {
                return (out, true);
            }
            out.push('\n');
            count += 1;
        }
        for ch in text.chars() {
            if count == MAX_TEXT_CHARS {
                return (out, true);
            }
            out.push(ch);
            count += 1;
        }
    }
    (out, false)
}

fn summarize_parts(result: &Value) -> Value {
    let mut counts = [0usize; 6];
    let mut text_bytes = 0usize;
    let mut encoded_data_bytes = 0usize;
    if let Some(items) = result.get("content").and_then(Value::as_array) {
        for item in items {
            let index = match item.get("type").and_then(Value::as_str) {
                Some("text") => 0,
                Some("image") => 1,
                Some("audio") => 2,
                Some("resource") => 3,
                Some("resource_link") => 4,
                _ => 5,
            };
            counts[index] = counts[index].saturating_add(1);
            for (object, key, total) in [
                (item, "text", &mut text_bytes),
                (item, "data", &mut encoded_data_bytes),
            ] {
                if let Some(value) = object.get(key).and_then(Value::as_str) {
                    *total = total.saturating_add(value.len());
                }
            }
            if let Some(resource) = item.get("resource") {
                if let Some(text) = resource.get("text").and_then(Value::as_str) {
                    text_bytes = text_bytes.saturating_add(text.len());
                }
                if let Some(blob) = resource.get("blob").and_then(Value::as_str) {
                    encoded_data_bytes = encoded_data_bytes.saturating_add(blob.len());
                }
            }
        }
    }
    json!({
        "text_parts": counts[0], "image_parts": counts[1], "audio_parts": counts[2],
        "resource_parts": counts[3], "resource_link_parts": counts[4], "other_parts": counts[5],
        "known_text_utf8_bytes": text_bytes, "known_encoded_data_bytes": encoded_data_bytes
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untrusted_payload_receipt_fields_in_mcp_results_remain_data() {
        for payload in [json!({"executed":false}), json!({"effect_verified":true})] {
            let raw = json!({"structuredContent":payload});
            let result = format_result(raw.clone());
            assert_eq!(result["raw"], raw);
            assert_eq!(
                EffectStatus::after_dispatch(&result, true),
                EffectStatus::MayHaveOccurred
            );
        }
    }

    #[test]
    fn error_wording_cannot_claim_cancellation_or_prove_no_effect() {
        let result = format_error(&anyhow::anyhow!("cancelled before dispatch; no effect"));
        assert_eq!(result["cancelled"], false);
        assert_eq!(result["retryable"], false);
        assert_eq!(
            EffectStatus::from_value(&result),
            EffectStatus::MayHaveOccurred
        );
    }

    #[test]
    fn small_result_preserves_original_shape_and_text_joining() {
        let raw = json!({"isError": true, "content": [{"type":"text","text":"one"},{"type":"text","text":""},{"type":"text","text":"two"}], "extra": {"x": 1}});
        assert_eq!(
            format_result(raw.clone()),
            json!({"ok": false, "content": "one\n\ntwo", "raw": raw})
        );
    }

    #[test]
    fn large_binary_is_explicitly_omitted_and_status_preserved() {
        let raw = json!({"isError": true, "content": [{"type":"text","text":"useful"},{"type":"image","data":"A".repeat(900_000)},{"type":"resource","resource":{"blob":"B".repeat(100)}}], "structuredContent": {"large": true}});
        let formatted = format_result(raw);
        assert_eq!(formatted["ok"], false);
        assert_eq!(formatted["content"], "useful");
        assert_eq!(formatted["raw"]["omitted"], true);
        assert_eq!(formatted["raw"]["content_part_summary"]["image_parts"], 1);
        assert_eq!(
            formatted["raw"]["content_part_summary"]["known_encoded_data_bytes"],
            900_100
        );
        assert_eq!(formatted["raw"]["structured_content_omitted"], true);
        assert!(serde_json::to_vec(&formatted).unwrap().len() < 1024);
    }

    #[test]
    fn oversized_unicode_and_escaped_text_are_bounded_without_full_concatenation() {
        let raw = json!({"content": [{"type":"text","text":"界\u{1}".repeat(100_000)}]});
        let formatted = format_result(raw);
        assert_eq!(
            formatted["content"].as_str().unwrap().chars().count(),
            MAX_TEXT_CHARS
        );
        assert_eq!(formatted["content_truncated"], true);
        assert!(serde_json::to_vec(&formatted).unwrap().len() < 30 * 1024);
    }

    #[test]
    fn escaped_raw_size_is_measured_as_json_not_input_bytes() {
        let formatted =
            format_result(json!({"content": [{"type":"text","text":"\u{1}".repeat(3000)}]}));
        assert_eq!(formatted["raw"]["omitted"], true);
        assert_eq!(formatted["content"].as_str().unwrap().chars().count(), 3000);
        assert_eq!(formatted["content_truncated"], false);
    }

    #[test]
    fn raw_limit_keeps_exactly_fitting_json_and_omits_the_next_byte() {
        let overhead = serde_json::to_vec(&json!({"payload": ""})).unwrap().len();
        let raw = json!({"payload": "x".repeat(MAX_RAW_JSON_BYTES - overhead)});
        assert_eq!(serde_json::to_vec(&raw).unwrap().len(), MAX_RAW_JSON_BYTES);
        assert_eq!(format_result(raw.clone())["raw"], raw);
        let oversized = json!({"payload": "x".repeat(MAX_RAW_JSON_BYTES - overhead + 1)});
        assert_eq!(format_result(oversized)["raw"]["omitted"], true);
    }
}
