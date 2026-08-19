use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use image::{Rgba, RgbaImage};

use crate::api::{TranslateImageRequest, translate_image_streaming};
use crate::model_config::ModelType;
use crate::retry_model_chain::{
    RetryChainKind, claim_model_attempt, preflight_skip_reason, record_model_failure,
    record_model_success, release_model_probe, resolve_next_retry_model,
};

use super::contract::{DetectedTextRegion, TranslationDocument, parse_response, response_schema};

const VISION_TIMEOUT: Duration = Duration::from_secs(8);
const MAX_VISION_ATTEMPTS: usize = 3;
const LOW_RECOGNITION_CONFIDENCE: f32 = 0.72;
const CLOSE_COMPETITOR_MARGIN: f32 = 0.06;

pub(super) fn uncertain_members(candidates: &[DetectedTextRegion]) -> HashSet<u16> {
    super::cell_proposals::propose(candidates)
        .into_iter()
        .filter(|cell| {
            cell.member_ids_in_reading_order.iter().any(|id| {
                candidates
                    .iter()
                    .find(|candidate| candidate.id == *id)
                    .is_some_and(is_uncertain)
            })
        })
        .flat_map(|cell| cell.member_ids_in_reading_order)
        .collect()
}

fn is_uncertain(candidate: &DetectedTextRegion) -> bool {
    let evidence = candidate.recognition;
    evidence.selected_confidence < LOW_RECOGNITION_CONFIDENCE
        || evidence.locator_confidence < 0.60
        || (candidate.source_alternatives.len() > 1
            && evidence.competing_confidence + CLOSE_COMPETITOR_MARGIN
                >= evidence.selected_confidence)
}

