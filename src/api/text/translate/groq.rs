// --- GROQ TRANSLATE PROVIDERS ---
// Groq compound and standard API translation handlers.

use crate::api::client::record_groq_json_usage;
use anyhow::Result;
use std::io::BufReader;

use super::TranslateTransportOptions;

// --- GROQ COMPOUND MODEL ---
pub(super) fn translate_groq_compound<F>(
    groq_api_key: &str,
    model: &str,
    prompt: &str,
    search_label: Option<String>,
    transport: TranslateTransportOptions<'_>,
    on_chunk: F,
) -> Result<String>
where
    F: FnMut(&str),
{
    crate::api::groq::compound::run_compound(
        crate::api::groq::compound::CompoundRequest {
            endpoint: crate::api::groq::CHAT_COMPLETIONS_URL,
            api_key: groq_api_key,
            model,
            prompt,
            search_label,
            ui_language: transport.ui_language,
            request_timeout: transport.request_timeout,
            cancel_token: transport.cancel_token,
        },
        on_chunk,
    )
}

// --- GROQ STANDARD API ---
pub(super) fn translate_groq_standard<F>(
    groq_api_key: &str,
    model: &str,
    prompt: &str,
    use_json_format: bool,
    response_schema: Option<&serde_json::Value>,
    transport: TranslateTransportOptions<'_>,
    on_chunk: F,
) -> Result<String>
where
    F: FnMut(&str),
{
    let payload = standard_payload(model, prompt, use_json_format, response_schema, transport);
    send_standard_payload(groq_api_key, &payload, transport, on_chunk)
}

fn standard_payload(
    model: &str,
    prompt: &str,
    use_json_format: bool,
    response_schema: Option<&serde_json::Value>,
    transport: TranslateTransportOptions<'_>,
) -> serde_json::Value {
    let mut payload = serde_json::json!({
        "model": model,
        "messages": [
            { "role": "user", "content": prompt }
        ],
        "stream": transport.streaming_enabled
    });
    if let Some(schema) = response_schema {
        // Exact source-to-slot translation needs focused sampling, not the
        // endpoint's creative default. Leave ordinary free-form calls unchanged.
        payload["temperature"] = 0.into();
        // Groq JSON modes buffer content. Only callers that explicitly own
        // per-item validation and recovery may trade server enforcement for SSE.
        if !transport.streaming_enabled || !transport.locally_validated_schema {
            payload["response_format"] = crate::api::groq::structured_response_format(
                model,
                "translation_result",
                schema.clone(),
            );
        }
    } else if use_json_format {
        payload["response_format"] = serde_json::json!({ "type": "json_object" });
    }
    crate::api::apply_ordinary_openai_reasoning_policy(&mut payload, "groq", model);
    if transport.search_enabled {
        payload["tools"] = serde_json::json!([{ "type": "browser_search" }]);
        payload["tool_choice"] = "required".into();
    }
    if let Some(limit) = transport.max_output_tokens {
        payload["max_completion_tokens"] = limit.into();
    }

    payload
}

fn send_standard_payload<F>(
    groq_api_key: &str,
    payload: &serde_json::Value,
    transport: TranslateTransportOptions<'_>,
    on_chunk: F,
) -> Result<String>
where
    F: FnMut(&str),
{
    send_standard_payload_to(
        crate::api::groq::CHAT_COMPLETIONS_URL,
        groq_api_key,
        payload,
        transport,
        on_chunk,
    )
}

