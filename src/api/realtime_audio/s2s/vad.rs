use super::*;

pub(super) fn run_vad_loop(
    audio_buffer: Arc<Mutex<Vec<i16>>>,
    stop_signal: Arc<AtomicBool>,
    segment_senders: Vec<mpsc::Sender<Segment>>,
    event_tx: mpsc::Sender<S2sEvent>,
    overlay_hwnd: HWND,
    session_id: u64,
    adaptive_vad: Arc<Mutex<AdaptiveS2sVadState>>,
) {
    let mut pending = Vec::<i16>::new();
    let mut preroll = VecDeque::<i16>::new();
    let mut active = Vec::<i16>::new();
    let mut active_speech_frames = 0usize;
    let mut active_peak_rms = 0.0f32;
    let mut segment_id = 0u64;
    let mut silence_frames = 0usize;
    let mut noise_floor = 0.004f32;

    while !stop_signal.load(Ordering::Relaxed) {
        let stale_session = is_stale_session(session_id);
        let audio_changed = AUDIO_SOURCE_CHANGE.load(Ordering::SeqCst);
        let model_changed = TRANSCRIPTION_MODEL_CHANGE.load(Ordering::SeqCst);
        let language_changed = LANGUAGE_CHANGE.load(Ordering::SeqCst);
        if stale_session || audio_changed || model_changed || language_changed {
            break;
        }

        if !overlay_hwnd.is_invalid() {
            unsafe {
                if !windows::Win32::UI::WindowsAndMessaging::IsWindow(Some(overlay_hwnd)).as_bool()
                {
                    stop_signal.store(true, Ordering::SeqCst);
                    break;
                }
            }
        }

        {
            let mut guard = audio_buffer.lock().unwrap();
            if !guard.is_empty() {
                pending.extend(guard.drain(..));
            }
        }

        while pending.len() >= FRAME_SAMPLES {
            let frame: Vec<i16> = pending.drain(..FRAME_SAMPLES).collect();
            let rms = calculate_rms(&frame);
            REALTIME_RMS.store(rms.to_bits(), Ordering::Relaxed);
            if !overlay_hwnd.is_invalid() {
                unsafe {
                    let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                        Some(overlay_hwnd),
                        WM_VOLUME_UPDATE,
                        windows::Win32::Foundation::WPARAM(0),
                        windows::Win32::Foundation::LPARAM(0),
                    );
                }
            }

            let speech_threshold = speech_threshold_for_noise(noise_floor);
            let is_speech = rms >= speech_threshold || rms >= ABSOLUTE_SPEECH_RMS;
            noise_floor = update_noise_floor(noise_floor, rms, speech_threshold, is_speech);

            if active.is_empty() {
                preroll.extend(frame.iter().copied());
                while preroll.len() > PREROLL_SAMPLES {
                    preroll.pop_front();
                }
                if is_speech {
                    active.extend(preroll.drain(..));
                    active.extend(frame);
                    active_speech_frames = 1;
                    active_peak_rms = rms;
                    silence_frames = 0;
                }
                continue;
            }

            active.extend(frame);
            active_peak_rms = active_peak_rms.max(rms);
            if is_speech {
                active_speech_frames += 1;
            }
            silence_frames = if is_speech { 0 } else { silence_frames + 1 };

            let long_enough = active.len() >= MIN_SEGMENT_SAMPLES;
            let target_hit = active.len() >= TARGET_SEGMENT_SAMPLES;
            let max_hit = active.len() >= MAX_SEGMENT_SAMPLES;
            let silence_hit = target_hit && silence_frames >= END_SILENCE_FRAMES;
            if long_enough && (silence_hit || max_hit) {
                let queued_at = Instant::now();
                let samples = std::mem::take(&mut active);
                let speech_frames = active_speech_frames;
                let peak_rms = active_peak_rms;
                active_speech_frames = 0;
                active_peak_rms = 0.0;
                let segment = Segment::new(segment_id, samples, speech_frames, peak_rms);
                let worker = (segment.id as usize) % segment_senders.len();
                let audio_ms = samples_to_ms(segment.samples.len());
                let vad_snapshot = adaptive_vad_snapshot(&adaptive_vad);
                if !is_segment_worth_sending(&segment, vad_snapshot) {
                    log_adaptive_vad_skip(&segment, vad_snapshot);
                    silence_frames = 0;
                    preroll.clear();
                    continue;
                }
                let _ = event_tx.send(S2sEvent::Queued {
                    id: segment.id,
                    audio_ms,
                    queued_at,
                });
                eprintln!(
                    "[RealtimeS2S][Segment] queued id={} worker={} audio_ms={} samples={} speech_frames={} speech_ratio={:.2} speech_like_ratio={:.2} confidence={:.2} strictness={:.2} mean_rms={:.4} peak_rms={:.4} peak_sample={:.4} backlog_ms={}",
                    segment.id,
                    worker,
                    audio_ms,
                    segment.samples.len(),
                    segment.speech_frames,
                    segment_speech_ratio(&segment),
                    segment_speech_like_ratio(&segment),
                    segment_speech_confidence(&segment),
                    vad_snapshot.strictness,
                    segment.mean_rms,
                    segment.peak_rms,
                    segment_peak_sample(&segment),
                    s2s_backlog_ms()
                );
                if segment_senders[worker].send(segment).is_err() {
                    stop_signal.store(true, Ordering::SeqCst);
                    break;
                }
                crate::overlay::realtime_webview::state::REALTIME_S2S_AUDIO_BACKLOG
                    .fetch_add(1, Ordering::Relaxed);
                crate::overlay::realtime_webview::state::REALTIME_S2S_AUDIO_BACKLOG_MS
                    .fetch_add(audio_ms as u32, Ordering::Relaxed);
                segment_id += 1;
                silence_frames = 0;
                preroll.clear();
            }
        }

        std::thread::sleep(Duration::from_millis(20));
    }

    if active.len() >= MIN_SEGMENT_SAMPLES {
        let worker = (segment_id as usize) % segment_senders.len();
        let audio_ms = samples_to_ms(active.len());
        let queued_at = Instant::now();
        let segment = Segment::new(segment_id, active, active_speech_frames, active_peak_rms);
        let vad_snapshot = adaptive_vad_snapshot(&adaptive_vad);
        if !is_segment_worth_sending(&segment, vad_snapshot) {
            log_adaptive_vad_skip(&segment, vad_snapshot);
            return;
        }
        let _ = event_tx.send(S2sEvent::Queued {
            id: segment_id,
            audio_ms,
            queued_at,
        });
        eprintln!(
            "[RealtimeS2S][Segment] queued id={} worker={} audio_ms={} samples={} speech_frames={} speech_ratio={:.2} speech_like_ratio={:.2} confidence={:.2} strictness={:.2} mean_rms={:.4} peak_rms={:.4} peak_sample={:.4} backlog_ms={}",
            segment_id,
            worker,
            audio_ms,
            segment.samples.len(),
            segment.speech_frames,
            segment_speech_ratio(&segment),
            segment_speech_like_ratio(&segment),
            segment_speech_confidence(&segment),
            vad_snapshot.strictness,
            segment.mean_rms,
            segment.peak_rms,
            segment_peak_sample(&segment),
            s2s_backlog_ms()
        );
        let _ = segment_senders[worker].send(segment);
        crate::overlay::realtime_webview::state::REALTIME_S2S_AUDIO_BACKLOG
            .fetch_add(1, Ordering::Relaxed);
        crate::overlay::realtime_webview::state::REALTIME_S2S_AUDIO_BACKLOG_MS
            .fetch_add(audio_ms as u32, Ordering::Relaxed);
    }
}

