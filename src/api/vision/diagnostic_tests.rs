//! Opt-in paired OCR diagnostics; results never enter catalog history.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{TranslateImageRequest, translate_image_streaming};
use crate::api::client::{RequestTimeouts, UREQ_RESPONSE_AGENT, with_request_timeouts};
use crate::catalog_benchmark::setup::Credentials;

#[derive(Deserialize)]
struct Plan {
    models: Vec<Endpoint>,
    cases: Vec<Case>,
    variants: Vec<String>,
    repetitions: usize,
    spacing_ms: u64,
}

#[derive(Deserialize)]
struct Endpoint {
    provider: String,
    model: String,
}

#[derive(Deserialize)]
struct Case {
    id: String,
    image: PathBuf,
    prompt: String,
    reference: String,
}

#[test]
#[ignore = "requires OCR_DIAGNOSTIC_PLAN and OCR_DIAGNOSTIC_OUTPUT; calls providers"]
fn ocr_paired_diagnostic() -> Result<()> {
    let plan_path = std::env::var("OCR_DIAGNOSTIC_PLAN")?;
    let output_path = std::env::var("OCR_DIAGNOSTIC_OUTPUT")?;
    let plan: Plan = serde_json::from_reader(File::open(plan_path)?)?;
    ensure!(!plan.models.is_empty() && !plan.cases.is_empty());
    ensure!(plan.repetitions > 0 && plan.repetitions <= 10);
    for variant in &plan.variants {
        ensure!(
            matches!(
                variant.as_str(),
                "interactive-stream"
                    | "relaxed-stream"
                    | "relaxed-unary"
                    | "raw-stream"
                    | "raw-unary"
            ),
            "unknown diagnostic variant: {variant}"
        );
    }
    let credentials = Credentials::load()?;
    if plan.models.iter().any(|model| model.provider == "nvidia") {
        let feed = crate::model_feed::store::refresh()?;
        println!("OCR_DIAGNOSTIC feed_generated_at={}", feed.generated_at);
    }
    let models = crate::model_config::get_all_models_with_ollama();
    let config = crate::config::Config::default();
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output_path)?;
    for repetition in 0..plan.repetitions {
        for case in &plan.cases {
            let bytes = std::fs::read(&case.image)?;
            let image = crate::image_decode::load_from_memory(&bytes)?.to_rgba8();
            for endpoint in &plan.models {
                ensure!(
                    credentials.supports(&endpoint.provider),
                    "missing provider credential"
                );
                let model = models
                    .iter()
                    .find(|model| {
                        model.provider == endpoint.provider
                            && model.full_name == endpoint.model
                            && model.model_type == crate::model_config::ModelType::Vision
                    })
                    .context("diagnostic endpoint must resolve through the current registry")?;
                let timeouts = crate::retry_model_chain::interactive_request_timeouts(
                    &model.id,
                    &config,
                    crate::retry_model_chain::InteractiveRequestWorkload::for_input(
                        case.prompt.len(),
                        bytes.len(),
                    ),
                );
                // Keep one credential fixed across each paired comparison.
                credentials.with_provider_key(&endpoint.provider, |key| -> Result<()> {
                    for offset in 0..plan.variants.len() {
                        let variant = &plan.variants[(offset + repetition) % plan.variants.len()];
                        std::thread::sleep(Duration::from_millis(plan.spacing_ms));
                        let started = Instant::now();
                        let data = if variant.starts_with("raw-") {
                            raw_response(endpoint, case, &bytes, variant == "raw-stream", key)
                        } else {
                            let mut chunks = Vec::new();
                            let selected_timeouts = if variant == "interactive-stream" {
                                timeouts
                            } else {
                                RequestTimeouts::uniform(Duration::from_secs(30))
                            };
                            let result = translate_image_streaming(
                                TranslateImageRequest {
                                    groq_api_key: Credentials::groq_key_for(&endpoint.provider, key),
                                    gemini_api_key: key,
                                    prompt: case.prompt.clone(),
                                    model: endpoint.model.clone(),
                                    provider: endpoint.provider.clone(),
                                    image: image.clone(),
                                    original_bytes: Some(bytes.clone()),
                                    streaming_enabled: variant != "relaxed-unary",
                                    response_schema: None,
                                    cancel_token: None,
                                    request_timeout: Some(selected_timeouts),
                                },
                                |chunk| chunks.push(json!({"ms": started.elapsed().as_millis(), "text": chunk})),
                            );
                            Ok(json!({
                                "status": match &result {
                                    Ok(text) if text.trim().is_empty() => "empty",
                                    Ok(_) => "success",
                                    Err(_) => "error",
                                },
                                "response": result.as_ref().ok(),
                                "error": result.as_ref().err().map(ToString::to_string),
                                "first_token_budget_ms": selected_timeouts.first_token.as_millis(),
                                "total_budget_ms": selected_timeouts.total.as_millis(),
                                "chunks": chunks,
                            }))
                        };
                        let record = json!({
                            "repetition": repetition, "case": case.id,
                            "provider": endpoint.provider, "model": endpoint.model,
                            "variant": variant, "width": image.width(), "height": image.height(),
                            "input_bytes": bytes.len(), "reference": case.reference,
                            "elapsed_ms": started.elapsed().as_millis(),
                            "profile": crate::model_config::vision_request_profile(&endpoint.provider, &endpoint.model),
                            "result": data.unwrap_or_else(|error| json!({"status":"error", "error":error.to_string()})),
                        });
                        let serialized = serde_json::to_string(&record)?;
                        let serialized = if key.is_empty() { serialized } else { serialized.replace(key, "[redacted]") };
                        writeln!(output, "{serialized}")?;
                        output.flush()?;
                        println!("OCR_DIAGNOSTIC case={} model={} variant={} elapsed_ms={} status={}",
                            case.id, endpoint.model, variant, record["elapsed_ms"], record["result"]["status"]);
                    }
                    Ok(())
                })?;
            }
        }
    }
    Ok(())
}

