use super::*;

pub(super) struct NarrationSynthesisAttempt<'a> {
    pub(super) job_id: &'a str,
    pub(super) index: usize,
    pub(super) total: usize,
    pub(super) item_id: &'a str,
    pub(super) clean_text: &'a str,
    pub(super) profile: &'a TtsRequestProfile,
    pub(super) snapshot: &'a Arc<Mutex<SubtitleNarrationJobSnapshot>>,
    pub(super) cancelled: &'a Arc<AtomicBool>,
}

pub(super) fn synthesize_narration_item_with_retries(
    attempt_ctx: NarrationSynthesisAttempt<'_>,
) -> Result<(String, TtsCollectedAudio, usize), String> {
    let NarrationSynthesisAttempt {
        job_id,
        index,
        total,
        item_id,
        clean_text,
        profile,
        snapshot,
        cancelled,
    } = attempt_ctx;
    let mut last_error = String::new();
    for attempt in 1..=NARRATION_TTS_MAX_ATTEMPTS {
        if cancelled.load(Ordering::SeqCst) {
            return Err("Subtitle narration cancelled".to_string());
        }

        if attempt > 1 {
            let _ = update_snapshot(snapshot, |state| {
                state.message = format!(
                    "Retrying narration {}/{} ({}/{})",
                    index + 1,
                    total,
                    attempt,
                    NARRATION_TTS_MAX_ATTEMPTS
                );
            });
        }

        let synth_started_at = std::time::Instant::now();
        let result = TTS_MANAGER
            .synthesize_to_wav_with_profile_cancel(clean_text, profile.clone(), cancelled.clone())
            .map_err(|error| error.to_string())
            .and_then(|audio| {
                media_server::write_managed_narration_wav(job_id, index, &audio.wav_data)
                    .map(|path| (path, audio))
            });
        let synth_elapsed_ms = synth_started_at.elapsed().as_millis();

        match result {
            Ok((path, duration)) => {
                return Ok((path, duration, attempt));
            }
            Err(message) => {
                last_error = message;
                eprintln!(
                    "[Narration][job={}] item {}/{} subtitle_id={} attempt {}/{} failed elapsed_ms={} error={}",
                    job_id,
                    index + 1,
                    total,
                    item_id,
                    attempt,
                    NARRATION_TTS_MAX_ATTEMPTS,
                    synth_elapsed_ms,
                    last_error
                );
                if attempt < NARRATION_TTS_MAX_ATTEMPTS {
                    let delay_ms = NARRATION_TTS_RETRY_BASE_DELAY_MS * attempt as u64;
                    let mut slept_ms = 0;
                    while slept_ms < delay_ms {
                        if cancelled.load(Ordering::SeqCst) {
                            return Err("Subtitle narration cancelled".to_string());
                        }
                        let step_ms = (delay_ms - slept_ms).min(100);
                        std::thread::sleep(std::time::Duration::from_millis(step_ms));
                        slept_ms += step_ms;
                    }
                }
            }
        }
    }

    Err(last_error)
}

pub(super) struct GeminiNarrationRetryRequest<'a> {
    pub(super) job_id: &'a str,
    pub(super) index: usize,
    pub(super) total: usize,
    pub(super) item_id: &'a str,
    pub(super) clean_text: &'a str,
    pub(super) profile: &'a TtsRequestProfile,
}

pub(super) fn synthesize_gemini_narration_group_with_retries(
    request: GeminiNarrationRetryRequest<'_>,
    snapshot: &Arc<Mutex<SubtitleNarrationJobSnapshot>>,
    cancelled: &Arc<AtomicBool>,
) -> Result<(String, TtsCollectedAudio, usize), String> {
    let GeminiNarrationRetryRequest {
        job_id,
        index,
        total,
        item_id,
        clean_text,
        profile,
    } = request;
    let mut last_error = String::new();
    for attempt in 1..=NARRATION_TTS_MAX_ATTEMPTS {
        if cancelled.load(Ordering::SeqCst) {
            return Err("Subtitle narration cancelled".to_string());
        }
        if attempt > 1 {
            let _ = update_snapshot(snapshot, |state| {
                state.message = format!(
                    "Retrying narration {}/{} ({}/{})",
                    index + 1,
                    total,
                    attempt,
                    NARRATION_TTS_MAX_ATTEMPTS
                );
            });
        }

        let synth_started_at = std::time::Instant::now();
        let result = crate::api::tts::worker::synthesize_gemini_live_to_wav_cancel(
            clean_text,
            profile.clone(),
            cancelled.clone(),
        )
        .map_err(|error| error.to_string())
        .and_then(|audio| {
            media_server::write_managed_narration_wav(job_id, index, &audio.wav_data)
                .map(|path| (path, audio))
        });
        let synth_elapsed_ms = synth_started_at.elapsed().as_millis();

        match result {
            Ok((path, audio)) => return Ok((path, audio, attempt)),
            Err(message) => {
                last_error = message;
                eprintln!(
                    "[Narration][job={}] Gemini parallel group {}/{} subtitle_id={} attempt {}/{} failed elapsed_ms={} error={}",
                    job_id,
                    index + 1,
                    total,
                    item_id,
                    attempt,
                    NARRATION_TTS_MAX_ATTEMPTS,
                    synth_elapsed_ms,
                    last_error
                );
                if attempt < NARRATION_TTS_MAX_ATTEMPTS {
                    let delay_ms = NARRATION_TTS_RETRY_BASE_DELAY_MS * attempt as u64;
                    let mut slept_ms = 0;
                    while slept_ms < delay_ms {
                        if cancelled.load(Ordering::SeqCst) {
                            return Err("Subtitle narration cancelled".to_string());
                        }
                        let step_ms = (delay_ms - slept_ms).min(100);
                        std::thread::sleep(std::time::Duration::from_millis(step_ms));
                        slept_ms += step_ms;
                    }
                }
            }
        }
    }
    Err(last_error)
}
