use super::*;

pub fn default_batch_settings_for_target(
    target_language: &str,
    model: &str,
    voice: &str,
    speed: &str,
) -> Result<S2sBatchSettings> {
    let app = APP.lock().unwrap();
    let target_language = if target_language.trim().is_empty() {
        app.config.realtime_target_language.clone()
    } else {
        target_language.trim().to_string()
    };
    let custom_instruction =
        tts_instruction_for_target(&target_language, &app.config.tts_language_conditions);
    let model = if model.trim().is_empty() {
        app.config.tts_gemini_live_model.trim().to_string()
    } else {
        model.trim().to_string()
    };
    let voice = if voice.trim().is_empty() {
        app.config.tts_voice.trim().to_string()
    } else {
        voice.trim().to_string()
    };
    let speed = if speed.trim().is_empty() {
        app.config.tts_speed.trim().to_string()
    } else {
        speed.trim().to_string()
    };
    Ok(S2sBatchSettings {
        model,
        voice,
        speed,
        target_language,
        custom_instruction,
        parallel_requests: 3,
        vad_group_budget: 25,
    })
}

pub fn run_gemini_live_s2s_batch(
    samples_16k_mono: Vec<i16>,
    batch_settings: S2sBatchSettings,
    stop_signal: Arc<AtomicBool>,
) -> Result<Vec<S2sBatchSegment>> {
    run_gemini_live_s2s_batch_with_progress(samples_16k_mono, batch_settings, stop_signal, None)
}

pub fn run_gemini_live_s2s_batch_with_progress(
    samples_16k_mono: Vec<i16>,
    batch_settings: S2sBatchSettings,
    stop_signal: Arc<AtomicBool>,
    progress: Option<&mut dyn FnMut(usize, usize)>,
) -> Result<Vec<S2sBatchSegment>> {
    run_gemini_live_s2s_batch_with_callbacks(
        samples_16k_mono,
        batch_settings,
        stop_signal,
        progress,
        None,
    )
}

