use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

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

mod context;

pub(super) struct TranslationOutcome {
    pub document: TranslationDocument,
    pub unresolved: Vec<u16>,
}

impl From<TranslationDocument> for TranslationOutcome {
    fn from(document: TranslationDocument) -> Self {
        Self {
            document,
            unresolved: Vec::new(),
        }
    }
}

impl TranslationOutcome {
    pub(super) fn warning(&self) -> Option<String> {
        (!self.unresolved.is_empty()).then(|| format!(
            "{} text unit(s) could not be translated; original pixels were preserved (units {:?})",
            self.unresolved.len(), self.unresolved
        ))
    }
}

pub(super) struct TranslateInput<'a> {
    pub trace_id: &'a str,
    pub target_language: &'a str,
    pub translation_model: &'a str,
    pub translation_prompt: &'a str,
    pub candidates: &'a [DetectedTextRegion],
    pub scene: &'a [DetectedTextRegion],
    pub prior_translations: &'a [TranslationRegion],
}

pub(super) fn translate<F>(
    input: TranslateInput<'_>,
    cancel: Arc<AtomicBool>,
    mut on_event: F,
) -> Result<TranslationOutcome>
where
    F: FnMut(TranslationRegion),
{
    let TranslateInput {
        trace_id,
        target_language,
        translation_model,
        translation_prompt,
        candidates,
        scene,
        prior_translations,
    } = input;
    let config = crate::APP
        .lock()
        .map(|app| app.config.clone())
        .map_err(|_| anyhow::anyhow!("app configuration is unavailable"))?;
    let mut failed = Vec::new();
    let mut blocked_providers = HashSet::new();
    let mut accepted = Vec::new();
    let mut covered = HashSet::new();
    let mut attempt_sequence = 0_usize;
    let mut copied_response = Vec::new();
    let selected_model = translation_model;
    let mut current =
        crate::model_config::get_model_by_id_with_custom(selected_model, &config.custom_models)
            .or_else(|| {
                resolve_next_retry_model(
                    selected_model,
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
            return Ok(document.into());
        }
        let schema = response_schema(pending.len());
        let mut request_text =
            prompt_with_instruction(target_language, translation_prompt, &pending)?;
        context::append(
            &mut request_text,
            if scene.is_empty() { candidates } else { scene },
            &pending,
            prior_translations,
            &accepted,
        )?;
        let request_timeout = crate::retry_model_chain::interactive_request_timeouts(
            &current.id,
            &config,
            crate::retry_model_chain::InteractiveRequestWorkload {
                encoded_request_bytes: request_text.len() as u64,
                expected_response_bytes: pending
                    .iter()
                    .map(|region| {
                        (region.source_text.len() as u64)
                            .saturating_mul(3)
                            .saturating_add(64)
                    })
                    .sum(),
            },
        );
        if let Some(reason) =
            preflight_skip_reason(&current.id, &current.provider, &config, &blocked_providers)
                .or_else(|| {
                    (!crate::api::text::supports_structured_translation(&current.provider))
                        .then(|| format!("STRUCTURED_OUTPUT_UNSUPPORTED:{}", current.provider))
                })
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
            // Cooldowns are shared across callers. Failed IDs bound this walk
            // without cutting off the configured chain at an arbitrary count.
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
            let confirm_copied_batch = attempt_sequence == 1 && {
                let mut excluded = failed.clone();
                excluded.push(current.id.clone());
                resolve_next_retry_model(
                    &current.id,
                    &excluded,
                    &blocked_providers,
                    RetryChainKind::TextToText,
                    &config,
                )
                .is_some()
            };
            attempt_trace.request(&request_text, &schema, target_language);
            let covered_before_attempt = covered.len();
            let attempt_cancel = Arc::clone(&cancel);
            let mut reasoning = serde_json::json!({"messages": []});
            crate::api::apply_ordinary_openai_reasoning_policy(
                &mut reasoning,
                &current.provider,
                &current.full_name,
            );
            let needs_reasoning = reasoning
                .get("reasoning_effort")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|effort| effort != "none");
            let transport = translate_text_streaming(
                TranslateTextRequest {
                    groq_api_key: &config.api_key,
                    gemini_api_key: &config.gemini_api_key,
                    text: request_text.clone(),
                    instruction: format!(
                        "Translate into {target_language}. Return only the requested structured screen translation."
                    ),
                    model: current.full_name.clone(),
                    provider: current.provider.clone(),
                    streaming_enabled: true,
                    use_json_format: true,
                    response_schema: Some(&schema),
                    max_output_tokens: Some(completion_budget(&pending, needs_reasoning)),
                    search_label: None,
                    ui_language: &config.ui_language,
                    cancel_token: Some(attempt_cancel),
                    request_timeout: Some(request_timeout),
                    target_language: Some(target_language.to_string()),
                },
                |chunk| {
                    attempt_trace.observe_chunk(chunk);
                    for (_, region) in parser.push(chunk) {
                        // An unchanged label is valid, but a wholly echoed batch
                        // needs one independent attempt before accepting it.
                        if confirm_copied_batch
                            && super::translation_validation::is_unconfirmed_copy(&region)
                        {
                            continue;
                        }
                        if accept_region(&mut accepted, &mut covered, region.clone(), candidates) {
                            attempt_trace.observe_validated_region();
                            on_event(region);
                        }
                    }
                },
            );
            attempt_trace.transport_complete();
            attempt_trace.response(&transport);
            let response = transport
                .and_then(|response| parse_response(&response, &pending))
                .and_then(|document| {
                    if confirm_copied_batch
                        && super::translation_validation::is_copied_batch(&document.regions)
                    {
                        copied_response = document.regions;
                        bail!("translation response copied the complete text batch; independent confirmation required");
                    }
                    Ok(document)
                });
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
                        return Ok(document.into());
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
                        return Ok(document.into());
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
            if confirm_copied_batch && !copied_response.is_empty() {
                // A valid unchanged response is not a provider-health failure.
                release_model_probe(&current.id);
            } else {
                record_model_failure(&current.id, &error.to_string());
            }
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
            if !confirm_copied_batch && !copied_response.is_empty() {
                return Ok(finish_confirmation(
                    &mut copied_response,
                    &mut accepted,
                    &mut covered,
                    candidates,
                    &mut on_event,
                ));
            }
        }
        let Some(next) = resolve_next_retry_model(
            &current.id,
            &failed,
            &blocked_providers,
            RetryChainKind::TextToText,
            &config,
        ) else {
            return Ok(finish_confirmation(
                &mut copied_response,
                &mut accepted,
                &mut covered,
                candidates,
                &mut on_event,
            ));
        };
        current = next;
    }
}