fn send_standard_payload_to(
    endpoint: &str,
    groq_api_key: &str,
    payload: &serde_json::Value,
    transport: TranslateTransportOptions<'_>,
    mut on_chunk: impl FnMut(&str),
) -> Result<String> {
    let model = payload["model"].as_str().unwrap_or_default();
    let mut request_payload = payload.clone();
    let _deadline = crate::api::client::first_token::FirstTokenGuard::new(
        transport.streaming_enabled,
        transport.request_timeout,
        transport.cancel_token,
    );
    let resp = crate::api::groq::send_chat(
        crate::api::groq::ChatSend {
            endpoint,
            api_key: groq_api_key,
            request_timeout: transport.request_timeout,
            cancel_token: transport.cancel_token,
            retry_short_rate_limit: transport.locally_validated_schema,
            transport_label: "Groq transport error",
        },
        &mut request_payload,
        |_| {},
    )?;

    let full_content;

    if transport.streaming_enabled {
        let reader = BufReader::new(resp.into_body().into_reader());
        full_content = crate::api::openai_compat::consume_content_stream(
            reader,
            transport.cancel_token,
            &mut on_chunk,
        )?;
    } else {
        let root: serde_json::Value = resp.into_body().read_json().map_err(|e| {
            anyhow::anyhow!(
                "PROVIDER_RESPONSE_INVALID:Malformed provider completion: {}",
                e
            )
        })?;
        record_groq_json_usage(model, &root);
        {
            let content_str = crate::api::openai_compat::parse_chat_completion(&root)?;

            full_content = content_str;

            on_chunk(&full_content);
        }
    }

    Ok(full_content)
}

#[cfg(test)]
#[path = "groq_recovery_tests.rs"]
mod recovery_tests;
#[cfg(test)]
#[path = "groq_replay_probe.rs"]
mod replay_probe;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_validation_is_an_explicit_streaming_only_choice() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../parity-fixtures/preset-system/text-provider-routing.json"
        ))
        .unwrap();
        let contract = &fixture["structured_text_contract"];
        assert_eq!(contract["default"], "provider-enforced");
        assert_eq!(
            contract["locally_validated_stream"]["requires_caller_validation_and_recovery"],
            true
        );
        let schema = serde_json::json!({"type":"object"});
        for streaming_enabled in [false, true] {
            for locally_validated_schema in [false, true] {
                let transport = TranslateTransportOptions {
                    locally_validated_schema,
                    streaming_enabled,
                    max_output_tokens: Some(512),
                    ui_language: "en",
                    cancel_token: &None,
                    request_timeout: None,
                    search_enabled: false,
                };
                let payload = standard_payload(
                    "openai/gpt-oss-20b",
                    "request",
                    true,
                    Some(&schema),
                    transport,
                );
                assert_eq!(
                    payload["temperature"],
                    contract["locally_validated_stream"]["temperature"]
                );
                assert_eq!(payload["max_completion_tokens"], 512);
                if streaming_enabled && locally_validated_schema {
                    assert_eq!(
                        payload["response_format"],
                        contract["locally_validated_stream"]["groq_response_format"]
                    );
                    assert!(payload.get("response_format").is_none());
                } else {
                    assert_eq!(payload["response_format"]["json_schema"]["strict"], true);
                }
                let ordinary =
                    standard_payload("openai/gpt-oss-20b", "request", false, None, transport);
                assert!(ordinary.get("temperature").is_none());
                assert!(ordinary.get("response_format").is_none());
                let legacy_json =
                    standard_payload("openai/gpt-oss-20b", "request", true, None, transport);
                assert_eq!(
                    legacy_json["response_format"],
                    serde_json::json!({"type":"json_object"})
                );
            }
        }
    }

    #[test]
    fn browser_search_is_only_in_explicit_groq_requests() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../parity-fixtures/preset-system/text-provider-routing.json"
        ))
        .unwrap();
        let contract = &fixture["ordinary_request_policy"];
        for streaming_enabled in [false, true] {
            let transport = TranslateTransportOptions {
                locally_validated_schema: false,
                max_output_tokens: None,
                streaming_enabled,
                ui_language: "en",
                cancel_token: &None,
                request_timeout: None,
                search_enabled: false,
            };
            let ordinary =
                standard_payload("openai/gpt-oss-20b", "request", false, None, transport);
            assert!(ordinary.get("tools").is_none());
            assert!(ordinary.get("tool_choice").is_none());

            let search = standard_payload(
                "openai/gpt-oss-20b",
                "request",
                false,
                None,
                TranslateTransportOptions {
                    search_enabled: true,
                    ..transport
                },
            );
            assert_eq!(
                search["tools"][0]["type"],
                contract["explicit_groq_search_tool"]
            );
            assert_eq!(search["tool_choice"], contract["groq_search_tool_choice"]);
            assert_eq!(search["reasoning_effort"], "low");
        }
    }
}
