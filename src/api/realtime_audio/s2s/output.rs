use super::*;

struct SegmentPlayback {
    chunks: VecDeque<Vec<u8>>,
    chunk_count: usize,
    byte_count: usize,
    source_audio_ms: usize,
    queued_at: Option<Instant>,
    has_input_text: bool,
    has_output_text: bool,
    done: bool,
    error: bool,
}

impl SegmentPlayback {
    fn new() -> Self {
        Self {
            chunks: VecDeque::new(),
            chunk_count: 0,
            byte_count: 0,
            source_audio_ms: 0,
            queued_at: None,
            has_input_text: false,
            has_output_text: false,
            done: false,
            error: false,
        }
    }
}

pub(super) fn coordinate_output(
    event_rx: mpsc::Receiver<S2sEvent>,
    stop_signal: Arc<AtomicBool>,
    overlay_hwnd: HWND,
    translation_hwnd: Option<HWND>,
    state: SharedRealtimeState,
    context_memory: Arc<Mutex<S2sContextMemory>>,
    mode: S2sMode,
) {
    let mut next_play_id = 0u64;
    let mut segments = BTreeMap::<u64, SegmentPlayback>::new();
    let mut inputs = BTreeMap::<u64, String>::new();
    let mut outputs = BTreeMap::<u64, String>::new();
    let mut playback: Option<RealtimePlaybackBridge> = None;

    while !stop_signal.load(Ordering::SeqCst) {
        match event_rx.recv_timeout(Duration::from_millis(30)) {
            Ok(event) => match event {
                S2sEvent::Queued {
                    id,
                    audio_ms,
                    queued_at,
                } => {
                    if id < next_play_id {
                        continue;
                    }
                    let segment = segments.entry(id).or_insert_with(SegmentPlayback::new);
                    segment.source_audio_ms = audio_ms;
                    segment.queued_at = Some(queued_at);
                }
                S2sEvent::InputText { id, text } => {
                    if id < next_play_id {
                        continue;
                    }
                    segments
                        .entry(id)
                        .or_insert_with(SegmentPlayback::new)
                        .has_input_text = true;
                    inputs.insert(id, text);
                    publish_text(&state, overlay_hwnd, translation_hwnd, &inputs, &outputs);
                }
                S2sEvent::OutputText { id, text } => {
                    if id < next_play_id {
                        continue;
                    }
                    segments
                        .entry(id)
                        .or_insert_with(SegmentPlayback::new)
                        .has_output_text = true;
                    merge_segment_text(outputs.entry(id).or_default(), &text);
                    publish_text(&state, overlay_hwnd, translation_hwnd, &inputs, &outputs);
                }
                S2sEvent::Audio { id, bytes } => {
                    if id < next_play_id {
                        continue;
                    }
                    let segment = segments.entry(id).or_insert_with(SegmentPlayback::new);
                    segment.chunk_count += 1;
                    segment.byte_count += bytes.len();
                    segment.chunks.push_back(bytes);
                }
                S2sEvent::Done { id } => {
                    if id < next_play_id {
                        continue;
                    }
                    segments.entry(id).or_insert_with(SegmentPlayback::new).done = true;
                    push_completed_context(id, &inputs, &outputs, &context_memory);
                }
                S2sEvent::Error { id, message } => {
                    if id < next_play_id {
                        continue;
                    }
                    eprintln!("[{}] segment={id} error: {message}", mode.log_tag());
                    let segment = segments.entry(id).or_insert_with(SegmentPlayback::new);
                    segment.error = true;
                    segment.done = true;
                }
                S2sEvent::LiveText {
                    source_full,
                    source_committed_len,
                    target_committed,
                    target_draft,
                } => {
                    publish_live_translate_text(
                        &state,
                        overlay_hwnd,
                        translation_hwnd,
                        source_full,
                        source_committed_len,
                        target_committed,
                        target_draft,
                    );
                }
                S2sEvent::Interrupt => {
                    TTS_MANAGER.stop();
                    playback = None;
                }
            },
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }

        drain_ordered_audio(
            &mut segments,
            &mut next_play_id,
            &mut playback,
            translation_hwnd.unwrap_or(overlay_hwnd),
            mode,
        );
        update_s2s_ready_backlog(&segments, next_play_id);
    }

    if let Some(current) = playback.take() {
        current.end();
    }
}