fn finish_confirmation(
    copied: &mut Vec<TranslationRegion>,
    accepted: &mut Vec<TranslationRegion>,
    covered: &mut HashSet<u16>,
    candidates: &[DetectedTextRegion],
    on_event: &mut impl FnMut(TranslationRegion),
) -> TranslationOutcome {
    for region in copied.drain(..) {
        if accept_region(accepted, covered, region.clone(), candidates) {
            on_event(region);
        }
    }
    unresolved_outcome(candidates, accepted, covered)
}

fn completion_budget(candidates: &[DetectedTextRegion], needs_reasoning: bool) -> u32 {
    // Some endpoints cannot disable reasoning. Their completion cap also owns
    // internal tokens, so a short visible answer still needs bounded headroom.
    candidates
        .iter()
        .fold(64_u32, |budget, region| {
            budget
                .saturating_add((region.source_text.len() as u32).saturating_mul(2))
                .saturating_add(12)
        })
        .clamp(if needs_reasoning { 1024 } else { 256 }, 8192)
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

fn unresolved_outcome(
    candidates: &[DetectedTextRegion],
    accepted: &[TranslationRegion],
    covered: &HashSet<u16>,
) -> TranslationOutcome {
    TranslationOutcome {
        document: assembled_document(accepted),
        unresolved: pending_candidates(candidates, covered)
            .iter()
            .map(|candidate| candidate.id)
            .collect(),
    }
}

fn accept_region(
    accepted: &mut Vec<TranslationRegion>,
    covered: &mut HashSet<u16>,
    region: TranslationRegion,
    candidates: &[DetectedTextRegion],
) -> bool {
    if region.member_ids.is_empty()
        || region.member_ids.iter().any(|id| {
            covered.contains(id) || !candidates.iter().any(|candidate| candidate.id == *id)
        })
    {
        return false;
    }
    covered.extend(region.member_ids.iter().copied());
    accepted.push(region);
    true
}

#[cfg(test)]
#[path = "inference_tests.rs"]
mod tests;
