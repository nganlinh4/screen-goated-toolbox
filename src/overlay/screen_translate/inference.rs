use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, bail};

use crate::api::{TranslateTextRequest, translate_text_streaming};
use crate::retry_model_chain::{
    RetryChainKind, claim_model_attempt, preflight_skip_reason, record_model_failure,
    record_model_success, release_model_probe, resolve_next_retry_model,
};

use super::contract::{
    DetectedTextRegion, TranslationDocument, TranslationRegion, parse_response,
    prompt_with_instruction, response_schema,
};
use super::stream_parser::TranslationStreamParser;

const MAX_UNRESOLVED_TAIL: usize = 3;
const MIN_PARTIAL_COVERAGE_PERCENT: usize = 80;
const MAX_OMITTED_REGION_AREA: u32 = 4_000;
const MAX_TOTAL_OMITTED_AREA: u32 = 8_000;
const MAX_TAIL_FAILURES: usize = 2;
const MAX_CONTENT_ATTEMPTS: usize = 2;
const MAX_TOTAL_ATTEMPTS: usize = 3;
const TAIL_REPAIR_TIMEOUT: Duration = Duration::from_secs(3);

pub(super) struct TranslateInput<'a> {
    pub trace_id: &'a str,
    pub target_language: &'a str,
    pub translation_model: &'a str,
    pub translation_prompt: &'a str,
    pub candidates: &'a [DetectedTextRegion],
    pub image: &'a image::RgbaImage,
}

pub(super) fn translate<F>(
    input: TranslateInput<'_>,
    cancel: Arc<AtomicBool>,
    mut on_event: F,
) -> Result<TranslationDocument>
where
    F: FnMut(TranslationRegion),
{
    let TranslateInput {
        trace_id,
        target_language,
        translation_model,
        translation_prompt,
        candidates,
        image,
    } = input;
    let uncertain = super::vision_fallback::uncertain_members(candidates);
    if uncertain.is_empty() {
        return translate_text(
            trace_id,
            target_language,
            translation_model,
            translation_prompt,
            candidates,
            cancel,
            on_event,
        );
    }
    let vision_candidates = candidates.to_vec();
    let vision_uncertain = uncertain.clone();
    let vision_image = image.clone();
    let vision_target = target_language.to_string();
    let vision_trace = trace_id.to_string();
    let vision_cancel = Arc::clone(&cancel);
    let vision = std::thread::spawn(move || {
        super::vision_fallback::translate(
            &vision_trace,
            &vision_image,
            &vision_target,
            &vision_candidates,
            &vision_uncertain,
            vision_cancel,
        )
    });
    let reliable_candidates = candidates
        .iter()
        .filter(|candidate| !uncertain.contains(&candidate.id))
        .cloned()
        .collect::<Vec<_>>();
    let text_result = if reliable_candidates.is_empty() {
        Ok(TranslationDocument {
            regions: Vec::new(),
        })
    } else {
        translate_text(
            trace_id,
            target_language,
            translation_model,
            translation_prompt,
            &reliable_candidates,
            Arc::clone(&cancel),
            &mut on_event,
        )
    };
    let vision_document = vision
        .join()
        .map_err(|_| anyhow::anyhow!("batched vision fallback panicked"));
    let mut vision_regions = match vision_document {
        Ok(Ok(document)) => document.regions,
        Ok(Err(error)) | Err(error) => {
            crate::log_info!(
                "[Screen Translate] trace={trace_id} batched vision fallback unavailable: {error:#}"
            );
            Vec::new()
        }
    };
    let vision_covered = vision_regions
        .iter()
        .flat_map(|region| region.member_ids.iter().copied())
        .collect::<HashSet<_>>();
    let (text_document, text_error) = match text_result {
        Ok(document) => (document, None),
        Err(error) => {
            crate::log_info!(
                "[Screen Translate] trace={trace_id} text branch unavailable; preserving batched vision output: {error:#}"
            );
            (
                TranslationDocument {
                    regions: Vec::new(),
                },
                Some(error),
            )
        }
    };
    for region in &vision_regions {
        on_event(region.clone());
    }
    let missing_vision = candidates
        .iter()
        .filter(|candidate| {
            uncertain.contains(&candidate.id) && !vision_covered.contains(&candidate.id)
        })
        .cloned()
        .collect::<Vec<_>>();
    let fallback_document = if missing_vision.is_empty() || text_error.is_some() {
        TranslationDocument {
            regions: Vec::new(),
        }
    } else {
        crate::log_info!(
            "[Screen Translate] trace={trace_id} vision left {} member(s) unresolved; routing only those members to text",
            missing_vision.len()
        );
        translate_text(
            trace_id,
            target_language,
            translation_model,
            translation_prompt,
            &missing_vision,
            cancel,
            &mut on_event,
        )?
    };
    let mut regions = text_document.regions;
    regions.append(&mut vision_regions);
    regions.extend(fallback_document.regions);
    regions.sort_by_key(|region| (region.bounds.top, region.bounds.left));
    if regions.is_empty()
        && let Some(error) = text_error
    {
        return Err(error);
    }
    Ok(TranslationDocument { regions })
}

