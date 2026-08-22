//! Opt-in provider transport experiment.
//!
//! Probe outputs never enter catalog benchmark history. They exist to choose a
//! production request policy before comparable benchmark runs begin.

use std::fs::{File, create_dir_all};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use base64::{Engine as _, engine::general_purpose};
use serde::Serialize;
use serde_json::json;

use super::manifest::{Manifest, OcrCase};
use super::scoring;
use super::setup::{Credentials, Pacer};
use crate::api::gemini_generate::{
    GeminiGenerateRequest, GeminiMediaResolution, stream_gemini_generate_detailed,
};
use crate::model_config::{ModelConfig, ModelType, get_all_models};

#[derive(Clone, Copy, Debug)]
enum PromptOrder {
    TextFirst,
    ImageFirst,
}

#[derive(Clone, Copy, Debug)]
struct Variant {
    order: PromptOrder,
    resolution: Option<GeminiMediaResolution>,
    streaming: bool,
}

#[derive(Serialize)]
struct ProbeAttempt {
    model_id: String,
    api_model: String,
    case_id: String,
    difficulty: u8,
    variant: String,
    prompt: String,
    status: String,
    latency_ms: u128,
    score: Option<f64>,
    strict_pass: Option<bool>,
    response: Option<String>,
    usage_metadata: Option<serde_json::Value>,
    error: Option<String>,
}