fn raw_response(
    endpoint: &Endpoint,
    case: &Case,
    bytes: &[u8],
    streaming: bool,
    key: &str,
) -> Result<Value> {
    let image = crate::image_decode::load_from_memory(bytes)?.to_rgba8();
    let prepared = super::image_payload::prepare_image_payload(
        &endpoint.provider,
        &endpoint.model,
        image,
        Some(bytes.to_vec()),
        case.prompt.len(),
    )?;
    // The production builders own every request field. Raw capture isolates
    // response framing and parser behavior; it is not catalog timing evidence.
    let (url, payload) = match endpoint.provider.as_str() {
        "nvidia" => (
            crate::api::NVIDIA_CHAT_COMPLETIONS_URL,
            super::payloads::nvidia_vision_payload(
                &endpoint.model,
                &case.prompt,
                &prepared.mime_type,
                &prepared.b64_image,
                streaming,
                None,
            ),
        ),
        "groq" => (
            "https://api.groq.com/openai/v1/chat/completions",
            super::payloads::groq_vision_payload(
                &endpoint.model,
                &case.prompt,
                &prepared.mime_type,
                &prepared.b64_image,
                streaming,
                None,
            ),
        ),
        _ => anyhow::bail!("raw framing capture requires an OpenAI-compatible vision adapter"),
    };
    let request = UREQ_RESPONSE_AGENT
        .post(url)
        .header("Authorization", &format!("Bearer {key}"))
        .header("Content-Type", "application/json");
    let response = with_request_timeouts(
        request,
        Some(RequestTimeouts::uniform(Duration::from_secs(30))),
    )
    .send(serde_json::to_vec(&payload)?)?;
    let status = response.status().as_u16();
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let mut body = String::new();
    let read_error = response
        .into_body()
        .into_reader()
        .take(1024 * 1024)
        .read_to_string(&mut body)
        .err()
        .map(|error| error.to_string());
    Ok(
        json!({"status":"raw", "http_status":status, "content_type":content_type, "body":body, "read_error":read_error}),
    )
}