fn drain_ordered_audio(
    segments: &mut BTreeMap<u64, SegmentPlayback>,
    next_play_id: &mut u64,
    playback: &mut Option<RealtimePlaybackBridge>,
    hwnd: HWND,
    mode: S2sMode,
) {
    loop {
        let Some(segment) = segments.get_mut(next_play_id) else {
            return;
        };
        crate::overlay::realtime_webview::state::REALTIME_S2S_AUDIO_DELAY_MS
            .store(segment_delay_ms(segment) as u32, Ordering::Relaxed);
        if playback.is_none()
            && segment.chunks.is_empty()
            && !segment.done
            && !segment.error
            && mode != S2sMode::LiveTranslate
            && should_skip_stale_pending_segment(segment)
        {
            eprintln!(
                "[{}] skip-play segment={} reason=pending-timeout delay_ms={} source_audio_ms={} input_text={} output_text={} backlog_ms={}",
                mode.log_tag(),
                next_play_id,
                segment_delay_ms(segment),
                segment.source_audio_ms,
                segment.has_input_text,
                segment.has_output_text,
                s2s_backlog_ms()
            );
            let source_audio_ms = segment.source_audio_ms;
            segments.remove(next_play_id);
            decrement_s2s_backlog(source_audio_ms);
            *next_play_id += 1;
            continue;
        }
        if playback.is_none() && segment.chunks.is_empty() && (segment.done || segment.error) {
            let reason = if segment.error { "error" } else { "empty" };
            eprintln!(
                "[{}] skip-play segment={} reason={reason} delay_ms={} backlog_ms={}",
                mode.log_tag(),
                next_play_id,
                segment_delay_ms(segment),
                s2s_backlog_ms()
            );
            let source_audio_ms = segment.source_audio_ms;
            segments.remove(next_play_id);
            decrement_s2s_backlog(source_audio_ms);
            *next_play_id += 1;
            continue;
        }
        if playback.is_none() && !segment.chunks.is_empty() {
            let delay_ms = segment_delay_ms(segment);
            crate::overlay::realtime_webview::state::REALTIME_S2S_AUDIO_DELAY_MS
                .store(delay_ms as u32, Ordering::Relaxed);
            *playback = Some(RealtimePlaybackBridge::new(hwnd.0 as isize));
        }
        if let Some(player) = playback.as_ref() {
            while let Some(bytes) = segment.chunks.pop_front() {
                player.push(bytes);
            }
        }
        if segment.done && segment.chunks.is_empty() {
            let source_audio_ms = segment.source_audio_ms;
            let next_id = *next_play_id + 1;
            let _ = segment;
            let next_ready = segments
                .get(&next_id)
                .is_some_and(|next| !next.chunks.is_empty());
            segments.remove(next_play_id);
            decrement_s2s_backlog(source_audio_ms);
            *next_play_id += 1;
            if !next_ready && let Some(player) = playback.take() {
                player.end();
            }
            continue;
        }
        return;
    }
}

fn publish_text(
    state: &SharedRealtimeState,
    overlay_hwnd: HWND,
    translation_hwnd: Option<HWND>,
    inputs: &BTreeMap<u64, String>,
    outputs: &BTreeMap<u64, String>,
) {
    let source_display = split_s2s_visuals(inputs);
    let target_display = split_s2s_visuals(outputs);
    let (source_display, target_display) = if let Ok(mut s) = state.lock() {
        s.full_transcript = source_display.full.clone();
        s.transcript_committed_pos = source_display.committed_len;
        s.last_committed_pos = s.transcript_committed_pos;
        s.uncommitted_source_start = source_display.committed_len;
        s.uncommitted_source_end = source_display.full.len();
        s.display_transcript = if s.frozen_prefix.is_empty() {
            source_display.full.clone()
        } else if source_display.full.is_empty() {
            s.frozen_prefix.clone()
        } else {
            format!("{}\n\n{}", s.frozen_prefix, source_display.full)
        };
        s.committed_translation = target_display.committed;
        s.uncommitted_translation = target_display.draft;
        s.display_translation = target_display.full;
        (s.display_transcript.clone(), s.display_translation.clone())
    } else {
        (String::new(), String::new())
    };
    update_overlay_text(overlay_hwnd, &source_display);
    if let Some(hwnd) = translation_hwnd {
        update_translation_text(hwnd, &target_display);
    }
}

fn publish_live_translate_text(
    state: &SharedRealtimeState,
    overlay_hwnd: HWND,
    translation_hwnd: Option<HWND>,
    source_full: String,
    source_committed_len: usize,
    target_committed: String,
    target_draft: String,
) {
    let source_committed_len = source_committed_len.min(source_full.len());
    let source_committed_len = clamp_to_char_boundary(&source_full, source_committed_len);
    let target_full = if target_committed.is_empty() {
        target_draft.clone()
    } else if target_draft.is_empty() {
        target_committed.clone()
    } else {
        format!("{target_committed} {target_draft}")
    };
    let source_display = if let Ok(mut s) = state.lock() {
        s.full_transcript = source_full.clone();
        s.transcript_committed_pos = source_committed_len;
        s.last_committed_pos = source_committed_len;
        s.uncommitted_source_start = source_committed_len;
        s.uncommitted_source_end = source_full.len();
        s.display_transcript = if s.frozen_prefix.is_empty() {
            source_full.clone()
        } else if source_full.is_empty() {
            s.frozen_prefix.clone()
        } else {
            format!("{}\n\n{}", s.frozen_prefix, source_full)
        };
        s.committed_translation = target_committed.clone();
        s.uncommitted_translation = target_draft.clone();
        s.display_translation = target_full.clone();
        s.display_transcript.clone()
    } else {
        String::new()
    };
    update_overlay_text(overlay_hwnd, &source_display);
    if let Some(hwnd) = translation_hwnd {
        update_translation_text(hwnd, &target_full);
    }
}

