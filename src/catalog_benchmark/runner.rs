use std::io::Cursor;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde_json::json;

use super::manifest::{Manifest, OcrCase, OcrInputMode, TextCase};
use super::reasoning::reasoning_policy_label;
use super::report::Attempt;
use super::scoring;
use super::setup::{Credentials, Pacer, Suites};
use crate::api::{
    TranslateImageRequest, TranslateTextRequest, translate_image_streaming,
    translate_text_streaming,
};
use crate::model_config::{ModelConfig, ModelType};
mod coordinate;
mod ordering;
use ordering::{case_at_difficulty, rotated};

pub fn run() -> Result<()> {
    let manifest = Manifest::load()?;
    manifest.validate()?;
    let suites = Suites::from_env()?;
    let credentials = Credentials::load()?;
    refresh_production_model_feed(&credentials);
    let filter = super::setup::model_filter();
    let providers = super::setup::provider_filter();
    let text_models = super::setup::select_models(
        ModelType::Text,
        filter.as_ref(),
        providers.as_ref(),
        &credentials,
    );
    let vision_models = super::setup::select_models(
        ModelType::Vision,
        filter.as_ref(),
        providers.as_ref(),
        &credentials,
    );
    super::setup::ensure_selection(suites, &text_models, &vision_models)?;

    if text_models
        .iter()
        .chain(&vision_models)
        .any(|model| model.provider == "gemini-live")
    {
        crate::api::gemini_live::init_gemini_live();
    }

    let mut recorder =
        super::history::live_recorder(&manifest, suites, &text_models, &vision_models)?;
    let completed = super::report::successful_attempt_keys(&super::setup::resume_inputs())?;
    let mut pacer = Pacer::from_env(&credentials)?;
    let timeout = super::setup::request_timeout()?;

    for round in 1..=manifest.rounds {
        if suites.text {
            let case = case_at_difficulty(&manifest.text_cases, round);
            for model in rotated(&text_models, round) {
                if completed.contains(&attempt_key("text", model, &case.id, round)) {
                    continue;
                }
                pacer.wait(model);
                recorder.push(run_text(model, case, round, &credentials, timeout))?;
            }
        }
        if suites.coordinate {
            let case = case_at_difficulty(&manifest.coordinate_cases, round);
            for model in rotated(&vision_models, round) {
                if completed.contains(&attempt_key("coordinate", model, &case.id, round)) {
                    continue;
                }
                pacer.wait(model);
                recorder.push(coordinate::run(
                    model,
                    case,
                    round,
                    &manifest,
                    &credentials,
                    timeout,
                    &mut pacer,
                ))?;
            }
        }
        if suites.ocr {
            let case = case_at_difficulty(&manifest.ocr_cases, round);
            for model in rotated(&vision_models, round) {
                if completed.contains(&attempt_key("ocr", model, &case.id, round)) {
                    continue;
                }
                pacer.wait(model);
                recorder.push(run_ocr(
                    model,
                    case,
                    round,
                    &manifest,
                    &credentials,
                    timeout,
                ))?;
            }
        }
    }
    recorder.finish()
}

fn refresh_production_model_feed(credentials: &Credentials) {
    if !credentials.supports("nvidia") {
        return;
    }
    match crate::model_feed::store::refresh() {
        Ok(feed) => println!(
            "BENCH_FEED provider={} generated_at={} models={}",
            feed.provider,
            feed.generated_at,
            feed.models.len()
        ),
        Err(error) if crate::model_feed::store::cached().is_some() => {
            println!("BENCH_FEED source=verified_cache refresh_error={error:#}");
        }
        Err(error) => {
            println!("BENCH_FEED source=catalog_fallback refresh_error={error:#}");
        }
    }
}

