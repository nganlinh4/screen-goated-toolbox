use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use super::super::translation_providers::{
    TranslationConversationTurn, ValidatedSubtitleTranslationResponse,
};
use super::super::types::{
    SubtitleTranslationItemRequest, SubtitleTranslationJobSnapshot, SubtitleTranslationResultItem,
};
use super::chunking::{sleep_cancelable, translation_retry_delay};
use super::diagnostics::TranslationDiagnostics;
use super::{TRANSLATION_MODEL_ATTEMPTS, update_translation_snapshot};

pub(super) enum AttemptOutcome {
    Succeeded,
    Cancelled,
    Failed,
}

pub(super) struct AttemptModelArgs<'a> {
    pub(super) model_id: &'a str,
    pub(super) model_label: &'a str,
    pub(super) group: &'a [SubtitleTranslationItemRequest],
    pub(super) history: &'a mut Vec<TranslationConversationTurn>,
    pub(super) translated_results: &'a mut Vec<SubtitleTranslationResultItem>,
    pub(super) snapshot: &'a Arc<Mutex<SubtitleTranslationJobSnapshot>>,
    pub(super) cancelled: &'a Arc<AtomicBool>,
    pub(super) total_items: usize,
    pub(super) total_groups: usize,
    pub(super) completed_groups: &'a mut usize,
    pub(super) diagnostics: &'a mut TranslationDiagnostics,
    pub(super) last_error: &'a mut String,
}

/// Runs the retry loop for a single model against `group`. Shared by the GTX
/// path and each catalog candidate; the only per-model difference is the
/// `model_id`/`model_label` shown in the snapshot/diagnostics and `translate_fn`.
pub(super) fn attempt_model(
    args: AttemptModelArgs<'_>,
    translate_fn: &mut impl FnMut(
        &[SubtitleTranslationItemRequest],
        &[TranslationConversationTurn],
    ) -> Result<ValidatedSubtitleTranslationResponse, String>,
) -> Result<AttemptOutcome, String> {
    let AttemptModelArgs {
        model_id,
        model_label,
        group,
        history,
        translated_results,
        snapshot,
        cancelled,
        total_items,
        total_groups,
        completed_groups,
        diagnostics,
        last_error,
    } = args;

    update_translation_snapshot(snapshot, |locked| {
        locked.current_model_id = Some(model_id.to_string());
        locked.current_model_label = Some(model_label.to_string());
        locked.current_chunk_count = total_groups;
        locked.current_chunk_index = *completed_groups + 1;
        locked.total_chunks = total_groups;
        locked.progress = translated_results.len() as f64 / total_items.max(1) as f64;
        locked.results = translated_results.clone();
        locked.message = format!(
            "Translating subtitles with {} ({}/{})",
            model_label,
            *completed_groups + 1,
            total_groups
        );
        locked.message_key = Some("subtitleTranslationStatusChunk".to_string());
        locked.message_params = HashMap::from([
            ("model".to_string(), model_label.to_string()),
            ("current".to_string(), (*completed_groups + 1).to_string()),
            ("total".to_string(), total_groups.to_string()),
        ]);
    })?;

    for attempt_index in 0..TRANSLATION_MODEL_ATTEMPTS {
        if cancelled.load(Ordering::SeqCst) {
            return Ok(AttemptOutcome::Cancelled);
        }
        if attempt_index > 0 {
            let delay = translation_retry_delay(attempt_index);
            update_translation_snapshot(snapshot, |locked| {
                locked.message = format!(
                    "Retrying subtitle translation with {} ({}/{}, attempt {}/{})",
                    model_label,
                    *completed_groups + 1,
                    total_groups,
                    attempt_index + 1,
                    TRANSLATION_MODEL_ATTEMPTS
                );
                locked.message_key = Some("subtitleTranslationStatusRetry".to_string());
                locked.message_params = HashMap::from([
                    ("model".to_string(), model_label.to_string()),
                    ("current".to_string(), (*completed_groups + 1).to_string()),
                    ("total".to_string(), total_groups.to_string()),
                    ("attempt".to_string(), (attempt_index + 1).to_string()),
                    (
                        "attempts".to_string(),
                        TRANSLATION_MODEL_ATTEMPTS.to_string(),
                    ),
                ]);
            })?;
            sleep_cancelable(cancelled, delay);
            if cancelled.load(Ordering::SeqCst) {
                return Ok(AttemptOutcome::Cancelled);
            }
        }

        match translate_fn(group, history) {
            Ok(response) => {
                diagnostics.record_success(model_id, group.len(), attempt_index + 1);
                history.push(TranslationConversationTurn {
                    user_payload: response.user_payload,
                    assistant_payload: response.assistant_payload,
                });
                translated_results.extend(response.items);
                *completed_groups += 1;
                update_translation_snapshot(snapshot, |locked| {
                    locked.progress = translated_results.len() as f64 / total_items.max(1) as f64;
                    locked.results = translated_results.clone();
                })?;
                return Ok(AttemptOutcome::Succeeded);
            }
            Err(error) => {
                diagnostics.record_failure(
                    model_id,
                    model_label,
                    attempt_index + 1,
                    group.len(),
                    history.len(),
                    &error,
                );
                *last_error = error;
            }
        }
    }

    Ok(AttemptOutcome::Failed)
}