fn translate_text<F>(
    trace_id: &str,
    target_language: &str,
    translation_model: &str,
    translation_prompt: &str,
    candidates: &[DetectedTextRegion],
    cancel: Arc<AtomicBool>,
    mut on_event: F,
) -> Result<TranslationDocument>
where
    F: FnMut(TranslationRegion),
{
    let config = crate::APP
        .lock()
        .map(|app| app.config.clone())
        .map_err(|_| anyhow::anyhow!("app configuration is unavailable"))?;
    let mut failed = Vec::new();
    let mut blocked_providers = HashSet::new();
    let mut accepted = Vec::new();
    let mut covered = HashSet::new();
    let mut tail_failures = 0;
    let mut content_attempts = 0;
    let mut total_attempts = 0;
    let mut current =
        crate::model_config::get_model_by_id_with_custom(translation_model, &config.custom_models)
            .or_else(|| {
                resolve_next_retry_model(
                    translation_model,
                    &failed,
                    &blocked_providers,
                    RetryChainKind::TextToText,
                    &config,
                )
            })
            .context("no text translation model is available")?;

    loop {
        if cancel.load(Ordering::SeqCst) {
            bail!("screen translation was cancelled");
        }
        let pending = pending_candidates(candidates, &covered);
        if let Some(document) = completed_document(candidates, &accepted, &covered) {
            return Ok(document);
        }
        let schema = response_schema(pending.len());
        let request_text = prompt_with_instruction(target_language, translation_prompt, &pending)?;
        let request_timeout = if can_finish_partial(candidates, &covered) {
            TAIL_REPAIR_TIMEOUT
        } else {
            total_attempts += 1;
            Duration::from_secs(20)
        };
        if let Some(reason) =
            preflight_skip_reason(&current.id, &current.provider, &config, &blocked_providers)
                .or_else(|| claim_model_attempt(&current.id))
        {
            crate::log_info!(
                "[Screen Translate] trace={trace_id} model skipped model={} reason={reason}",
                current.id
            );
            failed.push(current.id.clone());
            if crate::overlay::utils::should_block_retry_provider(&reason) {
                blocked_providers.insert(current.provider.clone());
            }
        } else {
            crate::log_info!(
                "[Screen Translate] trace={trace_id} model attempt model={} provider={}",
                current.id,
                current.provider
            );
            let mut parser = TranslationStreamParser::new(&pending);
            let covered_before_attempt = covered.len();
            let attempt_cancel = if request_timeout == TAIL_REPAIR_TIMEOUT {
                let tail_cancel = Arc::new(AtomicBool::new(false));
                let timeout_cancel = Arc::clone(&tail_cancel);
                let job_cancel = Arc::clone(&cancel);
                std::thread::spawn(move || {
                    let deadline = std::time::Instant::now() + TAIL_REPAIR_TIMEOUT;
                    while std::time::Instant::now() < deadline && !job_cancel.load(Ordering::SeqCst)
                    {
                        std::thread::sleep(Duration::from_millis(25));
                    }
                    timeout_cancel.store(true, Ordering::SeqCst);
                });
                tail_cancel
            } else {
                Arc::clone(&cancel)
            };
            let transport = translate_text_streaming(
                TranslateTextRequest {
                    groq_api_key: &config.api_key,
                    gemini_api_key: &config.gemini_api_key,
                    text: request_text.clone(),
                    instruction: "Return only the requested structured screen translation."
                        .to_string(),
                    model: current.full_name.clone(),
                    provider: current.provider.clone(),
                    streaming_enabled: true,
                    use_json_format: true,
                    response_schema: Some(&schema),
                    search_label: None,
                    ui_language: &config.ui_language,
                    cancel_token: Some(attempt_cancel),
                    request_timeout: Some(request_timeout),
                    target_language: Some(target_language.to_string()),
                },
                |chunk| {
                    for (_, region) in parser.push(chunk) {
                        if accept_region(&mut accepted, &mut covered, region.clone()) {
                            on_event(region);
                        }
                    }
                },
            );
            let received_content = transport.is_ok();
            if received_content {
                content_attempts += 1;
            }
            let response = transport.and_then(|response| parse_response(&response, &pending));
            let error = match response {
                Ok(document) => {
                    for region in document.regions {
                        if accept_region(&mut accepted, &mut covered, region.clone()) {
                            on_event(region);
                        }
                    }
                    if let Some(document) = completed_document(candidates, &accepted, &covered) {
                        record_model_success(&current.id);
                        crate::log_info!(
                            "[Screen Translate] trace={trace_id} model complete model={} regions={}",
                            current.id,
                            document.regions.len()
                        );
                        return Ok(document);
                    } else {
                        anyhow::anyhow!(
                            "translation response left {} region(s) unresolved; rejected {} malformed streamed region(s)",
                            pending_candidates(candidates, &covered).len(),
                            parser.rejected_count()
                        )
                    }
                }
                Err(error) => {
                    if let Some(document) = completed_document(candidates, &accepted, &covered) {
                        record_model_success(&current.id);
                        crate::log_info!(
                            "[Screen Translate] trace={trace_id} model complete model={} regions={}",
                            current.id,
                            document.regions.len()
                        );
                        return Ok(document);
                    } else {
                        error
                    }
                }
            };
            if cancel.load(Ordering::SeqCst) {
                release_model_probe(&current.id);
                bail!("screen translation was cancelled");
            }
            if can_finish_partial(candidates, &covered) {
                tail_failures += 1;
                if tail_failures >= MAX_TAIL_FAILURES {
                    let document = assembled_document(&accepted);
                    record_model_success(&current.id);
                    crate::log_info!(
                        "[Screen Translate] trace={trace_id} completed with validated partial output regions={} omitted={}",
                        document.regions.len(),
                        candidates.len().saturating_sub(covered.len())
                    );
                    return Ok(document);
                }
            } else if covered.len() > covered_before_attempt {
                tail_failures = 0;
            }
            record_model_failure(&current.id, &error.to_string());
            if crate::overlay::utils::should_block_retry_provider(&error.to_string()) {
                blocked_providers.insert(current.provider.clone());
            }
            if received_content && content_attempts == 1 {
                blocked_providers.insert(current.provider.clone());
                crate::log_info!(
                    "[Screen Translate] trace={trace_id} backup requires provider diversity after model={}",
                    current.id
                );
            }
            failed.push(current.id.clone());
            crate::log_info!(
                "[Screen Translate] trace={trace_id} text model failed model={} reason={error}",
                current.id
            );
            if content_attempts >= MAX_CONTENT_ATTEMPTS || total_attempts >= MAX_TOTAL_ATTEMPTS {
                if accepted.is_empty() {
                    return Err(error).context("translation models returned no validated output");
                }
                crate::log_info!(
                    "[Screen Translate] trace={trace_id} stopped at bounded fallback budget regions={} omitted={}",
                    accepted.len(),
                    candidates.len().saturating_sub(covered.len())
                );
                return Ok(assembled_document(&accepted));
            }
        }
        current = resolve_next_retry_model(
            &current.id,
            &failed,
            &blocked_providers,
            RetryChainKind::TextToText,
            &config,
        )
        .context("all configured text translation models failed")?;
    }
}

