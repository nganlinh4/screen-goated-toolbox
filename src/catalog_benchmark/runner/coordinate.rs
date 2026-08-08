use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde_json::json;

use super::{TimingMetrics, base_attempt};
use crate::api::{TranslateImageRequest, translate_image_streaming};
use crate::model_config::ModelConfig;
use crate::overlay::computer_control::vision_contract::{
    CONTROL_VISION_SHORT_EDGE, GROUNDING_STREAMING_ENABLED, GroundingRequest,
    MIN_VERIFICATION_CONFIDENCE, crosshair_crop, encode_jpeg, grounding_reports_not_visible,
    parse_named_grounding_records, parse_verification, point_request, resize_to_short_edge,
    verification_request,
};

use super::super::manifest::{CoordinateCase, Manifest};
use super::super::report::Attempt;
use super::super::scoring;
use super::super::setup::{Credentials, Pacer};

pub(super) fn run(
    model: &ModelConfig,
    case: &CoordinateCase,
    round: u8,
    manifest: &Manifest,
    credentials: &Credentials,
    timeout: Option<Duration>,
    pacer: &mut Pacer,
) -> Attempt {
    let prepared = match PreparedFrame::load(&manifest.image_path(&case.image), case.box_px) {
        Ok(prepared) => prepared,
        Err(error) => {
            return attempt(model, case, round, TimingMetrics::default())
                .failure("fixture_error", error.to_string());
        }
    };

    let locate = call_model(
        model,
        credentials,
        timeout,
        &prepared.jpeg,
        point_request(&case.target, &case.context),
    );
    let locate_response = match locate.result {
        Ok(response) => response,
        Err(error) => {
            return attempt(model, case, round, locate.timing)
                .failure("request_error", error.to_string());
        }
    };
    if grounding_reports_not_visible(&locate_response, &["target"]) {
        return attempt(model, case, round, locate.timing).success(
            0.0,
            Some(false),
            locate_response,
            details(
                model,
                &prepared,
                case,
                None,
                json!({"reported_not_visible": true}),
            ),
            None,
            Vec::new(),
            false,
        );
    }
    let Some(point) = parse_named_grounding_records(&locate_response, &["target"])
        .and_then(|points| points.into_iter().next())
    else {
        return attempt(model, case, round, locate.timing)
            .with_response(locate_response)
            .failure(
                "malformed",
                "production grounding parser rejected the target record",
            );
    };
    let (x, y) = (point.x, point.y);
    let score = scoring::coordinate_point(x, y, prepared.width, prepared.height, prepared.box_px);
    let verification_image = match crosshair_crop(&prepared.jpeg, x, y) {
        Ok(image) => image,
        Err(error) => {
            return attempt(model, case, round, locate.timing)
                .with_response(locate_response)
                .failure("fixture_error", format!("build verification crop: {error}"));
        }
    };

    // A coordinate attempt has two production calls. Pace both calls so free
    // provider limits are not distorted; artificial pacing is excluded from
    // the latency value used for catalog decisions.
    pacer.wait(&model.provider);
    let verification = call_model(
        model,
        credentials,
        timeout,
        &verification_image,
        verification_request(&case.target, &case.context),
    );
    let total_latency = locate.timing.total_ms + verification.timing.total_ms;
    let verification_response = match verification.result {
        Ok(response) => response,
        Err(error) => {
            return attempt(model, case, round, TimingMetrics::failure(total_latency))
                .with_response(locate_response)
                .failure("verification_request_error", error.to_string());
        }
    };
    let Some(decision) = parse_verification(&verification_response) else {
        return attempt(
            model,
            case,
            round,
            TimingMetrics::for_non_streaming_pipeline(
                total_latency,
                locate_response.chars().count() + verification_response.chars().count(),
            ),
        )
        .with_response(combined_response(&locate_response, &verification_response))
        .failure(
            "verification_malformed",
            "production verification parser rejected the response",
        );
    };
    let verified = decision.matches && decision.confidence >= MIN_VERIFICATION_CONFIDENCE;
    let strict_pass = score.hit && verified;
    let timing = TimingMetrics::for_non_streaming_pipeline(
        total_latency,
        locate_response.chars().count() + verification_response.chars().count(),
    );
    let result_details = json!({
        "x_1000": score.x_1000,
        "y_1000": score.y_1000,
        "error_px": score.error_px,
        "locator_hit": score.hit,
        "verification_matches": decision.matches,
        "verification_confidence": decision.confidence,
        "verification_note": decision.note,
        "verification_accepted": verified,
        "locate_latency_ms": locate.timing.total_ms,
        "verification_latency_ms": verification.timing.total_ms,
        "verification_image_bytes": verification_image.len(),
    });
    attempt(model, case, round, timing).success(
        f64::from(strict_pass),
        Some(strict_pass),
        combined_response(&locate_response, &verification_response),
        details(model, &prepared, case, Some(score.hit), result_details),
        None,
        Vec::new(),
        false,
    )
}

struct PreparedFrame {
    jpeg: Vec<u8>,
    width: u32,
    height: u32,
    source_width: u32,
    source_height: u32,
    source_bytes: usize,
    box_px: [f64; 4],
}