pub(super) fn collect_vad_segments(samples: Vec<i16>) -> Vec<TimedSegment> {
    let mut pending = samples;
    let mut cursor_sample = 0usize;
    let mut preroll = VecDeque::<(usize, i16)>::new();
    let mut active = Vec::<i16>::new();
    let mut active_start_sample = 0usize;
    let mut active_speech_frames = 0usize;
    let mut active_peak_rms = 0.0f32;
    let mut segment_id = 0u64;
    let mut silence_frames = 0usize;
    let mut noise_floor = 0.004f32;
    let mut output = Vec::new();

    while pending.len() >= FRAME_SAMPLES {
        let frame_start = cursor_sample;
        let frame: Vec<i16> = pending.drain(..FRAME_SAMPLES).collect();
        cursor_sample += frame.len();
        let rms = calculate_rms(&frame);
        let speech_threshold = speech_threshold_for_noise(noise_floor);
        let is_speech = rms >= speech_threshold || rms >= ABSOLUTE_SPEECH_RMS;
        noise_floor = update_noise_floor(noise_floor, rms, speech_threshold, is_speech);

        if active.is_empty() {
            preroll.extend(
                frame
                    .iter()
                    .copied()
                    .enumerate()
                    .map(|(idx, sample)| (frame_start + idx, sample)),
            );
            while preroll.len() > PREROLL_SAMPLES {
                preroll.pop_front();
            }
            if is_speech {
                active_start_sample = preroll.front().map(|(idx, _)| *idx).unwrap_or(frame_start);
                active.extend(preroll.drain(..).map(|(_, sample)| sample));
                active_speech_frames = 1;
                active_peak_rms = rms;
                silence_frames = 0;
            }
            continue;
        }

        active.extend(frame);
        active_peak_rms = active_peak_rms.max(rms);
        if is_speech {
            active_speech_frames += 1;
        }
        silence_frames = if is_speech { 0 } else { silence_frames + 1 };
        let long_enough = active.len() >= MIN_SEGMENT_SAMPLES;
        let target_hit = active.len() >= TARGET_SEGMENT_SAMPLES;
        let max_hit = active.len() >= MAX_SEGMENT_SAMPLES;
        let silence_hit = target_hit && silence_frames >= END_SILENCE_FRAMES;
        if long_enough && (silence_hit || max_hit) {
            push_timed_segment(
                &mut output,
                &mut segment_id,
                std::mem::take(&mut active),
                active_start_sample,
                active_speech_frames,
                active_peak_rms,
            );
            active_speech_frames = 0;
            active_peak_rms = 0.0;
            silence_frames = 0;
            preroll.clear();
        }
    }

    if !pending.is_empty() {
        if active.is_empty() {
            active_start_sample = cursor_sample;
        }
        active.extend(pending);
    }
    if active.len() >= MIN_SEGMENT_SAMPLES {
        push_timed_segment(
            &mut output,
            &mut segment_id,
            active,
            active_start_sample,
            active_speech_frames,
            active_peak_rms,
        );
    }
    output
}

