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

pub(super) fn translate<F>(
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
            let response = translate_text_streaming(
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
                    cancel_token: Some(Arc::clone(&cancel)),
                    request_timeout: Some(Duration::from_secs(20)),
                    target_language: Some(target_language.to_string()),
                },
                |chunk| {
                    for (_, region) in parser.push(chunk) {
                        if accept_region(&mut accepted, &mut covered, region.clone()) {
                            on_event(region);
                        }
                    }
                },
            )
            .and_then(|response| parse_response(&response, &pending));
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
            record_model_failure(&current.id, &error.to_string());
            if crate::overlay::utils::should_block_retry_provider(&error.to_string()) {
                blocked_providers.insert(current.provider.clone());
            }
            failed.push(current.id.clone());
            crate::log_info!(
                "[Screen Translate] trace={trace_id} text model failed model={} reason={error}",
                current.id
            );
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
    let mut regions = accepted.to_vec();
    regions.sort_by_key(|region| (region.bounds.top, region.bounds.left));
    Some(TranslationDocument { regions })
}

fn accept_region(
    accepted: &mut Vec<TranslationRegion>,
    covered: &mut HashSet<u16>,
    region: TranslationRegion,
) -> bool {
    if region.member_ids.iter().any(|id| covered.contains(id)) {
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
                left: 10,
                top,
                right: 90,
                bottom: top + 20,
            },
            source_text: format!("source-{id}"),
            source_alternatives: vec![format!("source-{id}")],
            appearance: None,
        }
    }

    fn translated(candidate: &DetectedTextRegion) -> TranslationRegion {
        TranslationRegion {
            id: candidate.id,
            member_ids: vec![candidate.id],
            selections: vec![super::super::contract::TranslationSelection {
                region_id: candidate.id,
                candidate_id: format!("r{}c0", candidate.id),
                source_text: candidate.source_text.clone(),
                bounds: candidate.bounds,
            }],
            semantic_role: super::super::contract::SemanticRole::Standalone,
            source_text: candidate.source_text.clone(),
            translated_text: format!("translated-{}", candidate.id),
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
            r#"{"regions":[{"regionId":1,"candidateId":"r1c0","translationRequirement":"translation_required","translatedText":"first"},{"regionId":2,"candidateId":"bad","translationRequirement":"translation_required","translatedText":"bad"}]}"#,
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
        assert_eq!(accepted[0].translated_text, "first");

        let mut fallback = TranslationStreamParser::new(&pending);
        for (_, region) in fallback.push(
            r#"{"regions":[{"regionId":2,"candidateId":"r2c0","translationRequirement":"translation_required","translatedText":"second"},{"regionId":3,"candidateId":"r3c0","translationRequirement":"translation_required","translatedText":"third"}]}"#,
        ) {
            accept_region(&mut accepted, &mut covered, region);
        }

        let completed = completed_document(&candidates, &accepted, &covered).unwrap();
        assert_eq!(completed.regions.len(), 3);
        assert_eq!(completed.regions[0].translated_text, "first");
    }

    #[test]
    fn source_equivalence_ignores_layout_whitespace_and_punctuation() {
        let candidate = candidate(1, 20);
        let mut region = translated(&candidate);
        region.source_text = "첫째 줄\n둘째 줄.".to_string();
        region.translated_text = "첫째 줄 둘째 줄".to_string();
        assert!(super::super::contract::text_is_source_equivalent(
            &region.source_text,
            &region.translated_text
        ));

        region.translated_text = "Dòng thứ nhất, dòng thứ hai.".to_string();
        assert!(!super::super::contract::text_is_source_equivalent(
            &region.source_text,
            &region.translated_text
        ));
    }

    #[test]
    fn model_declared_already_target_text_is_accepted_without_a_retry() {
        let candidate = candidate(1, 20);
        let mut region = translated(&candidate);
        region.translated_text = region.source_text.clone();
        let mut accepted = Vec::new();
        let mut covered = HashSet::new();
        assert!(accept_region(&mut accepted, &mut covered, region));
        assert_eq!(covered, HashSet::from([candidate.id]));
        assert_eq!(accepted.len(), 1);
    }
}
