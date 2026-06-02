use super::*;

struct ParallelNarrationGroupResult {
    group_index: usize,
    group_total: usize,
    group: NarrationRequestGroup,
    result: Result<(String, TtsCollectedAudio, usize), String>,
}

pub(super) fn run_gemini_subtitle_narration_parallel(
    job_id: &str,
    total_items: usize,
    groups: &[NarrationRequestGroup],
    profile: &TtsRequestProfile,
    grouping: &SubtitleNarrationGroupingRequest,
    snapshot: Arc<Mutex<SubtitleNarrationJobSnapshot>>,
    cancelled: Arc<AtomicBool>,
) {
    let parallel = profile.gemini_parallel_requests.clamp(1, 4);
    eprintln!(
        "[Narration][job={}] Gemini parallel generation groups={} parallel={}",
        job_id,
        groups.len(),
        parallel
    );
    let (tx, rx) = mpsc::channel::<ParallelNarrationGroupResult>();
    let mut next_index = 0usize;
    let mut active = 0usize;

    while (next_index < groups.len() || active > 0) && !cancelled.load(Ordering::SeqCst) {
        while active < parallel && next_index < groups.len() && !cancelled.load(Ordering::SeqCst) {
            let group_index = next_index;
            next_index += 1;
            let group = groups[group_index].clone();
            let tx = tx.clone();
            let profile = profile.clone();
            let cancelled = cancelled.clone();
            let thread_job_id = job_id.to_string();
            let snapshot_for_retry = snapshot.clone();
            let group_total = groups.len();
            let first_item_id = group
                .items
                .first()
                .map(|item| item.id.clone())
                .unwrap_or_default();
            let group_text = group.text.clone();
            let group_for_result = group.clone();
            active += 1;
            let _ = update_snapshot(&snapshot, |state| {
                state.active_subtitle_id = Some(first_item_id.clone());
                state.message = format!(
                    "Generating narration {}/{}",
                    (state.completed_items + 1).min(total_items),
                    total_items
                );
            });
            std::thread::spawn(move || {
                let result = synthesize_gemini_narration_group_with_retries(
                    GeminiNarrationRetryRequest {
                        job_id: &thread_job_id,
                        index: group_index,
                        total: group_total,
                        item_id: &first_item_id,
                        clean_text: &group_text,
                        profile: &profile,
                    },
                    &snapshot_for_retry,
                    &cancelled,
                );
                let _ = tx.send(ParallelNarrationGroupResult {
                    group_index,
                    group_total,
                    group: group_for_result,
                    result,
                });
            });
        }

        match rx.recv_timeout(std::time::Duration::from_millis(100)) {
            Ok(group_result) => {
                active = active.saturating_sub(1);
                apply_narration_group_result(
                    job_id,
                    total_items,
                    grouping,
                    &snapshot,
                    group_result,
                    profile.language_code_override.as_deref(),
                );
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    let _ = update_snapshot(&snapshot, |state| {
        state.active_subtitle_id = None;
        if cancelled.load(Ordering::SeqCst) {
            state.state = "cancelled".to_string();
            state.message = "Subtitle narration cancelled".to_string();
            return;
        }
        state.progress = 1.0;
        if state.results.is_empty() && !state.errors.is_empty() {
            state.state = "error".to_string();
            let first_error = state
                .errors
                .first()
                .map(|error| error.message.clone())
                .unwrap_or_else(|| "Subtitle narration failed".to_string());
            state.message = format!("Subtitle narration failed: {}", first_error);
            state.error = Some(first_error);
        } else {
            state.state = "completed".to_string();
            state.message = if state.errors.is_empty() {
                "Subtitle narration complete".to_string()
            } else {
                format!(
                    "Subtitle narration complete with {} failed item(s)",
                    state.errors.len()
                )
            };
        }
    });
}

fn apply_narration_group_result(
    job_id: &str,
    total_items: usize,
    grouping: &SubtitleNarrationGroupingRequest,
    snapshot: &Arc<Mutex<SubtitleNarrationJobSnapshot>>,
    group_result: ParallelNarrationGroupResult,
    language_code_override: Option<&str>,
) {
    let ParallelNarrationGroupResult {
        group_index,
        group_total,
        group,
        result,
    } = group_result;
    let Some(first_item) = group.items.first() else {
        return;
    };
    match &result {
        Ok((path, audio, attempts)) => eprintln!(
            "[Narration][job={}] group {}/{} group_id={} items={} OK duration_sec={:.3} attempts={} path={}",
            job_id,
            group_index + 1,
            group_total,
            group.id,
            group.items.len(),
            audio.duration_ms as f64 / 1000.0,
            attempts,
            path
        ),
        Err(message) => eprintln!(
            "[Narration][job={}] group {}/{} group_id={} first_subtitle_id={} FAILED attempts={} error={}",
            job_id,
            group_index + 1,
            group_total,
            group.id,
            first_item.id,
            NARRATION_TTS_MAX_ATTEMPTS,
            message
        ),
    }

    let _ = update_snapshot(snapshot, |state| {
        state.completed_items += group.items.len();
        state.progress = state.completed_items as f64 / total_items.max(1) as f64;
        match result {
            Ok((path, audio, _attempts)) => {
                let duration = audio.duration_ms as f64 / 1000.0;
                let split = align_group_audio_ranges(
                    &group,
                    &path,
                    &audio,
                    language_code_override,
                    grouping.vad_search_radius_sec,
                );
                let alignment_mode = split.mode.to_string();
                let take_id = format!("{job_id}-{}", group.id);
                for (item, range) in group.items.iter().zip(split.ranges.into_iter()) {
                    let result = SubtitleNarrationResult {
                        subtitle_id: item.id.clone(),
                        text: item.text.clone(),
                        path: path.clone(),
                        duration,
                        source_in_point: range.start_sec,
                        source_out_point: range.end_sec,
                        group_id: group.id.clone(),
                        narration_group_take_id: take_id.clone(),
                        narration_group_prompt_text: group.text.clone(),
                        narration_group_source_start_time: first_item.start_time,
                        alignment_mode: alignment_mode.clone(),
                        alignment_confidence: range.confidence,
                        start_time: item.start_time,
                        end_time: item.end_time,
                    };
                    state.results.push(result.clone());
                    state.results_revision += 1;
                    let revision = state.results_revision;
                    state
                        .result_events
                        .push(SubtitleNarrationResultEvent { revision, result });
                }
            }
            Err(message) => {
                for item in &group.items {
                    state.errors.push(SubtitleNarrationError {
                        subtitle_id: item.id.clone(),
                        message: message.clone(),
                    });
                }
            }
        }
    });
}
