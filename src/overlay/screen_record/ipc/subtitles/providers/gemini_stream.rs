use std::io::BufRead;

use serde_json::Value;

use crate::api::client::UREQ_AGENT;

pub fn stream_gemini_text_chunks<F>(
    api_key: &str,
    url: &str,
    payload: Value,
    mut on_text: F,
) -> Result<String, String>
where
    F: FnMut(&str) -> Result<(), String>,
{
    let response = UREQ_AGENT
        .post(url)
        .header("x-goog-api-key", api_key)
        .send_json(payload)
        .map_err(super::gemini::map_gemini_request_error)?;

    let reader = std::io::BufReader::new(response.into_body().into_reader());
    let mut full_content = String::new();
    for line in reader.lines() {
        let line = line.map_err(|error| format!("Read Gemini subtitle stream: {error}"))?;
        if let Some(json_str) = line.strip_prefix("data: ") {
            if json_str.trim() == "[DONE]" {
                break;
            }
            if let Ok(chunk) = serde_json::from_str::<Value>(json_str) {
                for part in extract_text_parts(&chunk) {
                    full_content.push_str(part);
                    on_text(part)?;
                }
            }
        }
    }
    Ok(full_content)
}

pub fn extract_complete_segment_object_strings(text: &str) -> Result<Vec<String>, String> {
    let Some(array_start) = find_segments_array_start(text) else {
        return Ok(Vec::new());
    };

    let mut objects = Vec::new();
    let mut in_string = false;
    let mut escape = false;
    let mut object_depth = 0usize;
    let mut object_start: Option<usize> = None;

    for (offset, ch) in text[array_start..].char_indices() {
        let index = array_start + offset;
        if in_string {
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        match ch {
            '"' => in_string = true,
            '{' => {
                if object_depth == 0 {
                    object_start = Some(index);
                }
                object_depth += 1;
            }
            '}' => {
                if object_depth == 0 {
                    continue;
                }
                object_depth -= 1;
                if object_depth == 0
                    && let Some(start) = object_start.take()
                {
                    objects.push(text[start..index + ch.len_utf8()].to_string());
                }
            }
            ']' if object_depth == 0 => break,
            _ => {}
        }
    }

    Ok(objects)
}

fn extract_text_parts(chunk: &Value) -> Vec<&str> {
    chunk
        .get("candidates")
        .and_then(|value| value.as_array())
        .and_then(|candidates| candidates.first())
        .and_then(|candidate| candidate.get("content"))
        .and_then(|content| content.get("parts"))
        .and_then(|value| value.as_array())
        .map(|parts| {
            parts
                .iter()
                .filter_map(|part| part.get("text").and_then(|value| value.as_str()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn find_segments_array_start(text: &str) -> Option<usize> {
    let key_index = text.find("\"segments\"")?;
    text[key_index..]
        .find('[')
        .map(|offset| key_index + offset + 1)
}

#[cfg(test)]
mod tests {
    use super::extract_complete_segment_object_strings;

    #[test]
    fn extracts_complete_objects_from_partial_array() {
        let text = r#"{"segments":[{"start_ms":0,"end_ms":1000,"text":"Alpha"},{"start_ms":1000,"end_ms":2000,"text":"Beta"}"#;
        let objects =
            extract_complete_segment_object_strings(text).expect("expected partial parse");
        assert_eq!(objects.len(), 2);
        assert!(objects[0].contains("\"Alpha\""));
        assert!(objects[1].contains("\"Beta\""));
    }

    #[test]
    fn ignores_unopened_segment_array() {
        let objects = extract_complete_segment_object_strings(r#"{"foo":"bar"}"#)
            .expect("expected empty result");
        assert!(objects.is_empty());
    }
}
