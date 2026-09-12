use anyhow::Result;

use super::contract::{
    COMPACT_OUTPUT_INSTRUCTION, DetectedTextRegion, TranslationDocument, TranslationRegion,
    parse_response, prompt_with_instruction, response_schema,
};
use crate::model_config::ModelConfig;

#[path = "inference/context.rs"]
pub(super) mod context;

pub(crate) struct PreparedTranslationRequest {
    pub text: String,
    pub instruction: String,
    pub schema: serde_json::Value,
    pub max_output_tokens: u32,
}

/// Shared by live screen translation and its production-path diagnostic.
pub(crate) fn prepare(
    model: &ModelConfig,
    target_language: &str,
    translation_prompt: &str,
    candidates: &[DetectedTextRegion],
    scene: &[DetectedTextRegion],
    prior: &[TranslationRegion],
    accepted: &[TranslationRegion],
) -> Result<PreparedTranslationRequest> {
    let mut text = prompt_with_instruction(target_language, translation_prompt, candidates)?;
    context::append(&mut text, scene, candidates, prior, accepted)?;
    text.push_str(COMPACT_OUTPUT_INSTRUCTION);
    let mut reasoning = serde_json::json!({"messages": []});
    crate::api::apply_ordinary_openai_reasoning_policy(
        &mut reasoning,
        &model.provider,
        &model.full_name,
    );
    let needs_reasoning = reasoning
        .get("reasoning_effort")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|effort| effort != "none");
    Ok(PreparedTranslationRequest {
        text,
        instruction: format!(
            "Translate into {target_language}. Return only the requested structured screen translation."
        ),
        schema: response_schema(candidates.len()),
        max_output_tokens: completion_budget(candidates, needs_reasoning),
    })
}

pub(super) fn completion_budget(candidates: &[DetectedTextRegion], needs_reasoning: bool) -> u32 {
    // Completion ceilings also own internal tokens on endpoints that require reasoning.
    candidates
        .iter()
        .fold(64_u32, |budget, region| {
            budget
                .saturating_add((region.source_text.len() as u32).saturating_mul(2))
                .saturating_add(12)
        })
        .clamp(if needs_reasoning { 1024 } else { 256 }, 8192)
}

pub(crate) fn completed_response(
    response: &str,
    candidates: &[DetectedTextRegion],
    streamed_regions: Vec<TranslationRegion>,
) -> Result<TranslationDocument> {
    parse_response(response, candidates).or_else(|error| {
        // Individually validated complete ownership survives a broken outer envelope.
        // Callers must still confirm wholly copied batches before accepting them.
        if streamed_regions.len() == candidates.len() {
            Ok(TranslationDocument {
                regions: streamed_regions,
            })
        } else {
            Err(error)
        }
    })
}