pub fn run_gemini_live_s2s_batch_with_callbacks(
    samples_16k_mono: Vec<i16>,
    batch_settings: S2sBatchSettings,
    stop_signal: Arc<AtomicBool>,
    mut progress: Option<&mut dyn FnMut(usize, usize)>,
    mut on_segment: Option<&mut dyn FnMut(S2sBatchSegment) -> Result<()>>,
) -> Result<Vec<S2sBatchSegment>> {
    let api_key = {
        let app = APP.lock().unwrap();
        app.config.gemini_api_key.trim().to_string()
    };
    if api_key.is_empty() {
        return Err(anyhow::anyhow!("NO_API_KEY:google"));
    }
    let settings = S2sSettings {
        api_key,
        model: if batch_settings.model.trim().is_empty() {
            crate::model_config::GEMINI_LIVE_API_MODEL_3_1.to_string()
        } else {
            crate::model_config::normalize_tts_gemini_model(&batch_settings.model).to_string()
        },
        mode: S2sMode::LegacyInterpreter,
        voice: if batch_settings.voice.trim().is_empty() {
            "Aoede".to_string()
        } else {
            batch_settings.voice.trim().to_string()
        },
        speed: if batch_settings.speed.trim().is_empty() {
            "Normal".to_string()
        } else {
            batch_settings.speed.trim().to_string()
        },
        custom_instruction: batch_settings.custom_instruction,
        target_language: batch_settings.target_language,
    };
    let parallel_requests = batch_settings.parallel_requests.clamp(1, 6);
    let context_memory = Arc::new(Mutex::new(S2sContextMemory::default()));
    let timed_segments = group_timed_segments(
        collect_vad_segments(samples_16k_mono.clone()),
        &samples_16k_mono,
        batch_settings.vad_group_budget,
    );
    let total_segments = timed_segments.len();
    crate::log_info!(
        "[GeminiS2S][Batch] vad_groups={} group_budget={} parallel={}",
        total_segments,
        batch_settings.vad_group_budget,
        parallel_requests
    );
    if let Some(callback) = progress.as_mut() {
        callback(0, total_segments);
    }
    if parallel_requests > 1 {
        return run_gemini_live_s2s_segments_parallel(
            timed_segments,
            total_segments,
            settings,
            stop_signal,
            progress,
            on_segment,
            parallel_requests,
        );
    }
    let mut results = Vec::with_capacity(timed_segments.len());
    for (index, timed) in timed_segments.into_iter().enumerate() {
        if stop_signal.load(Ordering::SeqCst) {
            break;
        }
        if let Some(callback) = progress.as_mut() {
            callback(index + 1, total_segments);
        }
        let id = timed.segment.id;
        let (event_tx, event_rx) = mpsc::channel::<S2sEvent>();
        let adaptive_vad = Arc::new(Mutex::new(AdaptiveS2sVadState::default()));
        let resources = S2sSessionResources {
            event_tx: event_tx.clone(),
            stop_signal: stop_signal.clone(),
            settings: settings.clone(),
            context_memory: context_memory.clone(),
            adaptive_vad,
        };
        let result = run_single_segment_session(0, id + 1, timed.segment.clone(), &resources);
        drop(resources);
        drop(event_tx);
        let mut source_text = String::new();
        let mut target_text = String::new();
        let mut audio_bytes = Vec::new();
        let mut error: Option<String> = None;
        for event in event_rx.try_iter() {
            match event {
                S2sEvent::InputText { text, .. } => {
                    source_text = text;
                }
                S2sEvent::OutputText { text, .. } => {
                    merge_segment_text(&mut target_text, &text);
                }
                S2sEvent::Audio { bytes, .. } => audio_bytes.extend(bytes),
                S2sEvent::Error { message, .. } => error = Some(message),
                _ => {}
            }
        }
        result?;
        if let Some(error) = error {
            return Err(anyhow::anyhow!(error));
        }
        if source_text.trim().is_empty() && target_text.trim().is_empty() && audio_bytes.is_empty()
        {
            continue;
        }
        if let Ok(mut memory) = context_memory.lock() {
            memory.push_completed(id, &source_text, &target_text);
        }
        let batch_segment = S2sBatchSegment {
            id,
            source_start_sec: timed.start_sample as f64 / 16_000.0,
            source_end_sec: timed.end_sample as f64 / 16_000.0,
            source_text,
            target_text,
            audio_pcm_24k: pcm_bytes_to_i16(&audio_bytes),
        };
        if let Some(callback) = on_segment.as_mut() {
            callback(batch_segment.clone())?;
        }
        results.push(batch_segment);
    }
    Ok(results)
}

struct S2sParallelSegmentResult {
    segment: Option<S2sBatchSegment>,
    error: Option<String>,
}