pub(super) fn group_timed_segments(
    timed_segments: Vec<TimedSegment>,
    source_samples: &[i16],
    group_budget: usize,
) -> Vec<TimedSegment> {
    if timed_segments.len() <= 1 {
        return timed_segments;
    }
    let max_group_sec = (group_budget.clamp(5, 120) as f64 * 0.25).clamp(2.5, 12.0);
    let max_gap_sec = 1.2;
    let max_group_items = 10usize;
    let mut output = Vec::new();
    let mut group_start_index = 0usize;
    while group_start_index < timed_segments.len() {
        let first = &timed_segments[group_start_index];
        let mut group_end_index = group_start_index;
        let mut group_items = 1usize;
        while group_end_index + 1 < timed_segments.len() {
            let current = &timed_segments[group_end_index];
            let next = &timed_segments[group_end_index + 1];
            let next_total_sec =
                (next.end_sample.saturating_sub(first.start_sample)) as f64 / 16_000.0;
            let gap_sec = (next.start_sample.saturating_sub(current.end_sample)) as f64 / 16_000.0;
            if group_items >= max_group_items
                || gap_sec > max_gap_sec
                || next_total_sec > max_group_sec
            {
                break;
            }
            group_end_index += 1;
            group_items += 1;
        }
        let last = &timed_segments[group_end_index];
        let start_sample = first.start_sample.min(source_samples.len());
        let end_sample = last.end_sample.min(source_samples.len()).max(start_sample);
        let samples = source_samples[start_sample..end_sample].to_vec();
        let speech_frames = timed_segments[group_start_index..=group_end_index]
            .iter()
            .map(|timed| timed.segment.speech_frames)
            .sum();
        let peak_rms = timed_segments[group_start_index..=group_end_index]
            .iter()
            .map(|timed| timed.segment.peak_rms)
            .fold(0.0f32, f32::max);
        output.push(TimedSegment {
            segment: Segment::new(first.segment.id, samples, speech_frames, peak_rms),
            start_sample,
            end_sample,
        });
        group_start_index = group_end_index + 1;
    }
    crate::log_info!(
        "[GeminiS2S][VADGroup] input_segments={} grouped_segments={} budget={} max_group_sec={:.2}",
        timed_segments.len(),
        output.len(),
        group_budget,
        max_group_sec
    );
    output
}

fn push_timed_segment(
    output: &mut Vec<TimedSegment>,
    segment_id: &mut u64,
    samples: Vec<i16>,
    start_sample: usize,
    speech_frames: usize,
    peak_rms: f32,
) {
    let segment = Segment::new(*segment_id, samples, speech_frames, peak_rms);
    *segment_id += 1;
    if !is_segment_worth_sending(&segment, AdaptiveS2sVadSnapshot::default()) {
        return;
    }
    let end_sample = start_sample + segment.samples.len();
    output.push(TimedSegment {
        segment,
        start_sample,
        end_sample,
    });
}
