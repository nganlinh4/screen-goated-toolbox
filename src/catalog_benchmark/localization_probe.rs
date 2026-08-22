use std::collections::{BTreeSet, HashMap, HashSet};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, ensure};
use serde_json::json;

use super::manifest::{LocalizationCase, Manifest};
use super::reasoning::reasoning_policy_label;
use super::report::{Attempt, Recorder};
use super::setup::{Credentials, Pacer};
use crate::api::{TranslateTextRequest, translate_text_streaming};
use crate::model_config::{ModelConfig, ModelType};
use crate::overlay::screen_translate::contract::{
    DetectedTextRegion, NormalizedBounds, parse_response, prompt_with_instruction, response_schema,
};

mod review;
mod scoring;

pub(super) fn run() -> Result<()> {
    let manifest = Manifest::load()?;
    manifest.validate()?;
    let credentials = Credentials::load()?;
    let filter = model_filter()?;
    let mut models =
        super::setup::select_models(ModelType::Text, Some(&filter), None, &credentials);
    ensure!(
        !models.is_empty(),
        "no selected structured screen-translation model has credentials"
    );
    order_by_priority(&mut models);
    let levels = levels_from_env()?;
    let cases = case_filter()?;
    let selected_cases = manifest
        .localization_cases
        .iter()
        .filter(|case| levels.contains(&case.difficulty))
        .filter(|case| cases.as_ref().is_none_or(|ids| ids.contains(&case.id)))
        .collect::<Vec<_>>();
    ensure!(
        !selected_cases.is_empty(),
        "localization filters selected no benchmark cases"
    );
    if let Some(ids) = &cases {
        ensure!(
            selected_cases.len() == ids.len(),
            "CATALOG_BENCH_LOCALIZATION_CASES contains an unknown or level-filtered case"
        );
    }
    let output = localization_output_dir();
    let mut recorder = Recorder::new(&output)?;
    let mut review_entries = Vec::new();
    let mut pacer = Pacer::from_env(&credentials)?;
    let timeout = super::setup::request_timeout()?.or(Some(Duration::from_secs(35)));

    for case in selected_cases {
        for model in &models {
            pacer.wait(model);
            let (attempt, review) =
                run_case(&manifest, case, model, &credentials, timeout, &output);
            recorder.push(attempt)?;
            if let Some(review) = review {
                review_entries.push(review);
            }
        }
    }
    recorder.finish()?;
    review::write_review(&output, &review_entries)?;
    println!(
        "Structured Screen Translate review: {}",
        output.join("localization-review.html").display()
    );
    Ok(())
}