fn run_gemini_live_s2s_segments_parallel(
    timed_segments: Vec<TimedSegment>,
    total_segments: usize,
    settings: S2sSettings,
    stop_signal: Arc<AtomicBool>,
    mut progress: Option<&mut dyn FnMut(usize, usize)>,
    mut on_segment: Option<&mut dyn FnMut(S2sBatchSegment) -> Result<()>>,
    parallel_requests: usize,
) -> Result<Vec<S2sBatchSegment>> {
    let (tx, rx) = mpsc::channel::<S2sParallelSegmentResult>();
    let mut next_index = 0usize;
    let mut active = 0usize;
    let mut completed = 0usize;
    let mut results = Vec::with_capacity(total_segments);
    let mut first_error: Option<String> = None;

    while (next_index < timed_segments.len() || active > 0) && !stop_signal.load(Ordering::SeqCst) {
        while active < parallel_requests
            && next_index < timed_segments.len()
            && !stop_signal.load(Ordering::SeqCst)
        {
            let index = next_index;
            next_index += 1;
            active += 1;
            let timed = timed_segments[index].clone();
            let settings = settings.clone();
            let stop_signal = stop_signal.clone();
            let tx = tx.clone();
            std::thread::spawn(move || {
                let response = run_s2s_timed_segment_without_context(timed, settings, stop_signal);
                let _ = tx.send(response);
            });
        }

        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(response) => {
                active = active.saturating_sub(1);
                completed += 1;
                if let Some(callback) = progress.as_mut() {
                    callback(completed, total_segments);
                }
                if let Some(error) = response.error {
                    first_error.get_or_insert(error);
                    continue;
                }
                if let Some(segment) = response.segment {
                    if let Some(callback) = on_segment.as_mut() {
                        callback(segment.clone())?;
                    }
                    results.push(segment);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    results.sort_by_key(|segment| segment.id);
    if let Some(error) = first_error {
        return Err(anyhow::anyhow!(error));
    }
    Ok(results)
}

fn run_s2s_timed_segment_without_context(
    timed: TimedSegment,
    settings: S2sSettings,
    stop_signal: Arc<AtomicBool>,
) -> S2sParallelSegmentResult {
    let id = timed.segment.id;
    for attempt in 0..S2S_BATCH_SEGMENT_ATTEMPTS {
        if stop_signal.load(Ordering::SeqCst) {
            break;
        }
        let (event_tx, event_rx) = mpsc::channel::<S2sEvent>();
        let context_memory = Arc::new(Mutex::new(S2sContextMemory::default()));
        let adaptive_vad = Arc::new(Mutex::new(AdaptiveS2sVadState::default()));
        let resources = S2sSessionResources {
            event_tx: event_tx.clone(),
            stop_signal: stop_signal.clone(),
            settings: settings.clone(),
            context_memory,
            adaptive_vad,
        };
        let result = run_single_segment_session(
            0,
            id + 1 + (attempt as u64 * 10_000_000),
            timed.segment.clone(),
            &resources,
        );
        drop(resources);
        drop(event_tx);
        let mut source_text = String::new();
        let mut target_text = String::new();
        let mut audio_bytes = Vec::new();
        let mut error: Option<String> = None;
        for event in event_rx.try_iter() {
            match event {
                S2sEvent::InputText { text, .. } => {
                    source_text = text;
                }
                S2sEvent::OutputText { text, .. } => {
                    merge_segment_text(&mut target_text, &text);
                }
                S2sEvent::Audio { bytes, .. } => audio_bytes.extend(bytes),
                S2sEvent::Error { message, .. } => error = Some(message),
                _ => {}
            }
        }
        if let Err(err) = result {
            eprintln!(
                "[RealtimeS2S] batch-retry segment={} attempt={}/{} reason=session_error error={}",
                id,
                attempt + 1,
                S2S_BATCH_SEGMENT_ATTEMPTS,
                err
            );
            continue;
        }
        if let Some(error) = error {
            eprintln!(
                "[RealtimeS2S] batch-retry segment={} attempt={}/{} reason=event_error error={}",
                id,
                attempt + 1,
                S2S_BATCH_SEGMENT_ATTEMPTS,
                error
            );
            continue;
        }
        if audio_bytes.is_empty() {
            eprintln!(
                "[RealtimeS2S] batch-retry segment={} attempt={}/{} reason=empty_audio source_text_chars={} target_text_chars={}",
                id,
                attempt + 1,
                S2S_BATCH_SEGMENT_ATTEMPTS,
                source_text.chars().count(),
                target_text.chars().count()
            );
            continue;
        }
        return S2sParallelSegmentResult {
            segment: Some(S2sBatchSegment {
                id,
                source_start_sec: timed.start_sample as f64 / 16_000.0,
                source_end_sec: timed.end_sample as f64 / 16_000.0,
                source_text,
                target_text,
                audio_pcm_24k: pcm_bytes_to_i16(&audio_bytes),
            }),
            error: None,
        };
    }
    S2sParallelSegmentResult {
        segment: None,
        error: Some(format!(
            "S2S segment {id} produced no audio after {S2S_BATCH_SEGMENT_ATTEMPTS} attempts"
        )),
    }
}