fn run_text(
    model: &ModelConfig,
    case: &TextCase,
    round: u8,
    credentials: &Credentials,
    timeout: Option<Duration>,
) -> Attempt {
    let instruction = case.instruction.clone();
    let started = Instant::now();
    let result = credentials.with_provider_key(&model.provider, |provider_key| {
        translate_text_streaming(
            TranslateTextRequest {
                groq_api_key: Credentials::groq_key_for(&model.provider, provider_key),
                gemini_api_key: provider_key,
                text: case.input.clone(),
                instruction,
                model: model.full_name.clone(),
                provider: model.provider.clone(),
                streaming_enabled: true,
                use_json_format: false,
                response_schema: None,
                search_label: None,
                ui_language: "en",
                cancel_token: None,
                request_timeout: timeout,
                target_language: case.target_language.clone(),
            },
            |_| {},
        )
    });
    let latency_ms = started.elapsed().as_millis();
    match result {
        Ok(response) if !response.trim().is_empty() => {
            let timing = TimingMetrics::for_response(latency_ms, &response);
            let similarity = scoring::text_similarity(&response, &case.reference);
            let term_coverage = scoring::term_coverage(&response, &case.required_terms);
            let exact_coverage = scoring::exact_constraint_coverage(
                &response,
                &case.required_exact,
                &case.required_exact_any,
            );
            let forbidden_avoidance =
                scoring::forbidden_avoidance(&response, &case.forbidden_terms);
            let line_count = scoring::line_count_matches(response.trim(), case.expected_line_count);
            let constraint_score =
                (term_coverage + exact_coverage + forbidden_avoidance + line_count) / 4.0;
            let similarity_weight = case.task.reference_similarity_weight();
            let automatic_score =
                similarity_weight * similarity + (1.0 - similarity_weight) * constraint_score;
            base_attempt(
                "text",
                model,
                case.id.clone(),
                case.difficulty,
                round,
                timing,
            )
            .success(
                None,
                None,
                response,
                json!({
                    "reference_similarity": similarity,
                    "required_term_coverage": term_coverage,
                    "required_exact_coverage": exact_coverage,
                    "required_exact_alternatives": case.required_exact_any,
                    "forbidden_term_avoidance": forbidden_avoidance,
                    "line_count_match": line_count,
                    "constraint_score": constraint_score,
                    "task": case.task.as_str(),
                    "reference_similarity_weight": similarity_weight,
                    "automatic_triage_score": automatic_score,
                    "input_chars": case.input.chars().count(),
                }),
                Some(case.reference.clone()),
                case.rubric.clone(),
                true,
            )
        }
        Ok(_) => base_attempt(
            "text",
            model,
            case.id.clone(),
            case.difficulty,
            round,
            TimingMetrics::for_response(latency_ms, ""),
        )
        .failure("empty", "provider returned an empty response"),
        Err(error) => base_attempt(
            "text",
            model,
            case.id.clone(),
            case.difficulty,
            round,
            TimingMetrics::failure(latency_ms),
        )
        .failure("request_error", error.to_string()),
    }
}

fn run_ocr(
    model: &ModelConfig,
    case: &OcrCase,
    round: u8,
    manifest: &Manifest,
    credentials: &Credentials,
    timeout: Option<Duration>,
) -> Attempt {
    let image_path = manifest.image_path(&case.image);
    let (image, original_bytes) = match load_ocr_image(&image_path, case.crop_px, case.input_mode) {
        Ok(value) => value,
        Err(error) => {
            return base_attempt(
                "ocr",
                model,
                case.id.clone(),
                case.difficulty,
                round,
                TimingMetrics::default(),
            )
            .failure("fixture_error", error.to_string());
        }
    };
    let image_bytes = original_bytes.len();
    let image_width = image.width();
    let image_height = image.height();
    let prompt = case.instruction.clone();
    let started = Instant::now();
    let result = credentials.with_provider_key(&model.provider, |provider_key| {
        translate_image_streaming(
            TranslateImageRequest {
                groq_api_key: Credentials::groq_key_for(&model.provider, provider_key),
                gemini_api_key: provider_key,
                prompt,
                model: model.full_name.clone(),
                provider: model.provider.clone(),
                image,
                original_bytes: Some(original_bytes),
                streaming_enabled: false,
                response_schema: None,
                cancel_token: None,
                request_timeout: timeout,
            },
            |_| {},
        )
    });
    let latency_ms = started.elapsed().as_millis();
    match result {
        Ok(response) if !response.trim().is_empty() => {
            let transcription = scoring::transcription(&response);
            let similarity = std::iter::once(&case.reference)
                .chain(&case.accepted_references)
                .map(|reference| scoring::ocr_similarity(&transcription, reference))
                .max_by(f64::total_cmp)
                .expect("OCR cases always have a primary reference");
            base_attempt(
                "ocr",
                model,
                case.id.clone(),
                case.difficulty,
                round,
                TimingMetrics::for_response(latency_ms, &response),
            )
            .success(
                Some(similarity),
                Some(similarity >= 0.98),
                response,
                json!({
                    "normalized_character_similarity": similarity,
                    "transcription": transcription,
                    "input_image_bytes": image_bytes,
                    "input_image_width": image_width,
                    "input_image_height": image_height,
                    "input_mode": case.input_mode.as_str(),
                    "vision_request_profile":
                        crate::model_config::vision_request_profile(
                            &model.provider,
                            &model.full_name,
                        ),
                }),
                Some(case.reference.clone()),
                Vec::new(),
                false,
            )
        }
        Ok(_) => base_attempt(
            "ocr",
            model,
            case.id.clone(),
            case.difficulty,
            round,
            TimingMetrics::for_response(latency_ms, ""),
        )
        .failure("empty", "provider returned an empty response"),
        Err(error) => base_attempt(
            "ocr",
            model,
            case.id.clone(),
            case.difficulty,
            round,
            TimingMetrics::failure(latency_ms),
        )
        .failure("request_error", error.to_string()),
    }
}