fn run_case(
    manifest: &Manifest,
    case: &LocalizationCase,
    model: &ModelConfig,
    credentials: &Credentials,
    timeout: Option<Duration>,
    output: &std::path::Path,
) -> (Attempt, Option<review::ReviewEntry>) {
    let image_path = manifest.image_path(&case.image);
    let image = match image::open(&image_path) {
        Ok(image) => image.to_rgba8(),
        Err(error) => {
            return (
                failed_attempt(case, model, "fixture_error", error.to_string(), 0),
                None,
            );
        }
    };
    let candidates = reference_candidates(case, image.width(), image.height());
    let request_text = match prompt_with_instruction(
        &case.target_language,
        &crate::config::types::ScreenTranslateSettings::default_prompt(),
        &candidates,
    ) {
        Ok(value) => value,
        Err(error) => {
            return (
                failed_attempt(case, model, "fixture_error", error.to_string(), 0),
                None,
            );
        }
    };
    let schema = response_schema(candidates.len());
    let started = Instant::now();
    let mut parser =
        crate::overlay::screen_translate::stream_parser::TranslationStreamParser::new(&candidates);
    let result = credentials.with_provider_key(&model.provider, |provider_key| {
        translate_text_streaming(
            TranslateTextRequest {
                groq_api_key: Credentials::groq_key_for(&model.provider, provider_key),
                gemini_api_key: provider_key,
                text: request_text.clone(),
                instruction: "Return only the requested structured screen translation.".to_string(),
                model: model.full_name.clone(),
                provider: model.provider.clone(),
                streaming_enabled: true,
                use_json_format: true,
                response_schema: Some(&schema),
                search_label: None,
                ui_language: "en",
                cancel_token: None,
                request_timeout: timeout,
                target_language: Some(case.target_language.clone()),
            },
            |chunk| {
                parser.push(chunk);
            },
        )
    });
    let latency_ms = started.elapsed().as_millis();
    let response = match result {
        Ok(response) if !response.trim().is_empty() => response,
        Ok(_) => {
            return (
                failed_attempt(
                    case,
                    model,
                    "empty",
                    "provider returned an empty response".to_string(),
                    latency_ms,
                ),
                None,
            );
        }
        Err(error) => {
            return (
                failed_attempt(case, model, "request_error", error.to_string(), latency_ms),
                None,
            );
        }
    };
    let document = match parse_response(&response, &candidates) {
        Ok(document) => document,
        Err(error) => {
            let mut attempt =
                failed_attempt(case, model, "parse_error", error.to_string(), latency_ms);
            attempt.response = Some(response);
            return (attempt, None);
        }
    };
    let evaluation = scoring::evaluate(case, &document, image.width(), image.height());
    let changed_ratio = changed_translation_ratio(&document);
    let strict_pass = !document.regions.is_empty() && changed_ratio >= 0.5;
    let score = 0.5 * evaluation.metrics.region_recall + 0.5 * changed_ratio;
    let overlays =
        review::write_overlays(output, &model.id, "text-schema", case, &image, &evaluation);
    let (raw_image, painted_image) = match overlays {
        Ok(paths) => paths,
        Err(error) => {
            let mut attempt =
                failed_attempt(case, model, "artifact_error", error.to_string(), latency_ms);
            attempt.response = Some(response);
            return (attempt, None);
        }
    };
    let response_chars = response.chars().count();
    let attempt = Attempt {
        suite: "screen-translate-structured-text".to_string(),
        round: case.difficulty,
        difficulty: case.difficulty,
        case_id: case.id.clone(),
        model_id: model.id.clone(),
        model_name: model.full_name.clone(),
        provider: model.provider.clone(),
        reasoning_policy: reasoning_policy_label(model),
        status: "success".to_string(),
        latency_ms,
        output_chars: Some(response_chars),
        end_to_end_chars_per_second: rate(response_chars, latency_ms),
        score: Some(score),
        strict_pass: Some(strict_pass),
        response: Some(response),
        error: None,
        details: json!({
            "schema_parse": true,
            "changed_translation_ratio": changed_ratio,
            "returned_regions": document.regions.len(),
            "input_regions": candidates.len(),
            "stream_rejected_regions": parser.rejected_count(),
            "metrics": evaluation.metrics,
            "matches": evaluation.matches,
        }),
        reference: Some("Valid production schema with detector-owned ids and geometry".to_string()),
        rubric: vec![
            "Return the strict production schema".to_string(),
            "Preserve only supplied detector region ids".to_string(),
            "Translate readable language instead of echoing source text".to_string(),
        ],
        manual_review_required: false,
    };
    let entry = review::ReviewEntry {
        model_id: model.id.clone(),
        case_id: case.id.clone(),
        difficulty: case.difficulty,
        variant: "text-schema".to_string(),
        raw_image,
        painted_image,
        metrics: evaluation.metrics,
    };
    (attempt, Some(entry))
}

fn failed_attempt(
    case: &LocalizationCase,
    model: &ModelConfig,
    status: &str,
    error: String,
    latency_ms: u128,
) -> Attempt {
    Attempt {
        suite: "screen-translate-structured-text".to_string(),
        round: case.difficulty,
        difficulty: case.difficulty,
        case_id: case.id.clone(),
        model_id: model.id.clone(),
        model_name: model.full_name.clone(),
        provider: model.provider.clone(),
        reasoning_policy: reasoning_policy_label(model),
        status: status.to_string(),
        latency_ms,
        output_chars: None,
        end_to_end_chars_per_second: None,
        score: None,
        strict_pass: Some(false),
        response: None,
        error: Some(error),
        details: json!({ "schema_parse": false }),
        reference: None,
        rubric: Vec::new(),
        manual_review_required: false,
    }
}

fn changed_translation_ratio(
    document: &crate::overlay::screen_translate::contract::TranslationDocument,
) -> f64 {
    if document.regions.is_empty() {
        return 0.0;
    }
    let changed = document
        .regions
        .iter()
        .filter(|region| {
            !region
                .source_text
                .eq_ignore_ascii_case(&region.translated_segments.join(" "))
        })
        .count();
    changed as f64 / document.regions.len() as f64
}

fn model_filter() -> Result<HashSet<String>> {
    let selected = std::env::var("CATALOG_BENCH_LOCALIZATION_MODELS").unwrap_or_else(|_| {
        crate::model_config::default_text_to_text_priority_chain_ids().join(",")
    });
    let filter = selected
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect::<HashSet<_>>();
    ensure!(
        !filter.is_empty(),
        "CATALOG_BENCH_LOCALIZATION_MODELS cannot be empty"
    );
    Ok(filter)
}