fn pending_candidates(
    candidates: &[DetectedTextRegion],
    covered: &HashSet<u16>,
) -> Vec<DetectedTextRegion> {
    candidates
        .iter()
        .filter(|candidate| !covered.contains(&candidate.id))
        .cloned()
        .collect()
}

fn completed_document(
    candidates: &[DetectedTextRegion],
    accepted: &[TranslationRegion],
    covered: &HashSet<u16>,
) -> Option<TranslationDocument> {
    if candidates
        .iter()
        .any(|candidate| !covered.contains(&candidate.id))
    {
        return None;
    }
    Some(assembled_document(accepted))
}

fn assembled_document(accepted: &[TranslationRegion]) -> TranslationDocument {
    let mut regions = accepted.to_vec();
    regions.sort_by_key(|region| (region.bounds.top, region.bounds.left));
    TranslationDocument { regions }
}

fn can_finish_partial(candidates: &[DetectedTextRegion], covered: &HashSet<u16>) -> bool {
    let pending = candidates
        .iter()
        .filter(|candidate| !covered.contains(&candidate.id))
        .collect::<Vec<_>>();
    let unresolved = pending.len();
    let omitted_areas = pending.iter().map(|candidate| {
        u32::from(candidate.bounds.right.saturating_sub(candidate.bounds.left))
            * u32::from(candidate.bounds.bottom.saturating_sub(candidate.bounds.top))
    });
    let total_omitted_area = omitted_areas.clone().sum::<u32>();
    !covered.is_empty()
        && unresolved > 0
        && unresolved <= MAX_UNRESOLVED_TAIL
        && covered.len() * 100 >= candidates.len() * MIN_PARTIAL_COVERAGE_PERCENT
        && omitted_areas.max().unwrap_or(0) <= MAX_OMITTED_REGION_AREA
        && total_omitted_area <= MAX_TOTAL_OMITTED_AREA
}

