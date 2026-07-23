use std::io::Cursor;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde_json::json;

use super::manifest::{CoordinateCase, Manifest, OcrCase, TextCase};
use super::report::{Attempt, Recorder};
use super::scoring;
use super::setup::{Credentials, Pacer, Suites};
use crate::api::{
    TranslateImageRequest, TranslateTextRequest, translate_image_streaming,
    translate_text_streaming,
};
use crate::model_config::{ModelConfig, ModelType};

pub fn run() -> Result<()> {
    let manifest = Manifest::load()?;
    manifest.validate()?;
    let suites = Suites::from_env()?;
    let credentials = Credentials::load();
    let filter = super::setup::model_filter();
    let text_models = super::setup::select_models(ModelType::Text, filter.as_ref(), &credentials);
    let vision_models =
        super::setup::select_models(ModelType::Vision, filter.as_ref(), &credentials);
    super::setup::ensure_selection(suites, &text_models, &vision_models)?;

    if text_models
        .iter()
        .chain(&vision_models)
        .any(|model| model.provider == "gemini-live")
    {
        crate::api::gemini_live::init_gemini_live();
    }

    let output = super::setup::output_dir();
    let mut recorder = Recorder::new(&output)?;
    let completed = super::report::successful_attempt_keys(&super::setup::resume_inputs())?;
    let mut pacer = Pacer::from_env()?;
    let timeout = super::setup::request_timeout()?;

    for round in 1..=manifest.rounds {
        if suites.text {
            let case = case_at_difficulty(&manifest.text_cases, round);
            for model in rotated(&text_models, round) {
                if completed.contains(&attempt_key("text", model, &case.id, round)) {
                    continue;
                }
                pacer.wait(&model.provider);
                recorder.push(run_text(model, case, round, &credentials, timeout))?;
            }
        }
        if suites.coordinate {
            let case = case_at_difficulty(&manifest.coordinate_cases, round);
            for model in rotated(&vision_models, round) {
                if completed.contains(&attempt_key("coordinate", model, &case.id, round)) {
                    continue;
                }
                pacer.wait(&model.provider);
                recorder.push(run_coordinate(
                    model,
                    case,
                    round,
                    &manifest,
                    &credentials,
                    timeout,
                ))?;
            }
        }
        if suites.ocr {
            let case = case_at_difficulty(&manifest.ocr_cases, round);
            for model in rotated(&vision_models, round) {
                if completed.contains(&attempt_key("ocr", model, &case.id, round)) {
                    continue;
                }
                pacer.wait(&model.provider);
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

fn run_text(
    model: &ModelConfig,
    case: &TextCase,
    round: u8,
    credentials: &Credentials,
    timeout: Option<Duration>,
) -> Attempt {
    let instruction = format!(
        "Translate from {} to {}. Preserve meaning, tone, formatting, names, numbers, and constraints. Output only the translation.",
        case.source_language, case.target_language
    );
    let started = Instant::now();
    let result = translate_text_streaming(
        TranslateTextRequest {
            groq_api_key: &credentials.groq,
            gemini_api_key: &credentials.gemini,
            text: case.input.clone(),
            instruction,
            model: model.full_name.clone(),
            provider: model.provider.clone(),
            streaming_enabled: false,
            use_json_format: false,
            search_label: None,
            ui_language: "en",
            cancel_token: None,
            request_timeout: timeout,
            target_language: Some(case.target_language.clone()),
        },
        |_| {},
    );
    let latency_ms = started.elapsed().as_millis();
    match result {
        Ok(response) if !response.trim().is_empty() => {
            let similarity = scoring::text_similarity(&response, &case.reference);
            let term_coverage = scoring::term_coverage(&response, &case.required_terms);
            let exact_coverage = scoring::exact_coverage(&response, &case.required_exact);
            let forbidden_avoidance =
                scoring::forbidden_avoidance(&response, &case.forbidden_terms);
            let line_count = scoring::line_count_matches(response.trim(), case.expected_line_count);
            let constraint_score =
                (term_coverage + exact_coverage + forbidden_avoidance + line_count) / 4.0;
            base_attempt(
                "text",
                model,
                case.id.clone(),
                case.difficulty,
                round,
                latency_ms,
            )
            .success(
                0.65 * similarity + 0.35 * constraint_score,
                None,
                response,
                json!({
                    "reference_similarity": similarity,
                    "required_term_coverage": term_coverage,
                    "required_exact_coverage": exact_coverage,
                    "forbidden_term_avoidance": forbidden_avoidance,
                    "line_count_match": line_count,
                    "constraint_score": constraint_score,
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
            latency_ms,
        )
        .failure("empty", "provider returned an empty response"),
        Err(error) => base_attempt(
            "text",
            model,
            case.id.clone(),
            case.difficulty,
            round,
            latency_ms,
        )
        .failure("request_error", error.to_string()),
    }
}

fn run_coordinate(
    model: &ModelConfig,
    case: &CoordinateCase,
    round: u8,
    manifest: &Manifest,
    credentials: &Credentials,
    timeout: Option<Duration>,
) -> Attempt {
    let prompt = format!(
        "Find this target in the image: {}. Output ONLY JSON {{\"x\": <number>, \"y\": <number>}}. x and y are the target center on a 0-1000 grid (left to right, top to bottom).",
        case.target
    );
    let image_path = manifest.image_path(&case.image);
    let loaded = load_image(&image_path);
    let (image, original_bytes) = match loaded {
        Ok(value) => value,
        Err(error) => {
            return base_attempt(
                "coordinate",
                model,
                case.id.clone(),
                case.difficulty,
                round,
                0,
            )
            .failure("fixture_error", error.to_string());
        }
    };
    let width = image.width();
    let height = image.height();
    let started = Instant::now();
    let result = translate_image_streaming(
        TranslateImageRequest {
            groq_api_key: &credentials.groq,
            gemini_api_key: &credentials.gemini,
            prompt,
            model: model.full_name.clone(),
            provider: model.provider.clone(),
            image,
            original_bytes: Some(original_bytes),
            streaming_enabled: false,
            use_json_format: true,
            response_schema: Some(coordinate_schema()),
            cancel_token: None,
            request_timeout: timeout,
        },
        |_| {},
    );
    let latency_ms = started.elapsed().as_millis();
    match result {
        Ok(response) => match scoring::coordinate(&response, width, height, case.box_px) {
            Some(score) => base_attempt("coordinate", model, case.id.clone(), case.difficulty, round, latency_ms)
                .success(
                    f64::from(score.hit),
                    Some(score.hit),
                    response,
                    json!({"x_1000": score.x_1000, "y_1000": score.y_1000, "error_px": score.error_px, "box_px": case.box_px}),
                    None,
                    Vec::new(),
                    false,
                ),
            None => base_attempt("coordinate", model, case.id.clone(), case.difficulty, round, latency_ms)
                .with_response(response)
                .failure("malformed", "response did not contain valid 0-1000 x/y coordinates"),
        },
        Err(error) => base_attempt("coordinate", model, case.id.clone(), case.difficulty, round, latency_ms)
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
    let (image, original_bytes) = match load_ocr_image(&image_path, case.crop_px) {
        Ok(value) => value,
        Err(error) => {
            return base_attempt("ocr", model, case.id.clone(), case.difficulty, round, 0)
                .failure("fixture_error", error.to_string());
        }
    };
    let prompt = case.instruction.clone();
    let started = Instant::now();
    let result = translate_image_streaming(
        TranslateImageRequest {
            groq_api_key: &credentials.groq,
            gemini_api_key: &credentials.gemini,
            prompt,
            model: model.full_name.clone(),
            provider: model.provider.clone(),
            image,
            original_bytes: Some(original_bytes),
            streaming_enabled: false,
            use_json_format: false,
            response_schema: None,
            cancel_token: None,
            request_timeout: timeout,
        },
        |_| {},
    );
    let latency_ms = started.elapsed().as_millis();
    match result {
        Ok(response) if !response.trim().is_empty() => {
            let transcription = scoring::transcription(&response);
            let similarity = std::iter::once(&case.reference)
                .chain(&case.accepted_references)
                .map(|reference| scoring::text_similarity(&transcription, reference))
                .max_by(f64::total_cmp)
                .expect("OCR cases always have a primary reference");
            base_attempt("ocr", model, case.id.clone(), case.difficulty, round, latency_ms)
                .success(
                    similarity,
                    Some(similarity >= 0.98),
                    response,
                    json!({"normalized_character_similarity": similarity, "transcription": transcription}),
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
            latency_ms,
        )
        .failure("empty", "provider returned an empty response"),
        Err(error) => base_attempt(
            "ocr",
            model,
            case.id.clone(),
            case.difficulty,
            round,
            latency_ms,
        )
        .failure("request_error", error.to_string()),
    }
}

fn base_attempt(
    suite: &str,
    model: &ModelConfig,
    case_id: String,
    difficulty: u8,
    round: u8,
    latency_ms: u128,
) -> AttemptBuilder {
    AttemptBuilder(Attempt {
        suite: suite.to_string(),
        round,
        difficulty,
        case_id,
        model_id: model.id.clone(),
        model_name: model.full_name.clone(),
        provider: model.provider.clone(),
        status: "pending".to_string(),
        latency_ms,
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
        score: f64,
        strict_pass: Option<bool>,
        response: String,
        details: serde_json::Value,
        reference: Option<String>,
        rubric: Vec<String>,
        manual_review_required: bool,
    ) -> Attempt {
        self.0.status = "success".to_string();
        self.0.score = Some(score);
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

fn load_ocr_image(
    path: &std::path::Path,
    crop_px: Option<[u32; 4]>,
) -> Result<(image::RgbaImage, Vec<u8>)> {
    let (image, original_bytes) = load_image(path)?;
    let Some([x, y, width, height]) = crop_px else {
        return Ok((image, original_bytes));
    };
    let cropped = image::imageops::crop_imm(&image, x, y, width, height).to_image();
    let mut encoded = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(cropped.clone())
        .write_to(&mut encoded, image::ImageFormat::Png)
        .context("encode OCR crop")?;
    Ok((cropped, encoded.into_inner()))
}

fn coordinate_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": {"x": {"type": "number"}, "y": {"type": "number"}},
        "required": ["x", "y"],
        "additionalProperties": false
    })
}

fn case_at_difficulty<T>(cases: &[T], difficulty: u8) -> &T
where
    T: Difficulty,
{
    cases
        .iter()
        .find(|case| case.difficulty() == difficulty)
        .expect("validated difficulty")
}

trait Difficulty {
    fn difficulty(&self) -> u8;
}

impl Difficulty for TextCase {
    fn difficulty(&self) -> u8 {
        self.difficulty
    }
}
impl Difficulty for CoordinateCase {
    fn difficulty(&self) -> u8 {
        self.difficulty
    }
}
impl Difficulty for OcrCase {
    fn difficulty(&self) -> u8 {
        self.difficulty
    }
}

fn rotated(models: &[ModelConfig], round: u8) -> impl Iterator<Item = &ModelConfig> {
    let skip = usize::from(round.saturating_sub(1)) % models.len();
    models.iter().cycle().skip(skip).take(models.len())
}

#[cfg(test)]
mod tests {
    use super::load_ocr_image;
    use crate::catalog_benchmark::manifest::Manifest;

    #[test]
    fn ocr_runtime_inputs_apply_manifest_crops() {
        let manifest = Manifest::load().unwrap();
        for case in &manifest.ocr_cases {
            let (image, encoded) =
                load_ocr_image(&manifest.image_path(&case.image), case.crop_px).unwrap();
            if let Some([_, _, width, height]) = case.crop_px {
                assert_eq!((image.width(), image.height()), (width, height));
                let decoded = image::load_from_memory(&encoded).unwrap();
                assert_eq!((decoded.width(), decoded.height()), (width, height));
            }
        }
    }
}
