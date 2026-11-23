//! Validation of the structured patch response every LLM provider returns.

use serde::Deserialize;

use super::ValidatedTranslationResponse;
use crate::api::realtime_audio::state::TranslationRequest;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TranslationModelResponse {
    patches: Vec<TranslationModelPatch>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TranslationModelPatch {
    source_start: usize,
    source_end: usize,
    state: String,
    translation: String,
}

pub(super) fn parse_translation_response(
    payload: &str,
    request: &TranslationRequest,
) -> Option<ValidatedTranslationResponse> {
    let trimmed = payload.trim();
    if trimmed.is_empty() {
        return None;
    }

    let without_prefix = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .unwrap_or(trimmed);
    let cleaned = without_prefix
        .strip_suffix("```")
        .unwrap_or(without_prefix)
        .trim();

    let response: TranslationModelResponse = serde_json::from_str(cleaned).ok()?;
    validate_translation_response(response, request)
}

fn validate_translation_response(
    response: TranslationModelResponse,
    request: &TranslationRequest,
) -> Option<ValidatedTranslationResponse> {
    let finalized_translation = if request.finalized_source.is_empty() {
        String::new()
    } else {
        response
            .patches
            .iter()
            .find(|patch| {
                patch.state == "final"
                    && patch.source_start == request.source_start
                    && patch.source_end == request.finalized_source_end
                    && !patch.translation.trim().is_empty()
            })?
            .translation
            .trim()
            .to_string()
    };

    let draft_translation = if request.draft_source.is_empty() {
        String::new()
    } else {
        match response.patches.iter().find(|patch| {
            patch.state == "draft"
                && patch.source_start == request.draft_source_start()
                && patch.source_end == request.source_end
                && !patch.translation.trim().is_empty()
        }) {
            Some(patch) => patch.translation.trim().to_string(),
            None if !request.requires_draft_translation() => request.fallback_draft_translation(),
            None => return None,
        }
    };

    Some(ValidatedTranslationResponse {
        finalized_translation,
        draft_translation,
    })
}