pub fn run() -> Result<()> {
    let manifest = Manifest::load()?;
    manifest.validate()?;
    let credentials = Credentials::load()?;
    ensure!(
        credentials.supports("google"),
        "transport probe requires GEMINI_API_KEY"
    );
    let models = selected_google_models()?;
    let cases = selected_cases(&manifest)?;
    let variants = selected_variants()?;
    let timeout = super::setup::request_timeout()?.or(Some(Duration::from_secs(120)));
    let output = output_path();
    if let Some(parent) = output.parent() {
        create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let mut writer = BufWriter::new(
        File::create(&output).with_context(|| format!("create {}", output.display()))?,
    );
    let mut pacer = Pacer::from_env(&credentials)?;

    for case in cases {
        let image_path = manifest.image_path(&case.image);
        let (image, bytes) =
            super::runner::load_ocr_image(&image_path, case.crop_px, case.input_mode)?;
        let mime_type = sniff_mime_type(&bytes);
        let encoded = general_purpose::STANDARD.encode(&bytes);
        let prompt = std::env::var("CATALOG_BENCH_PROBE_PROMPT_OVERRIDE")
            .unwrap_or_else(|_| case.instruction.clone());
        for (variant_name, variant) in &variants {
            for model in &models {
                pacer.wait(model);
                let attempt = credentials.with_provider_key("google", |gemini_api_key| {
                    run_google(
                        model,
                        case,
                        mime_type,
                        &encoded,
                        &prompt,
                        *variant,
                        variant_name,
                        gemini_api_key,
                        timeout,
                    )
                });
                serde_json::to_writer(&mut writer, &attempt)?;
                writer.write_all(b"\n")?;
                writer.flush()?;
                println!(
                    "PROBE_RESULT case={} model={} variant={} status={} latency_ms={} score={:?} image={}x{}",
                    case.id,
                    model.id,
                    variant_name,
                    attempt.status,
                    attempt.latency_ms,
                    attempt.score,
                    image.width(),
                    image.height()
                );
            }
        }
    }
    println!("Catalog transport probe: {}", output.display());
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_google(
    model: &ModelConfig,
    case: &OcrCase,
    mime_type: &str,
    encoded: &str,
    prompt: &str,
    variant: Variant,
    variant_name: &str,
    api_key: &str,
    timeout: Option<Duration>,
) -> ProbeAttempt {
    let text = json!({ "text": prompt });
    let image = json!({
        "inline_data": {
            "mime_type": mime_type,
            "data": encoded
        }
    });
    let parts = match variant.order {
        PromptOrder::TextFirst => json!([text, image]),
        PromptOrder::ImageFirst => json!([image, text]),
    };
    let started = Instant::now();
    let result = stream_gemini_generate_detailed(
        GeminiGenerateRequest {
            parts,
            model: &model.full_name,
            api_key,
            streaming: variant.streaming,
            ui_language: "en",
            cancel_token: &None,
            error_label: None,
            map_auth_errors: true,
            request_timeout: timeout,
            response_schema: None,
            media_resolution: variant.resolution,
            retry_observer: None,
        },
        &mut |_| {},
    );
    let latency_ms = started.elapsed().as_millis();
    match result {
        Ok(output) if !output.content.trim().is_empty() => {
            let transcription = scoring::transcription(&output.content);
            let score = std::iter::once(&case.reference)
                .chain(&case.accepted_references)
                .map(|reference| scoring::ocr_similarity(&transcription, reference))
                .max_by(f64::total_cmp)
                .unwrap_or_default();
            ProbeAttempt {
                model_id: model.id.clone(),
                api_model: model.full_name.clone(),
                case_id: case.id.clone(),
                difficulty: case.difficulty,
                variant: variant_name.to_string(),
                prompt: prompt.to_string(),
                status: "success".to_string(),
                latency_ms,
                score: Some(score),
                strict_pass: Some(score >= 0.98),
                response: Some(output.content),
                usage_metadata: output.usage_metadata,
                error: None,
            }
        }
        Ok(output) => ProbeAttempt {
            model_id: model.id.clone(),
            api_model: model.full_name.clone(),
            case_id: case.id.clone(),
            difficulty: case.difficulty,
            variant: variant_name.to_string(),
            prompt: prompt.to_string(),
            status: "empty".to_string(),
            latency_ms,
            score: None,
            strict_pass: Some(false),
            response: Some(output.content),
            usage_metadata: output.usage_metadata,
            error: Some("provider returned an empty response".to_string()),
        },
        Err(error) => ProbeAttempt {
            model_id: model.id.clone(),
            api_model: model.full_name.clone(),
            case_id: case.id.clone(),
            difficulty: case.difficulty,
            variant: variant_name.to_string(),
            prompt: prompt.to_string(),
            status: "request_error".to_string(),
            latency_ms,
            score: None,
            strict_pass: Some(false),
            response: None,
            usage_metadata: None,
            error: Some(error.to_string()),
        },
    }
}

fn selected_google_models() -> Result<Vec<ModelConfig>> {
    let requested = comma_values("CATALOG_BENCH_PROBE_MODELS")?;
    let models = get_all_models()
        .iter()
        .filter(|model| {
            model.enabled
                && model.provider == "google"
                && model.model_type == ModelType::Vision
                && requested
                    .as_ref()
                    .is_none_or(|ids| ids.iter().any(|id| id == &model.id))
        })
        .cloned()
        .collect::<Vec<_>>();
    ensure!(
        !models.is_empty(),
        "no enabled Google vision models selected"
    );
    Ok(models)
}

fn selected_cases(manifest: &Manifest) -> Result<Vec<&OcrCase>> {
    let requested = comma_values("CATALOG_BENCH_PROBE_CASES")?
        .unwrap_or_else(|| vec!["3".to_string(), "4".to_string(), "10".to_string()]);
    let mut cases = Vec::new();
    for value in requested {
        let difficulty = value
            .parse::<u8>()
            .with_context(|| format!("parse probe case difficulty {value:?}"))?;
        let case = manifest
            .ocr_cases
            .iter()
            .find(|case| case.difficulty == difficulty)
            .with_context(|| format!("find OCR difficulty {difficulty}"))?;
        cases.push(case);
    }
    Ok(cases)
}

fn selected_variants() -> Result<Vec<(String, Variant)>> {
    let requested = comma_values("CATALOG_BENCH_PROBE_VARIANTS")?.unwrap_or_else(|| {
        vec![
            "text-default".to_string(),
            "image-default".to_string(),
            "text-low".to_string(),
            "image-low".to_string(),
            "text-medium".to_string(),
            "image-medium".to_string(),
            "text-high".to_string(),
            "image-high".to_string(),
            "text-default-stream".to_string(),
            "image-default-stream".to_string(),
        ]
    });
    requested
        .into_iter()
        .map(|name| parse_variant(&name).map(|variant| (name, variant)))
        .collect()
}

fn parse_variant(value: &str) -> Result<Variant> {
    let streaming = value.ends_with("-stream");
    let base = value.strip_suffix("-stream").unwrap_or(value);
    let (order, resolution) = base
        .split_once('-')
        .with_context(|| format!("probe variant {value:?} must be <text|image>-<resolution>"))?;
    let order = match order {
        "text" => PromptOrder::TextFirst,
        "image" => PromptOrder::ImageFirst,
        _ => bail!("unknown probe prompt order in {value:?}"),
    };
    let resolution = match resolution {
        "default" => None,
        "low" => Some(GeminiMediaResolution::Low),
        "medium" => Some(GeminiMediaResolution::Medium),
        "high" => Some(GeminiMediaResolution::High),
        _ => bail!("unknown probe media resolution in {value:?}"),
    };
    Ok(Variant {
        order,
        resolution,
        streaming,
    })
}

fn comma_values(name: &str) -> Result<Option<Vec<String>>> {
    let Some(value) = std::env::var(name).ok() else {
        return Ok(None);
    };
    let values = value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    ensure!(!values.is_empty(), "{name} cannot be empty");
    Ok(Some(values))
}

fn output_path() -> PathBuf {
    std::env::var_os("CATALOG_BENCH_PROBE_OUTPUT").map_or_else(
        || {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("target/catalog-benchmark/transport-probe.jsonl")
        },
        PathBuf::from,
    )
}

fn sniff_mime_type(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        "image/jpeg"
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        "image/webp"
    } else {
        "image/png"
    }
}

#[cfg(test)]
mod tests {
    use super::{GeminiMediaResolution, PromptOrder, parse_variant};

    #[test]
    fn variant_names_encode_every_transport_dimension() {
        let variant = parse_variant("image-medium-stream").unwrap();
        assert!(matches!(variant.order, PromptOrder::ImageFirst));
        assert_eq!(variant.resolution, Some(GeminiMediaResolution::Medium));
        assert!(variant.streaming);
    }
}