#[derive(Clone, Copy, Default)]
struct TimingMetrics {
    total_ms: u128,
    output_chars: Option<usize>,
    end_to_end_chars_per_second: Option<f64>,
}

impl TimingMetrics {
    fn failure(total_ms: u128) -> Self {
        Self {
            total_ms,
            ..Self::default()
        }
    }

    fn for_response(total_ms: u128, response: &str) -> Self {
        let output_chars = response.chars().count();
        if output_chars == 0 {
            return Self {
                total_ms,
                output_chars: Some(0),
                ..Self::default()
            };
        }

        Self {
            total_ms,
            output_chars: Some(output_chars),
            end_to_end_chars_per_second: rate(output_chars, total_ms),
        }
    }

    fn for_non_streaming_pipeline(total_ms: u128, output_chars: usize) -> Self {
        Self {
            total_ms,
            output_chars: Some(output_chars),
            end_to_end_chars_per_second: rate(output_chars, total_ms),
        }
    }
}

fn rate(characters: usize, milliseconds: u128) -> Option<f64> {
    (characters > 0 && milliseconds > 0)
        .then(|| characters as f64 / (milliseconds as f64 / 1_000.0))
}

fn base_attempt(
    suite: &str,
    model: &ModelConfig,
    case_id: String,
    difficulty: u8,
    round: u8,
    timing: TimingMetrics,
) -> AttemptBuilder {
    AttemptBuilder(Attempt {
        suite: suite.to_string(),
        round,
        difficulty,
        case_id,
        model_id: model.id.clone(),
        model_name: model.full_name.clone(),
        provider: model.provider.clone(),
        reasoning_policy: reasoning_policy_label(model),
        status: "pending".to_string(),
        latency_ms: timing.total_ms,
        output_chars: timing.output_chars,
        end_to_end_chars_per_second: timing.end_to_end_chars_per_second,
        score: None,
        strict_pass: None,
        response: None,
        error: None,
        details: json!({}),
        reference: None,
        rubric: Vec::new(),
        manual_review_required: false,
    })
}

fn attempt_key(
    suite: &str,
    model: &ModelConfig,
    case_id: &str,
    round: u8,
) -> super::report::AttemptKey {
    (
        suite.to_string(),
        model.id.clone(),
        round,
        case_id.to_string(),
    )
}

struct AttemptBuilder(Attempt);

impl AttemptBuilder {
    #[expect(
        clippy::too_many_arguments,
        reason = "all scored result fields are explicit at the call site"
    )]
    fn success(
        mut self,
        score: Option<f64>,
        strict_pass: Option<bool>,
        response: String,
        details: serde_json::Value,
        reference: Option<String>,
        rubric: Vec<String>,
        manual_review_required: bool,
    ) -> Attempt {
        self.0.status = "success".to_string();
        self.0.score = score;
        self.0.strict_pass = strict_pass;
        self.0.response = Some(response);
        self.0.details = details;
        self.0.reference = reference;
        self.0.rubric = rubric;
        self.0.manual_review_required = manual_review_required;
        self.0
    }

    fn with_response(mut self, response: String) -> Self {
        self.0.response = Some(response);
        self
    }

    fn failure(mut self, status: &str, error: impl Into<String>) -> Attempt {
        self.0.status = status.to_string();
        self.0.error = Some(error.into());
        self.0
    }
}

fn load_image(path: &std::path::Path) -> Result<(image::RgbaImage, Vec<u8>)> {
    let bytes = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let image = image::load_from_memory(&bytes)
        .with_context(|| format!("decode {}", path.display()))?
        .to_rgba8();
    Ok((image, bytes))
}

pub(super) fn load_ocr_image(
    path: &std::path::Path,
    crop_px: Option<[u32; 4]>,
    input_mode: OcrInputMode,
) -> Result<(image::RgbaImage, Vec<u8>)> {
    let (image, original_bytes) = load_image(path)?;
    if input_mode == OcrInputMode::OriginalFile {
        anyhow::ensure!(
            crop_px.is_none(),
            "original-file OCR inputs cannot apply an app crop"
        );
        return Ok((image, original_bytes));
    }
    let effective = match crop_px {
        Some([x, y, width, height]) => {
            image::imageops::crop_imm(&image, x, y, width, height).to_image()
        }
        None => image,
    };
    let mut encoded = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(effective.clone())
        .write_to(&mut encoded, image::ImageFormat::Png)
        .context("encode OCR screen-crop input")?;
    Ok((effective, encoded.into_inner()))
}

#[cfg(test)]
mod tests;
