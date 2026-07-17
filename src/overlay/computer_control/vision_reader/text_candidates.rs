//! Text-only candidate chain for resource-bound structural authorization.

use anyhow::anyhow;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use crate::api::{TranslateTextRequest, translate_text_streaming};
use crate::model_config::{get_model_by_id_with_custom, model_is_non_llm};

use super::{CandidateAttempt, CandidateReport, circuit, key_for};

pub(in crate::overlay::computer_control) fn read_text_pref_where(
    instruction: &str,
    context: &str,
    cancel_token: Arc<AtomicBool>,
    request_timeout: Duration,
    mut on_attempt: impl FnMut(&CandidateAttempt),
    mut accept: impl FnMut(&str) -> bool,
) -> CandidateReport {
    let config = crate::load_config();
    let gemini_key = key_for("google", &config).unwrap_or_default();
    let groq_key = key_for("groq", &config).unwrap_or_default();
    let mut attempts = Vec::new();
    let mut last_error = None;
    let mut candidates = Vec::new();
    let input_bytes = instruction.len().saturating_add(context.len());
    for id in &config.model_priority_chains.text_to_text {
        if !id.trim().is_empty() && !candidates.contains(id) {
            candidates.push(id.clone());
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
        let Some(model) = get_model_by_id_with_custom(&id, &config.custom_models) else {
            continue;
        };
        if model_is_non_llm(&model.id) {
            continue;
        }
        let provider_needs_no_key = matches!(model.provider.as_str(), "ollama" | "taalas");
        if !provider_needs_no_key && key_for(&model.provider, &config).is_none() {
            continue;
        }

        let response = translate_text_streaming(
            TranslateTextRequest {
                groq_api_key: &groq_key,
                gemini_api_key: &gemini_key,
                text: context.to_string(),
                instruction: instruction.to_string(),
                model: model.full_name.clone(),
                provider: model.provider.clone(),
                streaming_enabled: false,
                use_json_format: true,
                search_label: None,
                ui_language: "en",
                cancel_token: Some(Arc::clone(&cancel_token)),
                request_timeout: Some(request_timeout),
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
    fn configured_text_chain_remains_distinct_from_image_chain() {
        let config = crate::load_config();
        assert!(!config.model_priority_chains.text_to_text.is_empty());
        assert_ne!(
            config.model_priority_chains.text_to_text,
            config.model_priority_chains.image_to_text
        );
    }
}
