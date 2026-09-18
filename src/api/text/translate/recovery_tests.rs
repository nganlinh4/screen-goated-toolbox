use super::*;

#[test]
#[ignore = "requires SGT_GROQ_PROBE_MODEL and credentials; calls the real text adapter"]
fn provider_allowance_recovery_live() -> Result<()> {
    let model = std::env::var("SGT_GROQ_PROBE_MODEL")?;
    let credentials = crate::catalog_benchmark::setup::Credentials::load()?;
    anyhow::ensure!(credentials.supports("groq"), "Groq credential required");
    let schema = serde_json::json!({"type":"object","properties":{"value":{"type":"string"}},"required":["value"],"additionalProperties":false});
    for (streaming, structured) in [(true, false), (false, false), (true, true)] {
        let started = std::time::Instant::now();
        let response = credentials.with_provider_key("groq", |key| {
            translate_text_streaming(
                TranslateTextRequest {
                    groq_api_key: key,
                    gemini_api_key: "",
                    text: "ready".into(),
                    instruction: if structured {
                        "Return only a JSON object with key value and the input as its value."
                    } else {
                        "Repeat the input exactly. Output nothing else."
                    }
                    .into(),
                    model: model.clone(),
                    provider: "groq".into(),
                    streaming_enabled: streaming,
                    use_json_format: false,
                    response_schema: structured
                        .then_some(TranslationSchema::LocallyValidated(&schema)),
                    max_output_tokens: Some(2048),
                    search_label: None,
                    ui_language: "en",
                    cancel_token: None,
                    request_timeout: Some(crate::api::client::RequestTimeouts::uniform(
                        std::time::Duration::from_secs(15),
                    )),
                    target_language: None,
                },
                |_| {},
            )
        })?;
        if structured {
            let value: serde_json::Value = serde_json::from_str(&response)?;
            anyhow::ensure!(value["value"] == "ready", "unexpected structured output");
        } else {
            anyhow::ensure!(response.trim() == "ready", "unexpected plain output");
        }
        println!(
            "GROQ_RECOVERY streaming={streaming} structured={structured} exact=true elapsed_ms={}",
            started.elapsed().as_millis()
        );
    }
    Ok(())
}