impl PreparedFrame {
    fn load(path: &Path, source_box: [f64; 4]) -> Result<Self> {
        let source_bytes =
            std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
        let source = image::load_from_memory(&source_bytes)
            .with_context(|| format!("decode {}", path.display()))?;
        let source_width = source.width();
        let source_height = source.height();
        // Screen capture is RGB before production resize/JPEG encoding.
        let source = image::DynamicImage::ImageRgb8(source.to_rgb8());
        let prepared_source = resize_to_short_edge(source, CONTROL_VISION_SHORT_EDGE);
        let jpeg = encode_jpeg(&prepared_source)?;
        let prepared = image::load_from_memory(&jpeg).context("decode production control JPEG")?;
        let width = prepared.width();
        let height = prepared.height();
        let scale_x = f64::from(width) / f64::from(source_width);
        let scale_y = f64::from(height) / f64::from(source_height);
        let [x, y, box_width, box_height] = source_box;
        Ok(Self {
            jpeg,
            width,
            height,
            source_width,
            source_height,
            source_bytes: source_bytes.len(),
            box_px: [
                x * scale_x,
                y * scale_y,
                box_width * scale_x,
                box_height * scale_y,
            ],
        })
    }
}

struct VisionCall {
    result: Result<String>,
    timing: TimingMetrics,
}

fn call_model(
    model: &ModelConfig,
    credentials: &Credentials,
    timeout: Option<Duration>,
    image_bytes: &[u8],
    request: GroundingRequest,
) -> VisionCall {
    let image = match image::load_from_memory(image_bytes) {
        Ok(image) => image.to_rgba8(),
        Err(error) => {
            return VisionCall {
                result: Err(error.into()),
                timing: TimingMetrics::default(),
            };
        }
    };
    let started = Instant::now();
    let mut events = Vec::new();
    let result = translate_image_streaming(
        TranslateImageRequest {
            groq_api_key: &credentials.groq,
            gemini_api_key: &credentials.gemini,
            prompt: request.prompt,
            model: model.full_name.clone(),
            provider: model.provider.clone(),
            image,
            original_bytes: Some(image_bytes.to_vec()),
            streaming_enabled: GROUNDING_STREAMING_ENABLED,
            response_schema: request.response_schema,
            cancel_token: None,
            request_timeout: timeout,
        },
        |chunk| events.push((started.elapsed().as_millis(), chunk.to_string())),
    );
    let elapsed = started.elapsed().as_millis();
    let timing = match &result {
        Ok(response) => TimingMetrics::for_response(elapsed, &events, response),
        Err(_) => TimingMetrics::failure(elapsed),
    };
    VisionCall { result, timing }
}

fn details(
    model: &ModelConfig,
    prepared: &PreparedFrame,
    case: &CoordinateCase,
    locator_hit: Option<bool>,
    result: serde_json::Value,
) -> serde_json::Value {
    json!({
        "pipeline": "production-point-plus-crosshair-verification",
        "pacing_delay_excluded_from_latency": true,
        "control_short_edge_limit": CONTROL_VISION_SHORT_EDGE,
        "source_box_px": case.box_px,
        "effective_box_px": prepared.box_px,
        "source_image_bytes": prepared.source_bytes,
        "source_image_width": prepared.source_width,
        "source_image_height": prepared.source_height,
        "input_image_bytes": prepared.jpeg.len(),
        "input_image_width": prepared.width,
        "input_image_height": prepared.height,
        "locator_hit": locator_hit,
        "vision_request_profile": crate::model_config::vision_request_profile(
            &model.provider,
            &model.full_name,
        ),
        "result": result,
    })
}

fn combined_response(locate: &str, verification: &str) -> String {
    json!({
        "locate": locate,
        "verify": verification,
    })
    .to_string()
}

fn attempt(
    model: &ModelConfig,
    case: &CoordinateCase,
    round: u8,
    timing: TimingMetrics,
) -> super::AttemptBuilder {
    base_attempt(
        "coordinate",
        model,
        case.id.clone(),
        case.difficulty,
        round,
        timing,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_preparation_matches_the_production_short_edge_contract() {
        let manifest = Manifest::load().unwrap();
        let case = manifest
            .coordinate_cases
            .iter()
            .find(|case| case.id == "coordinate-07-row-relative-add")
            .unwrap();
        let prepared = PreparedFrame::load(&manifest.image_path(&case.image), case.box_px).unwrap();
        assert_eq!(prepared.height, CONTROL_VISION_SHORT_EDGE);
        assert_eq!(prepared.width, 2302);
        assert_eq!(
            prepared.box_px[0],
            case.box_px[0] * f64::from(prepared.width) / 2360.0
        );
    }

    #[test]
    fn coordinate_benchmark_uses_the_exact_production_contract() {
        let request = point_request("the fourth star", "Give a four-star rating");
        assert!(request.prompt.contains("supplied schema"));
        assert!(request.response_schema.is_some());
        assert_eq!(
            request.response_schema,
            crate::overlay::computer_control::vision_contract::point_request(
                "the fourth star",
                "Give a four-star rating",
            )
            .response_schema
        );
    }
}
