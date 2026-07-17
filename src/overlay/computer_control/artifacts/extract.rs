//! Exact, anchor-bounded derivation of one text artifact from another.

use super::{TextArtifact, create_text, load_text, preview};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, Ordering};

const MAX_ANCHOR_CHARS: usize = 2_000;

pub(super) fn tool(args: &Value, cancel: &AtomicBool, dry: bool) -> Value {
    if cancel.load(Ordering::SeqCst) {
        return failure(
            "ERR_ARTIFACT_EXTRACTION_CANCELLED",
            "artifact extraction was cancelled before reading the source",
            json!({"retryable": true}),
        );
    }
    let id = args.get("id").and_then(Value::as_str).unwrap_or("");
    let Ok((source, text)) = load_text(id) else {
        return failure(
            "ERR_ARTIFACT_NOT_FOUND",
            &format!("artifact '{id}' not found"),
            json!({"retryable": true}),
        );
    };
    let start = match anchor_arg(args, "start_text") {
        Ok(anchor) => anchor,
        Err(error) => return error,
    };
    let end = match anchor_arg(args, "end_text") {
        Ok(anchor) => anchor,
        Err(error) => return error,
    };
    if start.is_none() && end.is_none() {
        return failure(
            "ERR_ARTIFACT_RANGE_REQUIRED",
            "extract_artifact needs start_text, end_text, or both",
            json!({"retryable": true}),
        );
    }
    let start_occurrence = match occurrence_arg(args, "start_occurrence") {
        Ok(value) => value,
        Err(error) => return error,
    };
    let end_occurrence = match occurrence_arg(args, "end_occurrence") {
        Ok(value) => value,
        Err(error) => return error,
    };
    let start_match = match start {
        Some(anchor) => match select_anchor(&text, anchor, 0, start_occurrence, "start") {
            Ok(found) => Some(found),
            Err(error) => return error,
        },
        None => None,
    };
    let include_start = args
        .get("include_start")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let include_end = args
        .get("include_end")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let range_start = start_match
        .map(|found| {
            if include_start {
                found.start
            } else {
                found.end
            }
        })
        .unwrap_or(0);
    let end_search_start = start_match.map(|found| found.end).unwrap_or(0);
    let end_match = match end {
        Some(anchor) => {
            match select_anchor(&text, anchor, end_search_start, end_occurrence, "end") {
                Ok(found) => Some(found),
                Err(error) => return error,
            }
        }
        None => None,
    };
    let range_end = end_match
        .map(|found| if include_end { found.end } else { found.start })
        .unwrap_or(text.len());
    if range_start > range_end {
        return failure(
            "ERR_ARTIFACT_RANGE_REVERSED",
            "the selected end boundary is before the selected start boundary",
            json!({"range_start_byte": range_start, "range_end_byte": range_end}),
        );
    }
    let selected = &text[range_start..range_end];
    if selected.is_empty() {
        return failure(
            "ERR_ARTIFACT_RANGE_EMPTY",
            "the selected artifact range is empty",
            json!({"range_start_byte": range_start, "range_end_byte": range_end}),
        );
    }
    let range = json!({
        "start_byte": range_start,
        "end_byte": range_end,
        "byte_count": selected.len(),
        "char_count": selected.chars().count(),
        "start_match_count": start_match.map(|found| found.count).unwrap_or(0),
        "end_match_count": end_match.map(|found| found.count).unwrap_or(0),
    });
    if dry {
        return json!({
            "ok": true,
            "dry": true,
            "source_artifact_id": source.id(),
            "source_range": range,
            "preview": preview(selected),
        });
    }
    derived_response(source, selected, range)
}

fn derived_response(source: TextArtifact, selected: &str, range: Value) -> Value {
    let title = if source.title.is_empty() {
        "Extracted text range".to_string()
    } else {
        format!("{} range", source.title)
    };
    let source_url = (!source.source_url.is_empty()).then_some(source.source_url.as_str());
    let derived = match create_text("text_range", Some(&title), source_url, selected) {
        Ok(artifact) => artifact,
        Err(error) => {
            return failure(
                "ERR_ARTIFACT_EXTRACTION_WRITE",
                &format!("could not create extracted artifact: {error}"),
                json!({"retryable": true}),
            );
        }
    };
    json!({
        "ok": true,
        "source_artifact_id": source.id,
        "source_sha256": source.sha256,
        "source_range": range,
        "artifact": derived.response(selected),
        "effect_verified": true,
        "effect_may_have_occurred": false,
        "executed": true,
        "instruction": "Use the new artifact id for paste_artifact or save_artifact; the source artifact remains unchanged.",
    })
}

#[derive(Clone, Copy)]
struct AnchorMatch {
    start: usize,
    end: usize,
    count: usize,
}