fn clamp_to_char_boundary(value: &str, mut index: usize) -> usize {
    index = index.min(value.len());
    while index > 0 && !value.is_char_boundary(index) {
        index -= 1;
    }
    index
}

struct S2sVisualText {
    committed: String,
    draft: String,
    full: String,
    committed_len: usize,
}

fn split_s2s_visuals(items: &BTreeMap<u64, String>) -> S2sVisualText {
    let mut segments = items
        .iter()
        .filter_map(|(_, value)| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        })
        .collect::<Vec<_>>();

    let draft = segments.pop().unwrap_or_default().to_string();
    let committed = segments.join(" ");
    let full = if committed.is_empty() {
        draft.clone()
    } else if draft.is_empty() {
        committed.clone()
    } else {
        format!("{committed} {draft}")
    };
    let committed_len = if committed.is_empty() {
        0
    } else {
        committed.len()
    };

    S2sVisualText {
        committed,
        draft,
        full,
        committed_len,
    }
}

fn push_completed_context(
    id: u64,
    inputs: &BTreeMap<u64, String>,
    outputs: &BTreeMap<u64, String>,
    context_memory: &Arc<Mutex<S2sContextMemory>>,
) {
    let source = inputs.get(&id).map(String::as_str).unwrap_or_default();
    let target = outputs.get(&id).map(String::as_str).unwrap_or_default();
    if let Ok(mut memory) = context_memory.lock() {
        memory.push_completed(id, source, target);
    }
}

fn segment_delay_ms(segment: &SegmentPlayback) -> u128 {
    segment
        .queued_at
        .map(|queued_at| queued_at.elapsed().as_millis())
        .unwrap_or_default()
}

fn should_skip_stale_pending_segment(segment: &SegmentPlayback) -> bool {
    let delay_ms = segment_delay_ms(segment);
    let base_grace_ms = if segment.has_input_text || segment.has_output_text {
        S2S_ORDERED_TRANSCRIPT_PENDING_SKIP_MS
    } else {
        S2S_ORDERED_PENDING_SKIP_MS
    };
    let source_multiplier = if segment.has_output_text { 4 } else { 2 };
    let grace_ms = base_grace_ms + segment.source_audio_ms as u128 * source_multiplier;
    delay_ms >= grace_ms
}

pub(super) fn s2s_backlog_ms() -> u32 {
    crate::overlay::realtime_webview::state::REALTIME_S2S_AUDIO_BACKLOG_MS.load(Ordering::Relaxed)
}

fn update_s2s_ready_backlog(segments: &BTreeMap<u64, SegmentPlayback>, next_play_id: u64) {
    let ready_ms = segments
        .range((next_play_id + 1)..)
        .filter(|(_, segment)| !segment.chunks.is_empty())
        .map(|(_, segment)| segment.source_audio_ms as u32)
        .sum();
    crate::overlay::realtime_webview::state::REALTIME_S2S_READY_BACKLOG_MS
        .store(ready_ms, Ordering::Relaxed);
}

fn decrement_s2s_backlog(audio_ms: usize) {
    crate::overlay::realtime_webview::state::REALTIME_S2S_AUDIO_BACKLOG
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            Some(value.saturating_sub(1))
        })
        .ok();
    crate::overlay::realtime_webview::state::REALTIME_S2S_AUDIO_BACKLOG_MS
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            Some(value.saturating_sub(audio_ms as u32))
        })
        .ok();
    if crate::overlay::realtime_webview::state::REALTIME_S2S_AUDIO_BACKLOG.load(Ordering::Relaxed)
        == 0
    {
        crate::overlay::realtime_webview::state::REALTIME_S2S_AUDIO_DELAY_MS
            .store(0, Ordering::Relaxed);
        crate::overlay::realtime_webview::state::REALTIME_S2S_READY_BACKLOG_MS
            .store(0, Ordering::Relaxed);
    }
}

struct RealtimePlaybackBridge {
    tx: mpsc::Sender<AudioEvent>,
}

impl RealtimePlaybackBridge {
    fn new(hwnd: isize) -> Self {
        let (tx, rx) = mpsc::channel();
        let generation = TTS_MANAGER.interrupt_generation.load(Ordering::SeqCst);
        let request_id = S2S_PLAYBACK_COUNTER.fetch_add(1, Ordering::SeqCst);
        {
            let mut queue = TTS_MANAGER.playback_queue.lock().unwrap();
            queue.push_back((rx, hwnd, request_id, generation, true));
        }
        TTS_MANAGER.playback_signal.notify_one();
        Self { tx }
    }

    fn push(&self, bytes: Vec<u8>) {
        let _ = self.tx.send(AudioEvent::Data(bytes));
    }

    fn end(self) {
        let _ = self.tx.send(AudioEvent::End);
    }
}
