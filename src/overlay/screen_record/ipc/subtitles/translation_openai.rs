use crate::api::client::{UREQ_RESPONSE_AGENT, record_groq_json_usage, record_usage_headers};

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
    if let Some(("groq", full_name)) = usage_endpoint {
        record_groq_json_usage(full_name, &root);
    }
    root.get("choices")
        .and_then(|value| value.as_array())
        .and_then(|items| items.first())
        .and_then(|value| value.get("message"))
        .and_then(|value| value.get("content"))
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
        .ok_or_else(|| format!("{label} subtitle translation returned no content"))
}