pub(super) fn translate(
    trace_id: &str,
    image: &RgbaImage,
    target_language: &str,
    candidates: &[DetectedTextRegion],
    uncertain: &HashSet<u16>,
    cancel: Arc<AtomicBool>,
) -> Result<TranslationDocument> {
    let config = crate::APP
        .lock()
        .map(|app| app.config.clone())
        .map_err(|_| anyhow::anyhow!("app configuration is unavailable"))?;
    let cells = super::cell_proposals::propose(candidates)
        .into_iter()
        .filter(|cell| {
            cell.member_ids_in_reading_order
                .iter()
                .any(|id| uncertain.contains(id))
        })
        .collect::<Vec<_>>();
    if cells.is_empty() {
        bail!("vision fallback has no uncertain cells");
    }
    let atlas = build_atlas(image, candidates, &cells)?;
    let mapping = cells
        .iter()
        .enumerate()
        .map(|(index, cell)| {
            format!(
                "panel {} = memberIds {:?}",
                index + 1,
                cell.member_ids_in_reading_order
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    let prompt = format!(
        "This image is a top-to-bottom atlas of uncertain screen-text cells separated by colored bars. {mapping}. Read each panel directly from its pixels and translate all of its natural-language text completely into {target_language}. Preserve names, usernames, handles, codes, punctuation, tone, and line meaning. Never use OCR guesses from outside the image. Return exactly one members entry for every mapped memberId. Keep each translation with its corresponding memberId without moving, merging, duplicating, or dropping content. Return only JSON matching the schema."
    );
    crate::log_info!(
        "[Screen Translate] trace={trace_id} batched vision fallback cells={} atlas={}x{}",
        cells.len(),
        atlas.width(),
        atlas.height()
    );

    let mut failed = Vec::new();
    let mut blocked_providers = HashSet::new();
    let mut attempts = 0;
    let mut current = first_vision_model(&config, &blocked_providers)
        .context("no configured vision model is available")?;
    loop {
        if cancel.load(Ordering::SeqCst) {
            bail!("screen translation was cancelled");
        }
        if let Some(reason) =
            preflight_skip_reason(&current.id, &current.provider, &config, &blocked_providers)
                .or_else(|| claim_model_attempt(&current.id))
        {
            crate::log_info!(
                "[Screen Translate] trace={trace_id} vision model skipped model={} reason={reason}",
                current.id
            );
            failed.push(current.id.clone());
            if crate::overlay::utils::should_block_retry_provider(&reason) {
                blocked_providers.insert(current.provider.clone());
            }
        } else {
            attempts += 1;
            crate::log_info!(
                "[Screen Translate] trace={trace_id} vision model attempt model={} provider={}",
                current.id,
                current.provider
            );
            let started = Instant::now();
            let outcome = request_translation(
                &config,
                &current,
                prompt.clone(),
                atlas.clone(),
                uncertain.len(),
                Arc::clone(&cancel),
            )
            .and_then(|response| {
                let document = parse_response(&response, candidates)?;
                Ok(TranslationDocument {
                    regions: document
                        .regions
                        .into_iter()
                        .filter(|region| region.member_ids.iter().any(|id| uncertain.contains(id)))
                        .collect(),
                })
            });
            match outcome {
                Ok(document) => {
                    record_model_success(&current.id);
                    crate::log_info!(
                        "[Screen Translate] trace={trace_id} batched vision fallback complete model={} elapsed_ms={}",
                        current.id,
                        started.elapsed().as_millis()
                    );
                    return Ok(document);
                }
                Err(error) => {
                    if cancel.load(Ordering::SeqCst) {
                        release_model_probe(&current.id);
                        bail!("screen translation was cancelled");
                    }
                    crate::log_info!(
                        "[Screen Translate] trace={trace_id} vision model failed model={} reason={error}",
                        current.id
                    );
                    record_model_failure(&current.id, &error.to_string());
                    if crate::overlay::utils::should_block_retry_provider(&error.to_string()) {
                        blocked_providers.insert(current.provider.clone());
                    }
                    failed.push(current.id.clone());
                    if attempts >= MAX_VISION_ATTEMPTS {
                        return Err(error).context("vision models returned no validated output");
                    }
                }
            }
        }
        current = resolve_next_retry_model(
            &current.id,
            &failed,
            &blocked_providers,
            RetryChainKind::ImageToText,
            &config,
        )
        .context("all configured vision models failed")?;
    }
}

/// First vision model in the configured chain, ignoring cooldown so the attempt
/// loop reports the skip and advances instead of failing the whole fallback.
fn first_vision_model(
    config: &crate::config::Config,
    blocked_providers: &HashSet<String>,
) -> Option<crate::model_config::ModelConfig> {
    config
        .model_priority_chains
        .image_to_text
        .iter()
        .filter_map(|id| {
            crate::model_config::get_model_by_id_with_custom(id, &config.custom_models)
        })
        .find(|model| {
            model.model_type == ModelType::Vision
                && !crate::model_config::model_is_non_llm(&model.id)
        })
        .or_else(|| {
            resolve_next_retry_model(
                "",
                &[],
                blocked_providers,
                RetryChainKind::ImageToText,
                config,
            )
        })
}

fn request_translation(
    config: &crate::config::Config,
    model: &crate::model_config::ModelConfig,
    prompt: String,
    atlas: RgbaImage,
    member_count: usize,
    cancel: Arc<AtomicBool>,
) -> Result<String> {
    translate_image_streaming(
        TranslateImageRequest {
            groq_api_key: &config.api_key,
            gemini_api_key: &config.gemini_api_key,
            prompt,
            model: model.full_name.clone(),
            provider: model.provider.clone(),
            image: atlas,
            original_bytes: None,
            streaming_enabled: false,
            response_schema: Some(response_schema(member_count)),
            cancel_token: Some(cancel),
            request_timeout: Some(VISION_TIMEOUT),
        },
        |_| {},
    )
}

fn build_atlas(
    image: &RgbaImage,
    candidates: &[DetectedTextRegion],
    cells: &[super::cell_proposals::CellProposal],
) -> Result<RgbaImage> {
    const SEPARATOR: u32 = 8;
    let crops = cells
        .iter()
        .map(|cell| {
            let members = cell
                .member_ids_in_reading_order
                .iter()
                .filter_map(|id| candidates.iter().find(|candidate| candidate.id == *id))
                .collect::<Vec<_>>();
            let left = members
                .iter()
                .map(|item| item.bounds.left)
                .min()
                .unwrap_or(0);
            let top = members
                .iter()
                .map(|item| item.bounds.top)
                .min()
                .unwrap_or(0);
            let right = members
                .iter()
                .map(|item| item.bounds.right)
                .max()
                .unwrap_or(1000);
            let bottom = members
                .iter()
                .map(|item| item.bounds.bottom)
                .max()
                .unwrap_or(1000);
            let x = u32::from(left) * image.width() / 1000;
            let y = u32::from(top) * image.height() / 1000;
            let end_x = (u32::from(right) * image.width())
                .div_ceil(1000)
                .min(image.width());
            let end_y = (u32::from(bottom) * image.height())
                .div_ceil(1000)
                .min(image.height());
            image::imageops::crop_imm(
                image,
                x,
                y,
                end_x.saturating_sub(x).max(1),
                end_y.saturating_sub(y).max(1),
            )
            .to_image()
        })
        .collect::<Vec<_>>();
    let width = crops
        .iter()
        .map(RgbaImage::width)
        .max()
        .context("atlas is empty")?;
    let height = crops.iter().map(RgbaImage::height).sum::<u32>()
        + SEPARATOR.saturating_mul(crops.len().saturating_sub(1) as u32);
    let mut atlas = RgbaImage::from_pixel(width, height, Rgba([245, 245, 245, 255]));
    let mut y = 0u32;
    for (index, crop) in crops.iter().enumerate() {
        image::imageops::replace(&mut atlas, crop, 0, i64::from(y));
        y += crop.height();
        if index + 1 < crops.len() {
            let color = if index % 2 == 0 {
                Rgba([255, 0, 180, 255])
            } else {
                Rgba([0, 180, 255, 255])
            };
            for row in y..y + SEPARATOR {
                for x in 0..width {
                    atlas.put_pixel(x, row, color);
                }
            }
            y += SEPARATOR;
        }
    }
    Ok(atlas)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::screen_translate::contract::{NormalizedBounds, RecognitionEvidence};

    fn candidate(id: u16, confidence: f32, competitor: f32) -> DetectedTextRegion {
        DetectedTextRegion {
            id,
            bounds: NormalizedBounds {
                left: 0,
                top: (id - 1) * 200,
                right: 500,
                bottom: (id - 1) * 200 + 100,
            },
            source_text: format!("text {id}"),
            source_alternatives: if competitor > 0.0 {
                vec![format!("text {id}"), format!("other {id}")]
            } else {
                vec![format!("text {id}")]
            },
            recognition: RecognitionEvidence {
                locator_confidence: 0.9,
                selected_confidence: confidence,
                competing_confidence: competitor,
            },
            appearance: None,
        }
    }

    #[test]
    fn uncertainty_expands_to_the_complete_local_cell() {
        let first = candidate(1, 0.95, 0.0);
        let mut second = candidate(2, 0.60, 0.0);
        second.bounds = [105, 0, 205, 500].into();
        let uncertain = uncertain_members(&[first, second]);
        assert_eq!(uncertain, HashSet::from([1, 2]));
    }

    #[test]
    fn close_recognizer_disagreement_routes_to_vision() {
        let uncertain = uncertain_members(&[candidate(1, 0.91, 0.87)]);
        assert_eq!(uncertain, HashSet::from([1]));
        assert!(uncertain_members(&[candidate(1, 0.91, 0.50)]).is_empty());
    }

    #[test]
    fn atlas_contains_every_uncertain_cell_in_one_image() {
        let candidates = vec![candidate(1, 0.6, 0.0), candidate(2, 0.6, 0.0)];
        let image = RgbaImage::from_pixel(100, 100, Rgba([20, 30, 40, 255]));
        let cells = super::super::cell_proposals::propose(&candidates);
        let atlas = build_atlas(&image, &candidates, &cells).unwrap();
        assert_eq!(atlas.width(), 50);
        assert!(atlas.height() >= 20);
    }
}