fn order_by_priority(models: &mut [ModelConfig]) {
    let order = crate::model_config::default_text_to_text_priority_chain_ids()
        .iter()
        .enumerate()
        .map(|(index, id)| (*id, index))
        .collect::<HashMap<_, _>>();
    models.sort_by_key(|model| order.get(model.id.as_str()).copied().unwrap_or(usize::MAX));
}

fn levels_from_env() -> Result<BTreeSet<u8>> {
    let value =
        std::env::var("CATALOG_BENCH_LOCALIZATION_LEVELS").unwrap_or_else(|_| "1,2,3".to_string());
    let levels = value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(|item| item.parse::<u8>().context("parse localization level"))
        .collect::<Result<BTreeSet<_>>>()?;
    ensure!(!levels.is_empty(), "localization levels cannot be empty");
    ensure!(
        levels.iter().all(|level| (1..=3).contains(level)),
        "localization levels must be between 1 and 3"
    );
    Ok(levels)
}

fn case_filter() -> Result<Option<HashSet<String>>> {
    let Some(value) = std::env::var_os("CATALOG_BENCH_LOCALIZATION_CASES") else {
        return Ok(None);
    };
    let cases = value
        .to_string_lossy()
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect::<HashSet<_>>();
    ensure!(
        !cases.is_empty(),
        "CATALOG_BENCH_LOCALIZATION_CASES cannot be empty"
    );
    Ok(Some(cases))
}

fn localization_output_dir() -> std::path::PathBuf {
    std::env::var_os("CATALOG_BENCH_OUTPUT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            super::setup::history_root()
                .join("localization-probes")
                .join(format!(
                    "{}-{}",
                    chrono::Utc::now().format("%Y%m%d-%H%M%S-%3f"),
                    std::process::id()
                ))
        })
}

fn rate(characters: usize, milliseconds: u128) -> Option<f64> {
    (characters > 0 && milliseconds > 0)
        .then(|| characters as f64 / (milliseconds as f64 / 1_000.0))
}

fn reference_candidates(
    case: &LocalizationCase,
    image_width: u32,
    image_height: u32,
) -> Vec<DetectedTextRegion> {
    fn scaled(value: u32, extent: u32) -> u16 {
        ((u64::from(value) * 1000 + u64::from(extent) / 2) / u64::from(extent)).min(1000) as u16
    }
    case.regions
        .iter()
        .enumerate()
        .map(|(index, region)| {
            let [x, y, width, height] = region.box_px;
            DetectedTextRegion {
                id: u16::try_from(index + 1).expect("localization fixture cap fits u16"),
                source_text: region.source_text.clone(),
                source_alternatives: vec![region.source_text.clone()],
                recognition: Default::default(),
                bounds: NormalizedBounds {
                    left: scaled(x, image_width),
                    top: scaled(y, image_height),
                    right: scaled(x.saturating_add(width), image_width),
                    bottom: scaled(y.saturating_add(height), image_height),
                },
                appearance: None,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changed_ratio_distinguishes_translations_from_echoes() {
        let document = crate::overlay::screen_translate::contract::TranslationDocument {
            regions: vec![
                crate::overlay::screen_translate::contract::TranslationRegion {
                    id: 1,
                    member_ids: vec![1],
                    member_joins: Vec::new(),
                    selections: vec![
                        crate::overlay::screen_translate::contract::TranslationSelection {
                            region_id: 1,
                            candidate_id: "r1c0".to_string(),
                            source_text: "Settings".to_string(),
                            bounds: [0, 0, 1, 1].into(),
                        },
                    ],
                    semantic_role:
                        crate::overlay::screen_translate::contract::SemanticRole::Standalone,
                    source_text: "Settings".to_string(),
                    translated_segments: vec!["Cài đặt".to_string()],
                    bounds: [0, 0, 1, 1].into(),
                    background_color: None,
                    text_color: None,
                },
                crate::overlay::screen_translate::contract::TranslationRegion {
                    id: 2,
                    member_ids: vec![2],
                    member_joins: Vec::new(),
                    selections: vec![
                        crate::overlay::screen_translate::contract::TranslationSelection {
                            region_id: 2,
                            candidate_id: "r2c0".to_string(),
                            source_text: "SGT".to_string(),
                            bounds: [1, 1, 2, 2].into(),
                        },
                    ],
                    semantic_role:
                        crate::overlay::screen_translate::contract::SemanticRole::Standalone,
                    source_text: "SGT".to_string(),
                    translated_segments: vec!["SGT".to_string()],
                    bounds: [1, 1, 2, 2].into(),
                    background_color: None,
                    text_color: None,
                },
            ],
        };
        assert_eq!(changed_translation_ratio(&document), 0.5);
    }
}
