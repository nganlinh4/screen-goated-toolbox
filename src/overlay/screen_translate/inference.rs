use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, bail};

use crate::api::{TranslateTextRequest, translate_text_streaming};
use crate::retry_model_chain::{
    RetryChainKind, claim_model_attempt, preflight_skip_reason, record_model_failure,
    record_model_success, release_model_probe, resolve_next_configured_model,
};

use super::contract::{
    DetectedTextRegion, TranslationDocument, TranslationRegion, parse_response,
    prompt_with_instruction, response_schema,
};
use super::stream_parser::TranslationStreamParser;

const MAX_CONTENT_ATTEMPTS: usize = 2;
const MAX_TOTAL_ATTEMPTS: usize = 4;
const TRANSLATION_TIMEOUT: Duration = Duration::from_secs(20);

pub(super) struct TranslateInput<'a> {
    pub trace_id: &'a str,
    pub target_language: &'a str,
    pub translation_model: &'a str,
    pub translation_prompt: &'a str,
    pub candidates: &'a [DetectedTextRegion],
}

pub(super) fn translate<F>(
    input: TranslateInput<'_>,
    cancel: Arc<AtomicBool>,
    on_event: F,
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
    } = input;
    translate_text(
        trace_id,
        target_language,
        translation_model,
        translation_prompt,
        candidates,
        cancel,
        on_event,
    )
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
    let mut content_attempts = 0;
    let mut total_attempts = 0;
    let mut attempt_sequence = 0_usize;
    let mut current =
        crate::model_config::get_model_by_id_with_custom(translation_model, &config.custom_models)
            .or_else(|| {
                resolve_next_configured_model(
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
        let request_timeout = TRANSLATION_TIMEOUT;
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
            // Only a dispatched request spends the fallback budget; a model skipped
            // for cooldown costs nothing and must leave the budget for a live one.
            total_attempts += 1;
            attempt_sequence += 1;
            crate::log_info!(
                "[Screen Translate] trace={trace_id} model attempt model={} provider={}",
                current.id,
                current.provider
            );
            let mut attempt_trace = super::inference_telemetry::AttemptTrace::new(
                trace_id,
                attempt_sequence,
                &current.id,
                &current.full_name,
                &current.provider,
                pending.len(),
            );
            let mut parser = TranslationStreamParser::new(&pending);
            let covered_before_attempt = covered.len();
            let attempt_cancel = Arc::clone(&cancel);
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
                    attempt_trace.observe_chunk(chunk);
                    for (_, region) in parser.push(chunk) {
                        if accept_region(&mut accepted, &mut covered, region.clone(), candidates) {
                            attempt_trace.observe_validated_region();
                            on_event(region);
                        }
                    }
                },
            );
            attempt_trace.transport_complete();
            let received_content = transport.is_ok();
            if received_content {
                content_attempts += 1;
            }
            let response = transport.and_then(|response| parse_response(&response, &pending));
            let error = match response {
                Ok(document) => {
                    for region in document.regions {
                        if accept_region(&mut accepted, &mut covered, region.clone(), candidates) {
                            attempt_trace.observe_validated_region();
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
                        attempt_trace.finish(
                            "complete",
                            covered.len().saturating_sub(covered_before_attempt),
                            pending_candidates(candidates, &covered).len(),
                            parser.rejected_count(),
                        );
                        return Ok(document);
                    } else {
                        let unresolved = pending_candidates(candidates, &covered);
                        anyhow::anyhow!(
                            "translation response left {} region(s) unresolved {:?}; rejected {} malformed streamed region(s)",
                            unresolved.len(),
                            unresolved
                                .iter()
                                .map(|candidate| candidate.id)
                                .collect::<Vec<_>>(),
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
                        attempt_trace.finish(
                            "complete",
                            covered.len().saturating_sub(covered_before_attempt),
                            pending_candidates(candidates, &covered).len(),
                            parser.rejected_count(),
                        );
                        return Ok(document);
                    } else {
                        error
                    }
                }
            };
            if cancel.load(Ordering::SeqCst) {
                attempt_trace.finish(
                    "cancelled",
                    covered.len().saturating_sub(covered_before_attempt),
                    pending_candidates(candidates, &covered).len(),
                    parser.rejected_count(),
                );
                release_model_probe(&current.id);
                bail!("screen translation was cancelled");
            }
            record_model_failure(&current.id, &error.to_string());
            if crate::overlay::utils::should_block_retry_provider(&error.to_string()) {
                blocked_providers.insert(current.provider.clone());
            }
            failed.push(current.id.clone());
            crate::log_info!(
                "[Screen Translate] trace={trace_id} text model failed model={} reason={error}",
                current.id
            );
            attempt_trace.finish(
                "failed",
                covered.len().saturating_sub(covered_before_attempt),
                pending_candidates(candidates, &covered).len(),
                parser.rejected_count(),
            );
            if content_attempts >= MAX_CONTENT_ATTEMPTS {
                let preserved = preserve_unresolved_candidates(
                    candidates,
                    &mut accepted,
                    &mut covered,
                    &mut on_event,
                );
                if let Some(document) = completed_document(candidates, &accepted, &covered) {
                    crate::log_info!(
                        "[Screen Translate] trace={trace_id} bounded fallback preserved {} unresolved source region(s)",
                        preserved
                    );
                    return Ok(document);
                }
                return Err(error).context("translation models did not resolve every text region");
            }
            if total_attempts >= MAX_TOTAL_ATTEMPTS {
                return Err(error).context("translation models did not resolve every text region");
            }
        }
        current = resolve_next_configured_model(
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

fn preserve_unresolved_candidates<F>(
    candidates: &[DetectedTextRegion],
    accepted: &mut Vec<TranslationRegion>,
    covered: &mut HashSet<u16>,
    on_event: &mut F,
) -> usize
where
    F: FnMut(TranslationRegion),
{
    let unresolved = pending_candidates(candidates, covered);
    for candidate in &unresolved {
        let region = TranslationRegion {
            id: candidate.id,
            member_ids: vec![candidate.id],
            member_joins: Vec::new(),
            selections: vec![super::contract::TranslationSelection {
                region_id: candidate.id,
                candidate_id: format!("r{}c0", candidate.id),
                source_text: candidate.source_text.clone(),
                bounds: candidate.bounds,
            }],
            semantic_role: super::contract::SemanticRole::Standalone,
            source_text: candidate.source_text.clone(),
            translated_segments: vec![candidate.source_text.clone()],
            bounds: candidate.bounds,
            background_color: None,
            text_color: None,
        };
        covered.insert(candidate.id);
        accepted.push(region.clone());
        on_event(region);
    }
    unresolved.len()
}

fn accept_region(
    accepted: &mut Vec<TranslationRegion>,
    covered: &mut HashSet<u16>,
    region: TranslationRegion,
    candidates: &[DetectedTextRegion],
) -> bool {
    if region.member_ids.iter().any(|id| covered.contains(id)) {
        return false;
    }
    let recognition = candidates
        .iter()
        .find(|candidate| candidate.id == region.id)
        .map(|candidate| candidate.recognition)
        .unwrap_or_default();
    if super::translation_validation::is_suspiciously_unchanged(&region, recognition) {
        crate::log_info!(
            "[Screen Translate] member validation rejected member={} reason=unchanged_prose",
            region.id
        );
        return false;
    }
    if super::translation_validation::retains_source_fragment(&region) {
        crate::log_info!(
            "[Screen Translate] member validation rejected member={} reason=source_fragment",
            region.id
        );
        return false;
    }
    covered.extend(region.member_ids.iter().copied());
    accepted.push(region);
    true
}

#[cfg(test)]
#[path = "inference_tests.rs"]
mod tests;
