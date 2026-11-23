//! Text-only candidate chain for resource-bound structural authorization.

use anyhow::anyhow;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use crate::api::{TranslateTextRequest, translate_text_streaming};

use super::{CandidateAttempt, CandidateReport, circuit, request_keys, usable_model};

pub(in crate::overlay::computer_control) fn read_text_pref_where(
    instruction: &str,
    context: &str,
    cancel_token: Arc<AtomicBool>,
    request_timeout: Duration,
    mut on_attempt: impl FnMut(&CandidateAttempt),
    mut accept: impl FnMut(&str) -> bool,
) -> CandidateReport {
    let config = super::super::app_config::snapshot();
    let (gemini_key, groq_key) = request_keys(&config);
    let mut attempts = Vec::new();
    let mut last_error = None;
    let mut candidates = Vec::new();
    let input_bytes = instruction.len().saturating_add(context.len());
    for id in crate::retry_model_chain::RetryChainKind::TextToText.effective_chain(&config) {
        if !id.trim().is_empty() && !candidates.contains(&id) {
            candidates.push(id);
        }
    }

    for id in candidates {
        if cancel_token.load(Ordering::SeqCst) {
            last_error = Some(anyhow!("cancelled"));
            break;
        }
        if let Some(remaining) = circuit::remaining(&id) {
            last_error = Some(anyhow!(
                "{id} is cooling down after a rate limit for {}s",
                remaining.as_secs().max(1)
            ));
            continue;
        }
        if let Some(threshold) = circuit::rejects_text_size(&id, input_bytes) {
            last_error = Some(anyhow!(
                "{id} skipped: {input_bytes}-byte text input is at or above its learned {threshold}-byte rejection boundary"
            ));
            continue;
        }
        let Some(model) = usable_model(&id, &config) else {
            continue;
        };

        let response = translate_text_streaming(
            TranslateTextRequest {
                max_output_tokens: None,
                search_enabled: false,
                search_capability: None,
                search_tool_enabled_by_default: None,
                groq_api_key: &groq_key,
                gemini_api_key: &gemini_key,
                text: context.to_string(),
                instruction: instruction.to_string(),
                model: model.full_name.clone(),
                provider: model.provider.clone(),
                streaming_enabled: false,
                use_json_format: true,
                response_schema: None,
                search_label: None,
                ui_language: "en",
                cancel_token: Some(Arc::clone(&cancel_token)),
                request_timeout: Some(crate::api::client::RequestTimeouts::uniform(
                    request_timeout,
                )),
                target_language: None,
            },
            |_| {},
        );
        match response {
            Ok(response) => {
                let trimmed = response.trim();
                let accepted = !trimmed.is_empty() && accept(trimmed);
                let attempt = CandidateAttempt::response(
                    &model.id,
                    &model.provider,
                    response.clone(),
                    accepted,
                );
                on_attempt(&attempt);
                attempts.push(attempt);
                if accepted {
                    return CandidateReport {
                        answer: Ok(trimmed.to_string()),
                        attempts,
                    };
                }
                last_error = Some(if trimmed.is_empty() {
                    anyhow!("{} returned empty", model.id)
                } else {
                    anyhow!("{} did not satisfy the caller contract", model.id)
                });
            }
            Err(error) => {
                let message = error.to_string();
                let attempt = CandidateAttempt::error(&model.id, &model.provider, message.clone());
                on_attempt(&attempt);
                attempts.push(attempt);
                if circuit::is_rate_limit_error(&message) {
                    circuit::cool_down(&model.id);
                }
                if circuit::is_oversize_error(&message) {
                    circuit::learn_text_oversize(&model.id, input_bytes);
                }
                last_error = Some(error);
            }
        }
    }

    CandidateReport {
        answer: Err(last_error.unwrap_or_else(|| anyhow!("no usable text authorization model"))),
        attempts,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn effective_text_chain_remains_distinct_from_image_chain() {
        // A fixture config, not the developer's saved chains.
        let config = crate::config::Config::default();
        let text_chain =
            crate::retry_model_chain::RetryChainKind::TextToText.effective_chain(&config);
        assert!(!text_chain.is_empty());
        assert_ne!(text_chain, config.model_priority_chains.image_to_text);
    }
}
