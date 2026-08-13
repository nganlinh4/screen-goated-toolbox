use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, bail};

use crate::api::{TranslateTextRequest, translate_text_streaming};
use crate::retry_model_chain::{
    RetryChainKind, preflight_skip_reason, record_model_failure, resolve_next_configured_model,
};

use super::contract::{
    DetectedTextRegion, TranslationDocument, TranslationRegion, parse_response, prompt,
    response_schema,
};
use super::stream_parser::TranslationStreamParser;

pub(super) enum TranslationEvent {
    Region(TranslationRegion),
    Reset,
}

pub(super) fn translate<F>(
    target_language: &str,
    candidates: &[DetectedTextRegion],
    cancel: Arc<AtomicBool>,
    mut on_event: F,
) -> Result<TranslationDocument>
where
    F: FnMut(TranslationEvent),
{
    let config = crate::APP
        .lock()
        .map(|app| app.config.clone())
        .map_err(|_| anyhow::anyhow!("app configuration is unavailable"))?;
    let schema = response_schema(candidates.len());
    let request_text = prompt(target_language, candidates)?;
    let mut failed = Vec::new();
    let mut blocked_providers = HashSet::new();
    let mut current = resolve_next_configured_model(
        "",
        &failed,
        &blocked_providers,
        RetryChainKind::TextToText,
        &config,
    )
    .context("no configured text translation model is available")?;

    loop {
        if cancel.load(Ordering::SeqCst) {
            bail!("screen translation was cancelled");
        }
        if let Some(reason) =
            preflight_skip_reason(&current.id, &current.provider, &config, &blocked_providers)
        {
            failed.push(current.id.clone());
            if crate::overlay::utils::should_block_retry_provider(&reason) {
                blocked_providers.insert(current.provider.clone());
            }
        } else {
            let mut parser = TranslationStreamParser::new(candidates);
            let mut stream_error = None;
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
                    if stream_error.is_some() {
                        return;
                    }
                    match parser.push(chunk) {
                        Ok(regions) => {
                            for (_, region) in regions {
                                on_event(TranslationEvent::Region(region));
                            }
                        }
                        Err(error) => stream_error = Some(error),
                    }
                },
            )
            .and_then(|response| stream_error.map_or(Ok(response), Err))
            .and_then(|response| parse_response(&response, candidates));
            match response {
                Ok(document) => {
                    for region in &document.regions {
                        if !parser.emitted(region.id) {
                            on_event(TranslationEvent::Region(region.clone()));
                        }
                    }
                    return Ok(document);
                }
                Err(error) => {
                    if parser.emitted_any() {
                        on_event(TranslationEvent::Reset);
                    }
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
