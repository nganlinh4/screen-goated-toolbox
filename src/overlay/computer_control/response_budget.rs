//! Bound model-facing tool responses without losing effect certainty.
use std::io::{self, Write};

use anyhow::{Result, bail};
use serde::Serialize;
use serde_json::{Value, json};

use super::effect_receipt::EffectStatus;

const MAX_WIRE_BYTES: usize = 64 * 1024;

#[derive(Serialize)]
struct Wire<'a> {
    #[serde(rename = "toolResponse")]
    tool_response: Responses<'a>,
}

#[derive(Serialize)]
struct Responses<'a> {
    #[serde(rename = "functionResponses")]
    function_responses: [Response<'a>; 1],
}

#[derive(Serialize)]
struct Response<'a> {
    id: &'a str,
    name: &'a str,
    response: &'a Value,
}

struct BoundedCount(usize);

impl Write for BoundedCount {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_WIRE_BYTES.saturating_sub(self.0) {
            return Err(io::Error::other("tool response exceeds wire budget"));
        }
        self.0 += bytes.len();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn fits(id: &str, name: &str, response: &Value) -> bool {
    let wire = Wire {
        tool_response: Responses {
            function_responses: [Response { id, name, response }],
        },
    };
    serde_json::to_writer(BoundedCount(0), &wire).is_ok()
}

pub(super) struct Prepared {
    pub wire: Value,
    pub delivered: Value,
    pub omitted: bool,
}

fn summary_effect(mutating: bool) -> EffectStatus {
    // An omitted response does not establish that nested fields are trusted receipts.
    if mutating {
        EffectStatus::MayHaveOccurred
    } else {
        EffectStatus::Unknown
    }
}

pub(super) fn prepare(id: &str, name: &str, response: Value) -> Result<Prepared> {
    let wire_omitted = !fits(id, name, &response);
    let source_omitted = name.starts_with("mcp__")
        && (response.pointer("/raw/omitted").and_then(Value::as_bool) == Some(true)
            || response
                .pointer("/action_result/raw/omitted")
                .and_then(Value::as_bool)
                == Some(true));
    let delivered = if wire_omitted || source_omitted {
        let mut summary = json!({
            "result_omitted": true,
            "reason": "The tool result exceeded a response size limit; its details were not delivered."
        });
        if source_omitted {
            let source = response.get("action_result").unwrap_or(&response);
            if let Some(content) = source.get("content").and_then(Value::as_str) {
                let mut chars = content.chars();
                summary["content"] = json!(chars.by_ref().take(4000).collect::<String>());
                summary["content_truncated"] = Value::Bool(
                    chars.next().is_some()
                        || source.get("content_truncated").and_then(Value::as_bool) == Some(true),
                );
            }
            summary["source_omitted"] = Value::Bool(true);
            if let Some(parts) = source.pointer("/raw/content_part_summary") {
                let mut counts = serde_json::Map::new();
                for key in [
                    "text_parts",
                    "image_parts",
                    "audio_parts",
                    "resource_parts",
                    "resource_link_parts",
                    "other_parts",
                    "known_text_utf8_bytes",
                    "known_encoded_data_bytes",
                ] {
                    if let Some(value) = parts.get(key).and_then(Value::as_u64) {
                        counts.insert(key.to_string(), json!(value));
                    }
                }
                summary["content_part_summary"] = Value::Object(counts);
            }
        }
        for key in [
            "ok",
            "execution_ok",
            "cancelled",
            "terminal_blocker",
            "silent_terminal_blocker",
            "final_response_required",
        ] {
            if let Some(value) = response
                .get(key)
                .or_else(|| {
                    response
                        .get("action_result")
                        .and_then(|inner| inner.get(key))
                })
                .and_then(Value::as_bool)
            {
                summary[key] = Value::Bool(value);
            }
        }
        if let Some(status) = response
            .get("status")
            .or_else(|| {
                response
                    .get("action_result")
                    .and_then(|inner| inner.get("status"))
            })
            .and_then(Value::as_str)
            .filter(|status| status.len() <= 128)
        {
            summary["status"] = json!(status);
        }
        let status = summary_effect(super::turn_policy::is_mutating_tool(name));
        status.annotate(&mut summary);
        summary
    } else {
        response
    };
    if !fits(id, name, &delivered) {
        bail!("tool response identity exceeds the wire budget");
    }
    let wire = super::protocol::tool_response(id, name, delivered.clone());
    Ok(Prepared {
        wire,
        delivered,
        omitted: wire_omitted || source_omitted,
    })
}

#[cfg(test)]
#[path = "response_budget_tests.rs"]
mod tests;
