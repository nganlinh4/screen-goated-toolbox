//! Structural completion validation shared by streaming and unary adapters.

use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;
use std::io::BufRead;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

fn check_error(root: &Value) -> Result<()> {
    if let Some(error) = root.get("error").filter(|value| !value.is_null()) {
        let code = error
            .get("code")
            .map_or(String::new(), |value| format!(" {value}"));
        bail!(
            "PROVIDER_RESPONSE_INVALID:Provider response error{code}: {}",
            super::provider_error_message(200, &root.to_string())
        );
    }
    Ok(())
}

fn check_finish(choice: &Value) -> Result<bool> {
    match choice.get("finish_reason").filter(|value| !value.is_null()) {
        None => Ok(false),
        Some(value) if value.as_str() == Some("stop") => Ok(true),
        Some(value) if value.as_str() == Some("length") => {
            bail!("PROVIDER_RESPONSE_INVALID:Provider completion token limit reached")
        }
        Some(value) => {
            bail!("PROVIDER_RESPONSE_INVALID:Provider completion did not succeed: {value}")
        }
    }
}

fn first_choice(root: &Value) -> Option<&Value> {
    root.get("choices")?.as_array()?.iter().find(|choice| {
        choice
            .get("index")
            .is_none_or(|index| index.as_u64() == Some(0))
    })
}

pub fn parse_chat_completion(root: &Value) -> Result<String> {
    check_error(root)?;
    let choice = first_choice(root)
        .context("PROVIDER_RESPONSE_INVALID:Provider returned no output content")?;
    check_finish(choice)?;
    let content = choice
        .pointer("/message/content")
        .and_then(Value::as_str)
        .context("PROVIDER_RESPONSE_INVALID:Provider returned no output content")?;
    ensure!(
        !content.trim().is_empty(),
        "PROVIDER_RESPONSE_INVALID:Provider returned no output content"
    );
    Ok(content.to_owned())
}

pub(super) fn consume_stream<R: BufRead>(
    reader: R,
    cancel_token: &Option<Arc<AtomicBool>>,
    mut on_event: impl FnMut(&str, &str),
) -> Result<String> {
    let mut content = String::new();
    let mut complete = false;
    let mut done = false;
    let mut event = Vec::new();
    let mut dispatch = |event: &mut Vec<String>| -> Result<bool> {
        if event.is_empty() {
            return Ok(false);
        }
        let data = event.join("\n");
        event.clear();
        if data.trim() == "[DONE]" {
            done = true;
            return Ok(true);
        }
        let root: Value = serde_json::from_str(&data)
            .context("PROVIDER_RESPONSE_INVALID:Malformed provider stream event")?;
        check_error(&root)?;
        let choices = root.get("choices").and_then(Value::as_array).context(
            "PROVIDER_RESPONSE_INVALID:Malformed provider stream event: missing choices",
        )?;
        if choices.is_empty() {
            return Ok(false);
        }
        if let Some(choice) = first_choice(&root) {
            let finished = check_finish(choice)?;
            let delta = choice
                .get("delta")
                .filter(|value| value.is_object())
                .context(
                    "PROVIDER_RESPONSE_INVALID:Malformed provider stream event: missing delta",
                )?;
            let text = delta
                .get("content")
                .filter(|value| !value.is_null())
                .map(|value| {
                    value
                        .as_str()
                        .context("PROVIDER_RESPONSE_INVALID:Malformed provider stream event: content is not text")
                })
                .transpose()?
                .unwrap_or("");
            let reasoning = delta
                .get("reasoning")
                .or_else(|| delta.get("reasoning_content"))
                .and_then(Value::as_str)
                .unwrap_or("");
            if complete {
                ensure!(
                    delta
                        .as_object()
                        .is_some_and(|fields| fields.iter().all(|(key, value)| {
                            key == "role"
                                || value.is_null()
                                || value.as_str() == Some("")
                                || value.as_array().is_some_and(Vec::is_empty)
                        })),
                    "PROVIDER_RESPONSE_INVALID:Provider emitted output after completion"
                );
                return Ok(false);
            }
            complete = finished;
            if !text.is_empty() {
                super::super::client::first_token::received();
                content.push_str(text);
            }
            on_event(text, reasoning);
        }
        Ok(false)
    };
    for line in reader.lines() {
        if cancel_token
            .as_ref()
            .is_some_and(|value| value.load(Ordering::Relaxed))
        {
            bail!("Cancelled");
        }
        let line = line?;
        if line.is_empty() {
            if dispatch(&mut event)? {
                break;
            }
        } else if let Some(data) = line.strip_prefix("data:") {
            event.push(data.strip_prefix(' ').unwrap_or(data).to_owned());
        }
    }
    if !event.is_empty() {
        dispatch(&mut event)?;
    }
    ensure!(
        complete || done,
        "PROVIDER_RESPONSE_INVALID:Provider stream ended before completion"
    );
    ensure!(
        !content.trim().is_empty(),
        "PROVIDER_RESPONSE_INVALID:Provider returned no output content"
    );
    Ok(content)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_completion_contract() {
        let fixture: Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/parity-fixtures/preset-system/chat-completion.json"
        )))
        .unwrap();
        for kind in ["stream", "unary"] {
            for case in fixture[kind].as_array().unwrap() {
                let result = if kind == "stream" {
                    consume_stream(
                        std::io::Cursor::new(case["body"].as_str().unwrap()),
                        &None,
                        |_, _| {},
                    )
                } else {
                    parse_chat_completion(&case["body"])
                };
                if let Some(expected) = case["output"].as_str() {
                    assert_eq!(result.unwrap(), expected, "{}", case["name"]);
                } else {
                    let error = result.unwrap_err().to_string();
                    assert!(
                        crate::overlay::utils::should_advance_retry_chain(&error),
                        "{}",
                        case["name"]
                    );
                    assert!(
                        error.contains(case["error"].as_str().unwrap()),
                        "{}",
                        case["name"]
                    );
                }
            }
        }
    }
}
