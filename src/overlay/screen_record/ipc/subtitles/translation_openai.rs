use std::sync::{Arc, atomic::AtomicBool};

use crate::api::client::{UREQ_RESPONSE_AGENT, record_groq_json_usage, record_usage_headers};

#[cfg(test)]
#[path = "../../../../api/http_test_support.rs"]
mod http_test_support;

/// Post a prepared OpenAI-compatible payload and extract the first message.
pub(super) fn post_openai_compat_chat(
    url: &str,
    api_key: &str,
    label: &str,
    payload: serde_json::Value,
    extra_headers: &[(&str, &str)],
    usage_endpoint: Option<(&str, &str)>,
) -> Result<String, String> {
    let mut request = UREQ_RESPONSE_AGENT
        .post(url)
        .header("Authorization", &format!("Bearer {api_key}"));
    for (name, value) in extra_headers {
        request = request.header(*name, *value);
    }
    let response = request
        .send_json(payload)
        .map_err(|error| format!("{label} subtitle translation transport failed: {error}"))?;
    if let Some((provider, full_name)) = usage_endpoint {
        record_usage_headers(provider, full_name, response.headers());
    }
    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.into_body().read_to_string().unwrap_or_default();
        return Err(format!(
            "{label} subtitle translation HTTP {status}: {body}"
        ));
    }
    let root: serde_json::Value = response
        .into_body()
        .read_json()
        .map_err(|error| format!("{label} subtitle translation JSON failed: {error}"))?;
    first_message_content(&root, label)
}

/// Post a Groq chat payload through the one Groq retry contract the host
/// uses: a provider-declared output allowance is adopted once, one short
/// rate-limit delay is waited out in place, and failures use the shared
/// Groq error mapping (a rejected key becomes `INVALID_API_KEY:groq`).
pub(super) fn post_groq_chat(
    endpoint: &str,
    api_key: &str,
    mut payload: serde_json::Value,
    cancelled: &Arc<AtomicBool>,
) -> Result<String, String> {
    let model = payload["model"].as_str().unwrap_or_default().to_owned();
    let response = crate::api::groq::send_with_retries(
        &mut payload,
        &Some(cancelled.clone()),
        true,
        |payload| {
            UREQ_RESPONSE_AGENT
                .post(endpoint)
                .header("Authorization", &format!("Bearer {api_key}"))
                .send_json(payload)
                .map_err(|error| {
                    anyhow::anyhow!("Groq subtitle translation transport failed: {error}")
                })
        },
        |_| {},
    )
    .map_err(|error| error.to_string())?;
    let root: serde_json::Value = response
        .into_body()
        .read_json()
        .map_err(|error| format!("Groq subtitle translation JSON failed: {error}"))?;
    record_groq_json_usage(&model, &root);
    first_message_content(&root, "Groq")
}

fn first_message_content(root: &serde_json::Value, label: &str) -> Result<String, String> {
    root.pointer("/choices/0/message/content")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| format!("{label} subtitle translation returned no content"))
}

#[cfg(test)]
mod tests {
    use super::http_test_support::scripted_server;
    use super::post_groq_chat;
    use std::sync::{Arc, atomic::AtomicBool};

    fn post(endpoint: &str) -> Result<String, String> {
        let payload = serde_json::json!({"model": "fixture-model", "messages": []});
        post_groq_chat(
            endpoint,
            "fixture-key",
            payload,
            &Arc::new(AtomicBool::new(false)),
        )
    }

    fn response(status: &str, body: &str) -> &'static str {
        let text = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        Box::leak(text.into_boxed_str())
    }

    #[test]
    fn groq_subtitle_translation_adopts_the_declared_output_allowance() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../parity-fixtures/preset-system/groq-output-allowance.json"
        ))
        .unwrap();
        let case = fixture["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["expected"].is_u64() && case["current"].is_null())
            .unwrap();
        let limited = response("429 Too Many Requests", &case["body"].to_string());
        let ok = response("200 OK", r#"{"choices":[{"message":{"content":"done"}}]}"#);
        let (endpoint, server) = scripted_server(vec![limited, ok]);
        assert_eq!(post(&endpoint).unwrap(), "done");
        let bodies = server.join().unwrap();
        assert_eq!(bodies.len(), 2);
        let retried: serde_json::Value = serde_json::from_str(&bodies[1]).unwrap();
        assert_eq!(retried["max_completion_tokens"], case["expected"]);
    }

    #[test]
    fn groq_subtitle_translation_reports_a_rejected_key_with_the_shared_signal() {
        let (endpoint, server) = scripted_server(vec![
            "HTTP/1.1 401 Unauthorized\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
        ]);
        assert_eq!(post(&endpoint).unwrap_err(), "INVALID_API_KEY:groq");
        server.join().unwrap();
    }
}
