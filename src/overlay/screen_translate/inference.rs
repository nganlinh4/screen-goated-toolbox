use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, bail};

use crate::api::{TranslateTextRequest, translate_text_streaming};
use crate::retry_model_chain::{
    RetryChainKind, preflight_skip_reason, record_model_failure, resolve_next_retry_model,
};

use super::contract::{
    DetectedTextRegion, TranslationDocument, TranslationRegion, parse_response,
    prompt_with_instruction, response_schema,
};
use super::stream_parser::TranslationStreamParser;

pub(super) fn translate<F>(
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
    let mut accepted = HashMap::new();
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
        let pending = pending_candidates(candidates, &accepted);
        if let Some(document) = completed_document(candidates, &accepted) {
            return Ok(document);
        }
        let schema = response_schema(pending.len());
        let request_text = prompt_with_instruction(target_language, translation_prompt, &pending)?;
        if let Some(reason) =
            preflight_skip_reason(&current.id, &current.provider, &config, &blocked_providers)
        {
            failed.push(current.id.clone());
            if crate::overlay::utils::should_block_retry_provider(&reason) {
                blocked_providers.insert(current.provider.clone());
            }
        } else {
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
                        if accepted.insert(region.id, region.clone()).is_none() {
                            on_event(region);
                        }
                    }
                },
            )
            .and_then(|response| parse_response(&response, &pending));
            let error = match response {
                Ok(document) => {
                    for region in document.regions {
                        if accepted.insert(region.id, region.clone()).is_none() {
                            on_event(region);
                        }
                    }
                    if let Some(document) = completed_document(candidates, &accepted) {
                        return Ok(document);
                    }
                    anyhow::anyhow!(
                        "translation response left {} region(s) unresolved; rejected {} malformed streamed region(s)",
                        pending_candidates(candidates, &accepted).len(),
                        parser.rejected_count()
                    )
                }
                Err(error) => {
                    if let Some(document) = completed_document(candidates, &accepted) {
                        return Ok(document);
                    }
                    error
                }
            };
            record_model_failure(&current.id, &error.to_string());
            if crate::overlay::utils::should_block_retry_provider(&error.to_string()) {
                blocked_providers.insert(current.provider.clone());
            }
            failed.push(current.id.clone());
            crate::log_info!(
                "[Screen Translate] text model failed model={} reason={error}",
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
    accepted: &HashMap<u16, TranslationRegion>,
) -> Vec<DetectedTextRegion> {
    candidates
        .iter()
        .filter(|candidate| !accepted.contains_key(&candidate.id))
        .cloned()
        .collect()
}

fn completed_document(
    candidates: &[DetectedTextRegion],
    accepted: &HashMap<u16, TranslationRegion>,
) -> Option<TranslationDocument> {
    let mut regions = candidates
        .iter()
        .map(|candidate| accepted.get(&candidate.id).cloned())
        .collect::<Option<Vec<_>>>()?;
    regions.sort_by_key(|region| (region.bounds.top, region.bounds.left));
    Some(TranslationDocument { regions })
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
        }
    }

    fn translated(candidate: &DetectedTextRegion) -> TranslationRegion {
        TranslationRegion {
            id: candidate.id,
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
        let mut accepted = HashMap::from([(1, translated(&candidates[0]))]);

        assert_eq!(
            pending_candidates(&candidates, &accepted)
                .iter()
                .map(|candidate| candidate.id)
                .collect::<Vec<_>>(),
            vec![2]
        );
        assert!(completed_document(&candidates, &accepted).is_none());

        accepted.insert(2, translated(&candidates[1]));
        let completed = completed_document(&candidates, &accepted).unwrap();
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
        let mut accepted = HashMap::new();
        let mut first_attempt = TranslationStreamParser::new(&candidates);
        for (_, region) in first_attempt.push(
            r#"{"regions":[{"id":1,"sourceCandidateIndex":0,"translatedText":"first"},{"id":2,"sourceCandidateIndex":9,"translatedText":"bad"}]}"#,
        ) {
            accepted.insert(region.id, region);
        }

        let pending = pending_candidates(&candidates, &accepted);
        assert_eq!(
            pending
                .iter()
                .map(|candidate| candidate.id)
                .collect::<Vec<_>>(),
            vec![2, 3]
        );
        assert_eq!(accepted.get(&1).unwrap().translated_text, "first");

        let mut fallback = TranslationStreamParser::new(&pending);
        for (_, region) in fallback.push(
            r#"{"regions":[{"id":2,"sourceCandidateIndex":0,"translatedText":"second"},{"id":3,"sourceCandidateIndex":0,"translatedText":"third"}]}"#,
        ) {
            accepted.insert(region.id, region);
        }

        let completed = completed_document(&candidates, &accepted).unwrap();
        assert_eq!(completed.regions.len(), 3);
        assert_eq!(completed.regions[0].translated_text, "first");
    }
}
