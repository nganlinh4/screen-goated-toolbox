use serde_json::{Value, json};

pub(super) fn serialized_json_bytes(value: &Value) -> usize {
    serde_json::to_vec(value)
        .expect("research result must remain JSON-serializable")
        .len()
}

pub(super) fn fit_source_excerpt_bytes(
    mut sources: Vec<Value>,
    max_payload_bytes: usize,
) -> Vec<Value> {
    if sources.is_empty() {
        return sources;
    }
    let per_source = max_payload_bytes / sources.len();
    let remainder = max_payload_bytes % sources.len();
    for (index, source) in sources.iter_mut().enumerate() {
        let budget = per_source + usize::from(index < remainder);
        let excerpt = source.get("excerpt").and_then(Value::as_str).unwrap_or("");
        let bounded = take_json_payload_bytes(excerpt, budget);
        if let Some(object) = source.as_object_mut() {
            object.insert("excerpt".to_string(), Value::String(bounded));
        }
    }
    sources
}

fn take_json_payload_bytes(value: &str, max_bytes: usize) -> String {
    let mut output = String::new();
    let mut used = 0usize;
    for character in value.chars() {
        let cost = match character {
            '"' | '\\' | '\u{0008}' | '\t' | '\n' | '\u{000c}' | '\r' => 2,
            '\u{0000}'..='\u{001f}' => 6,
            character => character.len_utf8(),
        };
        if used.saturating_add(cost) > max_bytes {
            break;
        }
        output.push(character);
        used += cost;
    }
    debug_assert_eq!(
        used,
        serde_json::to_vec(&output)
            .expect("excerpt must serialize")
            .len()
            .saturating_sub(2)
    );
    output
}

pub(super) fn stabilize_serialized_byte_count(result: &mut Value) -> usize {
    let mut count = serialized_json_bytes(result);
    for _ in 0..8 {
        if let Some(object) = result.as_object_mut() {
            object.insert("model_visible_byte_count".to_string(), json!(count));
        }
        let next = serialized_json_bytes(result);
        if next == count {
            return count;
        }
        count = next;
    }
    unreachable!("serialized byte count did not converge")
}

pub(super) fn metadata_overflow_result(
    source_count: usize,
    metadata_bytes: usize,
    temporary_browser_effects: Value,
    source_diagnostics: Value,
) -> Value {
    let cleanup_complete = temporary_browser_effects
        .get("cleanup_complete")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut result = json!({
        "ok": false,
        "code": "ERR_RESEARCH_RESULT_METADATA_TOO_LARGE",
        "error": "research metadata exceeded the bounded model-visible result contract",
        "valid_source_count": source_count,
        "metadata_byte_count": metadata_bytes,
        "model_visible_byte_count": 0,
        "model_visible_byte_limit": super::MAX_MODEL_VISIBLE_BYTES,
        "read_only": true,
        "executed": true,
        "effect_may_have_occurred": true,
        "effect_verified": cleanup_complete,
        "temporary_browser_effects": temporary_browser_effects,
        "source_diagnostics": source_diagnostics,
    });
    let count = stabilize_serialized_byte_count(&mut result);
    debug_assert!(count <= super::MAX_MODEL_VISIBLE_BYTES);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escaped_payload_budget_counts_controls_quotes_and_unicode_exactly() {
        let source = json!({"excerpt": "\0\n\"\\한🙂tail"});
        let fitted = fit_source_excerpt_bytes(vec![source], 16);
        let excerpt = fitted[0]["excerpt"].as_str().unwrap();
        assert!(serde_json::to_vec(excerpt).unwrap().len() - 2 <= 16);
        assert_eq!(excerpt, "\0\n\"\\한");
    }

    #[test]
    fn metadata_overflow_preserves_the_already_executed_browser_effect() {
        let result = metadata_overflow_result(
            5,
            50_000,
            json!({"opened_count": 2, "closed_verified_count": 2, "cleanup_complete": true}),
            json!({"failed_source_count": 0, "errors": []}),
        );
        assert_eq!(result["executed"], true);
        assert_eq!(result["effect_may_have_occurred"], true);
        assert_eq!(result["effect_verified"], true);
        assert_eq!(result["temporary_browser_effects"]["opened_count"], 2);
        assert_eq!(
            result["model_visible_byte_count"],
            serialized_json_bytes(&result)
        );
    }
}