fn select_anchor(
    text: &str,
    anchor: &str,
    search_start: usize,
    occurrence: Option<usize>,
    label: &str,
) -> Result<AnchorMatch, Value> {
    let matches: Vec<usize> = text[search_start..]
        .match_indices(anchor)
        .map(|(offset, _)| search_start + offset)
        .collect();
    if matches.is_empty() {
        return Err(failure(
            "ERR_ARTIFACT_ANCHOR_NOT_FOUND",
            &format!("{label}_text was not found exactly in the source artifact"),
            json!({"anchor": label, "match_count": 0, "retryable": true}),
        ));
    }
    let index = match occurrence {
        Some(value) if value <= matches.len() => value - 1,
        Some(value) => {
            return Err(failure(
                "ERR_ARTIFACT_OCCURRENCE_NOT_FOUND",
                &format!(
                    "requested {label}_occurrence {value}, but only {} match(es) exist",
                    matches.len()
                ),
                json!({"anchor": label, "match_count": matches.len(), "retryable": true}),
            ));
        }
        None if matches.len() == 1 => 0,
        None => {
            return Err(failure(
                "ERR_ARTIFACT_ANCHOR_AMBIGUOUS",
                &format!(
                    "{label}_text matched {} times; provide {label}_occurrence",
                    matches.len()
                ),
                json!({"anchor": label, "match_count": matches.len(), "retryable": true}),
            ));
        }
    };
    let start = matches[index];
    Ok(AnchorMatch {
        start,
        end: start + anchor.len(),
        count: matches.len(),
    })
}

fn anchor_arg<'a>(args: &'a Value, key: &str) -> Result<Option<&'a str>, Value> {
    let Some(anchor) = args.get(key).and_then(Value::as_str) else {
        return Ok(None);
    };
    if anchor.is_empty() {
        return Ok(None);
    }
    if anchor.chars().count() > MAX_ANCHOR_CHARS {
        return Err(failure(
            "ERR_ARTIFACT_ANCHOR_TOO_LONG",
            &format!("{key} exceeds {MAX_ANCHOR_CHARS} characters"),
            json!({"retryable": true}),
        ));
    }
    Ok(Some(anchor))
}

fn occurrence_arg(args: &Value, key: &str) -> Result<Option<usize>, Value> {
    let Some(raw) = args.get(key) else {
        return Ok(None);
    };
    let Some(value) = raw.as_u64().and_then(|value| usize::try_from(value).ok()) else {
        return Err(failure(
            "ERR_ARTIFACT_OCCURRENCE_INVALID",
            &format!("{key} must be a positive integer"),
            json!({"retryable": true}),
        ));
    };
    if value == 0 {
        return Err(failure(
            "ERR_ARTIFACT_OCCURRENCE_INVALID",
            &format!("{key} is 1-based and must be at least 1"),
            json!({"retryable": true}),
        ));
    }
    Ok(Some(value))
}

fn failure(code: &str, message: &str, details: Value) -> Value {
    let mut result = json!({
        "ok": false,
        "code": code,
        "error": message,
        "effect_verified": false,
        "effect_may_have_occurred": false,
        "executed": false,
    });
    if let (Some(target), Some(source)) = (result.as_object_mut(), details.as_object()) {
        target.extend(source.clone());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &TextArtifact, args: Value) -> Value {
        let mut args = args;
        args["id"] = json!(source.id());
        tool(&args, &AtomicBool::new(false), false)
    }

    #[test]
    fn exact_boundaries_create_a_new_subset_without_changing_the_source() {
        let text = "navigation\nOpening boundary\nbody α\nClosing boundary.\nrelated content";
        let source = create_text("test", Some("page"), None, text).unwrap();
        let result = run(
            &source,
            json!({
                "start_text": "Opening boundary",
                "end_text": "Closing boundary."
            }),
        );
        assert_eq!(result["ok"], true);
        let derived_id = result["artifact"]["id"].as_str().unwrap();
        let (_, derived) = load_text(derived_id).unwrap();
        assert_eq!(derived, "Opening boundary\nbody α\nClosing boundary.");
        assert_eq!(load_text(source.id()).unwrap().1, text);
    }

    #[test]
    fn ambiguous_anchor_fails_instead_of_guessing() {
        let source = create_text("test", None, None, "mark one mark two").unwrap();
        let result = run(&source, json!({"start_text": "mark"}));
        assert_eq!(result["code"], "ERR_ARTIFACT_ANCHOR_AMBIGUOUS");
        assert_eq!(result["match_count"], 2);
        assert_eq!(result["executed"], false);
    }

    #[test]
    fn occurrence_and_exclusion_are_structural_and_one_based() {
        let source = create_text(
            "test",
            None,
            None,
            "start\nfirst\nend\nstart\nsecond\nend\ntail",
        )
        .unwrap();
        let result = run(
            &source,
            json!({
                "start_text": "start",
                "start_occurrence": 2,
                "include_start": false,
                "end_text": "end",
                "include_end": false
            }),
        );
        let (_, derived) = load_text(result["artifact"]["id"].as_str().unwrap()).unwrap();
        assert_eq!(derived, "\nsecond\n");
    }
}
