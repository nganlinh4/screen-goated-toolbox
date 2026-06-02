use super::*;

pub(super) fn run_subtitle_narration(
    job_id: &str,
    request: SubtitleNarrationRequest,
    profile: TtsRequestProfile,
    snapshot: Arc<Mutex<SubtitleNarrationJobSnapshot>>,
    cancelled: Arc<AtomicBool>,
) {
    let total = request.items.len();
    eprintln!("[Narration][job={}] running total_items={}", job_id, total);
    let _ = update_snapshot(&snapshot, |state| {
        state.state = "running".to_string();
        state.message = "Generating narration audio".to_string();
        state.progress = 0.0;
        state.total_items = total;
    });

    let mut clean_items = Vec::new();
    for (index, item) in request.items.iter().enumerate() {
        if cancelled.load(Ordering::SeqCst) {
            eprintln!(
                "[Narration][job={}] cancelled at item {}/{}",
                job_id,
                index + 1,
                total
            );
            let _ = update_snapshot(&snapshot, |state| {
                state.state = "cancelled".to_string();
                state.message = "Subtitle narration cancelled".to_string();
                state.active_subtitle_id = None;
            });
            return;
        }

        let clean_text = item.text.trim();
        if clean_text.is_empty() {
            eprintln!(
                "[Narration][job={}] skip empty item {}/{} subtitle_id={}",
                job_id,
                index + 1,
                total,
                item.id
            );
            let _ = update_snapshot(&snapshot, |state| {
                state.completed_items += 1;
                state.progress = state.completed_items as f64 / total.max(1) as f64;
            });
            continue;
        }

        let Some(narration_text) = normalize_narration_input_text(clean_text, &profile.method)
        else {
            eprintln!(
                "[Narration][job={}] skip invalid narration text item {}/{} subtitle_id={} text_json={}",
                job_id,
                index + 1,
                total,
                item.id,
                serde_json::to_string(clean_text)
                    .unwrap_or_else(|_| "\"<unserializable>\"".to_string())
            );
            let _ = update_snapshot(&snapshot, |state| {
                state.completed_items += 1;
                state.progress = state.completed_items as f64 / total.max(1) as f64;
            });
            continue;
        };

        let tts_text = prepare_narration_tts_text(&narration_text, &profile.method);
        clean_items.push(CleanNarrationItem {
            id: item.id.clone(),
            text: clean_text.to_string(),
            text_units: estimate_narration_speech_units(&narration_text),
            aligner_text: normalize_alignment_text(&narration_text),
            tts_text,
            start_time: item.start_time,
            end_time: item.end_time,
        });
    }

    let groups = build_narration_groups(clean_items, &request.grouping);
    eprintln!(
        "[Narration][job={}] grouped total_items={} groups={} text_budget={} vad_radius={:.2}",
        job_id,
        total,
        groups.len(),
        request.grouping.text_budget_units,
        request.grouping.vad_search_radius_sec
    );

    if profile.method == TtsMethod::GeminiLive && profile.gemini_parallel_requests > 1 {
        run_gemini_subtitle_narration_parallel(
            job_id,
            total,
            &groups,
            &profile,
            &request.grouping,
            snapshot.clone(),
            cancelled.clone(),
        );
        return;
    }

    for (group_index, group) in groups.iter().enumerate() {
        if cancelled.load(Ordering::SeqCst) {
            eprintln!(
                "[Narration][job={}] cancelled at group {}/{}",
                job_id,
                group_index + 1,
                groups.len()
            );
            let _ = update_snapshot(&snapshot, |state| {
                state.state = "cancelled".to_string();
                state.message = "Subtitle narration cancelled".to_string();
                state.active_subtitle_id = None;
            });
            return;
        }

        let Some(first_item) = group.items.first() else {
            continue;
        };
        let _ = update_snapshot(&snapshot, |state| {
            state.active_subtitle_id = Some(first_item.id.clone());
            state.message = format!(
                "Generating narration {}/{}",
                (state.completed_items + 1).min(total),
                total
            );
        });
        eprintln!(
            "[Narration][job={}] group {}/{} group_id={} items={} first_subtitle_id={} method={:?} lang_override='{}' text_chars={} text_json={}",
            job_id,
            group_index + 1,
            groups.len(),
            group.id,
            group.items.len(),
            first_item.id,
            profile.method,
            profile.language_code_override.as_deref().unwrap_or(""),
            group.text.chars().count(),
            serde_json::to_string(&group.text)
                .unwrap_or_else(|_| "\"<unserializable>\"".to_string())
        );

        let result = synthesize_narration_item_with_retries(NarrationSynthesisAttempt {
            job_id,
            index: group_index,
            total: groups.len(),
            item_id: &first_item.id,
            clean_text: &group.text,
            profile: &profile,
            snapshot: &snapshot,
            cancelled: &cancelled,
        });

        match &result {
            Ok((path, audio, attempts)) => eprintln!(
                "[Narration][job={}] group {}/{} group_id={} items={} OK duration_sec={:.3} attempts={} path={}",
                job_id,
                group_index + 1,
                groups.len(),
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
                groups.len(),
                group.id,
                first_item.id,
                NARRATION_TTS_MAX_ATTEMPTS,
                message
            ),
        }

        let _ = update_snapshot(&snapshot, |state| {
            state.completed_items += group.items.len();
            state.progress = state.completed_items as f64 / total.max(1) as f64;
            match result {
                Ok((path, audio, _attempts)) => {
                    let duration = audio.duration_ms as f64 / 1000.0;
                    let split = align_group_audio_ranges(
                        group,
                        &path,
                        &audio,
                        profile.language_code_override.as_deref(),
                        request.grouping.vad_search_radius_sec,
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

    let _ = update_snapshot(&snapshot, |state| {
        state.active_subtitle_id = None;
        state.progress = 1.0;
        if state.results.is_empty() && !state.errors.is_empty() {
            state.state = "error".to_string();
            // Include the first underlying error in the human-readable message
            // so the side panel shows it instead of a generic "failed" string.
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
    if let Ok(state) = snapshot.lock() {
        eprintln!(
            "[Narration][job={}] done state={} results={} errors={} message=\"{}\"",
            job_id,
            state.state,
            state.results.len(),
            state.errors.len(),
            state.message
        );
        for (i, error) in state.errors.iter().enumerate() {
            eprintln!(
                "[Narration][job={}] error[{}] subtitle_id={} message={}",
                job_id, i, error.subtitle_id, error.message
            );
        }
    }
}