fn accept_region(
    accepted: &mut Vec<TranslationRegion>,
    covered: &mut HashSet<u16>,
    region: TranslationRegion,
) -> bool {
    if region.member_ids.iter().any(|id| covered.contains(id)) {
        return false;
    }
    if super::translation_validation::is_suspiciously_unchanged(&region) {
        return false;
    }
    covered.extend(region.member_ids.iter().copied());
    accepted.push(region);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::screen_translate::contract::NormalizedBounds;
    use crate::overlay::screen_translate::stream_parser::TranslationStreamParser;

    fn candidate(id: u16, top: u16) -> DetectedTextRegion {
        DetectedTextRegion {
            id,
            bounds: NormalizedBounds {
                left: id.saturating_mul(200),
                top,
                right: id.saturating_mul(200).saturating_add(80),
                bottom: top + 20,
            },
            source_text: format!("source-{id}"),
            source_alternatives: vec![format!("source-{id}")],
            recognition: Default::default(),
            appearance: None,
        }
    }

    fn translated(candidate: &DetectedTextRegion) -> TranslationRegion {
        TranslationRegion {
            id: candidate.id,
            member_ids: vec![candidate.id],
            member_joins: Vec::new(),
            selections: vec![super::super::contract::TranslationSelection {
                region_id: candidate.id,
                candidate_id: format!("r{}c0", candidate.id),
                source_text: candidate.source_text.clone(),
                bounds: candidate.bounds,
            }],
            semantic_role: super::super::contract::SemanticRole::Standalone,
            source_text: candidate.source_text.clone(),
            translated_segments: vec![format!("translated-{}", candidate.id)],
            bounds: candidate.bounds,
            background_color: None,
            text_color: None,
        }
    }

    #[test]
    fn retry_requests_only_missing_regions_and_keeps_committed_output() {
        let candidates = vec![candidate(1, 80), candidate(2, 20)];
        let mut accepted = vec![translated(&candidates[0])];
        let mut covered = HashSet::from([1]);

        assert_eq!(
            pending_candidates(&candidates, &covered)
                .iter()
                .map(|candidate| candidate.id)
                .collect::<Vec<_>>(),
            vec![2]
        );
        assert!(completed_document(&candidates, &accepted, &covered).is_none());

        assert!(accept_region(
            &mut accepted,
            &mut covered,
            translated(&candidates[1]),
        ));
        let completed = completed_document(&candidates, &accepted, &covered).unwrap();
        assert_eq!(
            completed
                .regions
                .iter()
                .map(|region| region.id)
                .collect::<Vec<_>>(),
            vec![2, 1]
        );
    }

    #[test]
    fn malformed_stream_member_cannot_erase_valid_regions_before_fallback() {
        let candidates = vec![candidate(1, 20), candidate(2, 40), candidate(3, 60)];
        let mut accepted = Vec::new();
        let mut covered = HashSet::new();
        let mut first_attempt = TranslationStreamParser::new(&candidates);
        for (_, region) in first_attempt.push(
            r#"{"members":[{"memberId":1,"translation":"first"},{"memberId":2,"translation":3}]}"#,
        ) {
            accept_region(&mut accepted, &mut covered, region);
        }

        let pending = pending_candidates(&candidates, &covered);
        assert_eq!(
            pending
                .iter()
                .map(|candidate| candidate.id)
                .collect::<Vec<_>>(),
            vec![2, 3]
        );
        assert_eq!(accepted[0].translated_segments, ["first"]);

        let mut fallback = TranslationStreamParser::new(&pending);
        for (_, region) in fallback.push(
            r#"{"members":[{"memberId":2,"translation":"second"},{"memberId":3,"translation":"third"}]}"#,
        ) {
            accept_region(&mut accepted, &mut covered, region);
        }

        let completed = completed_document(&candidates, &accepted, &covered).unwrap();
        assert_eq!(completed.regions.len(), 3);
        assert_eq!(completed.regions[0].translated_segments, ["first"]);
    }

    #[test]
    fn partial_completion_is_reserved_for_a_small_high_coverage_tail() {
        let candidates = (1..=19)
            .map(|id| candidate(id, id.saturating_mul(10)))
            .collect::<Vec<_>>();
        let covered = (1..=18).collect::<HashSet<_>>();
        assert!(can_finish_partial(&candidates, &covered));

        let insufficient = (1..=15).collect::<HashSet<_>>();
        assert!(!can_finish_partial(&candidates, &insufficient));
        assert!(!can_finish_partial(&candidates[..2], &HashSet::from([1])));

        let mut salient = candidates.clone();
        salient[18].bounds = NormalizedBounds {
            left: 0,
            top: 0,
            right: 500,
            bottom: 250,
        };
        assert!(!can_finish_partial(&salient, &covered));
    }
}
