use std::collections::HashSet;

use anyhow::Result;

/// Turn local class-agnostic proposals into a small semantic action set. The
/// ONNX model has one "UI element" class, so this independent pass is what
/// rejects headings/static text and names duplicate-looking controls.
pub(in crate::overlay::computer_control) fn label_clickable_marks(
    jpeg: &[u8],
    ids: &[u32],
) -> Result<Vec<(u32, String)>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let allowed = ids
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    let prompt = format!(
        "The image contains numbered cyan candidate marks. Candidate IDs: [{allowed}]. \
    Return ONLY a JSON object with a `marks` array containing the candidates whose MARK CENTER is on an enabled actionable control \
    that a user can operate by clicking. Exclude headings, ordinary/static text, decorative panels, \
    disabled controls, window borders, and empty background. Each item must be \
    {{\"id\": <candidate id>, \"label\": \"<exact visible control text plus the nearest container/state text needed to distinguish duplicates, max 14 words>\"}}. \
    Return {{\"marks\": []}} if none. Never invent an ID.",
    );
    let answer = super::run_chain_where(
        jpeg,
        &prompt,
        &[],
        Some(schema()),
        super::VisionTask::General,
        |answer| parse_labels(answer, ids).is_some(),
    )?;
    parse_labels(&answer, ids)
        .ok_or_else(|| anyhow::anyhow!("invalid actionable-mark JSON: {answer}"))
}

fn schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "marks": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "id": {"type": "integer"},
                        "label": {"type": "string"}
                    },
                    "required": ["id", "label"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["marks"],
        "additionalProperties": false
    })
}

fn parse_labels(answer: &str, ids: &[u32]) -> Option<Vec<(u32, String)>> {
    let items = match parse_json_value(answer)? {
        serde_json::Value::Object(mut object) => object.remove("marks")?.as_array()?.clone(),
        // Keep unconstrained provider fallbacks compatible while all providers
        // receive the object-root schema accepted by strict JSON decoders.
        serde_json::Value::Array(items) => items,
        _ => return None,
    };
    let allowed: HashSet<u32> = ids.iter().copied().collect();
    let mut seen = HashSet::new();
    let mut labels = Vec::new();
    for item in items {
        let id = u32::try_from(item.get("id")?.as_u64()?).ok()?;
        let label = item.get("label")?.as_str()?.trim();
        if label.is_empty() {
            return None;
        }
        if allowed.contains(&id) && seen.insert(id) {
            labels.push((id, label.chars().take(100).collect()));
        }
    }
    labels.sort_by_key(|(id, _)| *id);
    Some(labels)
}

fn parse_json_value(answer: &str) -> Option<serde_json::Value> {
    let trimmed = answer.trim();
    if let Ok(value) = serde_json::from_str(trimmed) {
        return Some(value);
    }
    let object = answer
        .find('{')
        .zip(answer.rfind('}'))
        .filter(|(start, end)| start < end)
        .and_then(|(start, end)| serde_json::from_str(&answer[start..=end]).ok());
    if object.is_some() {
        return object;
    }
    answer
        .find('[')
        .zip(answer.rfind(']'))
        .filter(|(start, end)| start < end)
        .and_then(|(start, end)| serde_json::from_str(&answer[start..=end]).ok())
}

#[cfg(test)]
mod tests {
    use super::{parse_labels, schema};

    #[test]
    fn provider_schema_has_an_object_root() {
        let schema = schema();
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["properties"]["marks"]["type"], "array");
        assert_eq!(schema["required"], serde_json::json!(["marks"]));
    }

    #[test]
    fn parser_rejects_unknown_and_duplicate_ids() {
        let parsed = parse_labels(
            r#"[{"id":2,"label":"Save button"},{"id":99,"label":"fake"},{"id":2,"label":"duplicate"}]"#,
            &[1, 2, 3],
        )
        .unwrap();
        assert_eq!(parsed, vec![(2, "Save button".to_string())]);
    }

    #[test]
    fn parser_accepts_a_valid_empty_set_but_not_prose() {
        assert_eq!(parse_labels(r#"{"marks":[]}"#, &[1]), Some(Vec::new()));
        assert_eq!(parse_labels("[]", &[1]), Some(Vec::new()));
        assert_eq!(parse_labels("none", &[1]), None);
    }

    #[test]
    fn parser_accepts_object_root_and_rejects_malformed_items() {
        assert_eq!(
            parse_labels(
                r#"```json
                {"marks":[{"id":3,"label":"Apply in settings"}]}
                ```"#,
                &[3],
            ),
            Some(vec![(3, "Apply in settings".to_string())])
        );
        assert_eq!(
            parse_labels(r#"{"marks":[{"id":3,"label":""}]}"#, &[3]),
            None
        );
        assert_eq!(parse_labels(r#"{"items":[]}"#, &[3]), None);
    }
}
