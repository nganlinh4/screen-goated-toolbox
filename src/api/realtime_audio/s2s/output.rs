use super::*;

#[path = "segment_texts.rs"]
mod segment_texts;
use segment_texts::SegmentTexts;

struct SegmentPlayback {
    chunks: VecDeque<Vec<u8>>,
    chunk_count: usize,
    byte_count: usize,
    source_audio_ms: usize,
    queued_at: Option<Instant>,
    playback_epoch: Option<u64>,
    _backlog: Option<Arc<super::backlog::S2sBacklogLease>>,
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
            playback_epoch: None,
            _backlog: None,
            has_input_text: false,
            has_output_text: false,
            done: false,
            error: false,
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "S2S output owns session identity, state, and playback"
)]
pub(super) fn coordinate_output(
    event_rx: mpsc::Receiver<S2sEvent>,
    stop_signal: Arc<AtomicBool>,
    session_stopped: Arc<AtomicBool>,
    session_id: u64,
    engine_revision: u64,
    overlay_hwnd: HWND,
    translation_hwnd: Option<HWND>,
    state: SharedRealtimeState,
    context_memory: Arc<Mutex<S2sContextMemory>>,
    mode: S2sMode,
    backlog: Arc<super::backlog::S2sBacklogRun>,
) {
    let mut next_play_id = 0u64;
    let mut segments = BTreeMap::<u64, SegmentPlayback>::new();
    let mut inputs = SegmentTexts::default();
    let mut outputs = SegmentTexts::default();
    let mut playback: Option<RealtimePlaybackBridge> = None;
    let mut admitted = Vec::new();
    let mut playback_epoch = TTS_MANAGER.realtime_epoch();

    while !stop_signal.load(Ordering::SeqCst) && !session_stopped.load(Ordering::SeqCst) {
        let event = event_rx.recv_timeout(Duration::from_millis(30));
        if matches!(&event, Err(mpsc::RecvTimeoutError::Disconnected)) {
            break;
        }
        let mut retired = Vec::new();
        let published = crate::overlay::realtime_webview::state::publish_if_engine_current(
            session_id,
            &session_stopped,
            engine_revision,
            || {
                if stop_signal.load(Ordering::SeqCst) {
                    return;
                }
                let visible =
                    crate::overlay::realtime_webview::state::TRANS_VISIBLE.load(Ordering::SeqCst);
                let epoch = TTS_MANAGER.realtime_epoch();
                if retire_stale_audio(&mut segments, playback_epoch, epoch, visible) {
                    playback.take();
                }
                playback_epoch = epoch;
                match event {
                    Ok(event) => match event {
                        S2sEvent::Queued {
                            id,
                            audio_ms,
                            queued_at,
                            playback_epoch: segment_epoch,
                            backlog,
                        } => {
                            if id < next_play_id {
                                return;
                            }
                            let segment = segments.entry(id).or_insert_with(SegmentPlayback::new);
                            segment.source_audio_ms = audio_ms;
                            segment.queued_at = Some(queued_at);
                            segment.playback_epoch = segment_epoch;
                            segment._backlog = Some(backlog);
                        }
                        S2sEvent::InputText { id, text } => {
                            if id >= next_play_id {
                                segments
                                    .entry(id)
                                    .or_insert_with(SegmentPlayback::new)
                                    .has_input_text = true;
                            }
                            inputs.replace(id, text);
                            publish_text(&state, overlay_hwnd, translation_hwnd, &inputs, &outputs);
                        }
                        S2sEvent::OutputText { id, text } => {
                            if id >= next_play_id {
                                segments
                                    .entry(id)
                                    .or_insert_with(SegmentPlayback::new)
                                    .has_output_text = true;
                            }
                            outputs.merge(id, &text);
                            publish_text(&state, overlay_hwnd, translation_hwnd, &inputs, &outputs);
                        }
                        S2sEvent::Audio { id, bytes } => {
                            if id < next_play_id {
                                return;
                            }
                            let segment = segments.entry(id).or_insert_with(SegmentPlayback::new);
                            queue_audio(segment, bytes, playback_epoch, visible);
                        }
                        S2sEvent::Done { id } => {
                            if id >= next_play_id {
                                segments.entry(id).or_insert_with(SegmentPlayback::new).done = true;
                            }
                            push_completed_context(id, &inputs, &outputs, &context_memory);
                            inputs.mark_done(id);
                            outputs.mark_done(id);
                        }
                        S2sEvent::Error { id, message } => {
                            inputs.mark_done(id);
                            outputs.mark_done(id);
                            if id < next_play_id {
                                return;
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
                            if let Some(current) = playback.take()
                                && let Some(notice) = TTS_MANAGER.retire_if_active(current.id)
                            {
                                retired.push(notice);
                            }
                        }
                    },
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => unreachable!(),
                }

                drain_ordered_audio(
                    &mut segments,
                    &mut next_play_id,
                    &mut playback,
                    translation_hwnd.unwrap_or(overlay_hwnd),
                    mode,
                    &stop_signal,
                    playback_epoch,
                    &mut admitted,
                    &backlog,
                    &mut retired,
                );
                update_s2s_ready_backlog(&segments, next_play_id, &backlog);
            },
        );
        TTS_MANAGER.notify_retired_playback(retired);
        if !published || stop_signal.load(Ordering::SeqCst) {
            break;
        }
    }

    if stop_signal.load(Ordering::SeqCst)
        || session_stopped.load(Ordering::SeqCst)
        || is_stale_session(session_id)
        || !crate::overlay::realtime_webview::state::engine_is_current(engine_revision)
    {
        for id in admitted {
            TTS_MANAGER.stop_if_active(id);
        }
    } else if let Some(current) = playback.take() {
        current.end();
    }
}

fn is_current_audio(produced: Option<u64>, current: u64, visible: bool) -> bool {
    visible && produced == Some(current)
}

fn queue_audio(segment: &mut SegmentPlayback, bytes: Vec<u8>, epoch: u64, visible: bool) {
    segment.chunk_count += 1;
    segment.byte_count += bytes.len();
    if is_current_audio(segment.playback_epoch, epoch, visible) {
        segment.chunks.push_back(bytes);
    }
}

fn retire_stale_audio(
    segments: &mut BTreeMap<u64, SegmentPlayback>,
    previous_epoch: u64,
    current_epoch: u64,
    visible: bool,
) -> bool {
    if previous_epoch == current_epoch && visible {
        return false;
    }
    for segment in segments.values_mut() {
        segment.chunks.clear();
    }
    true
}

#[expect(
    clippy::too_many_arguments,
    reason = "ordered playback needs its queue, run ownership, and current admission state"
)]
fn drain_ordered_audio(
    segments: &mut BTreeMap<u64, SegmentPlayback>,
    next_play_id: &mut u64,
    playback: &mut Option<RealtimePlaybackBridge>,
    hwnd: HWND,
    mode: S2sMode,
    stop_signal: &AtomicBool,
    playback_epoch: u64,
    admitted: &mut Vec<u64>,
    backlog: &super::backlog::S2sBacklogRun,
    retired: &mut Vec<(isize, u64)>,
) {
    if stop_signal.load(Ordering::SeqCst) {
        if let Some(current) = playback.take()
            && let Some(notice) = TTS_MANAGER.retire_if_active(current.id)
        {
            retired.push(notice);
        }
        return;
    }
    loop {
        let Some(segment) = segments.get_mut(next_play_id) else {
            return;
        };
        if segment.playback_epoch != Some(playback_epoch) {
            segments.remove(next_play_id);
            *next_play_id += 1;
            continue;
        }
        backlog.set_delay(segment_delay_ms(segment) as u32);
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
            segments.remove(next_play_id);
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
            segments.remove(next_play_id);
            *next_play_id += 1;
            continue;
        }
        if playback
            .as_ref()
            .is_some_and(|bridge| !TTS_MANAGER.is_speaking(bridge.id))
        {
            *playback = None;
        }
        if playback.is_none() && !segment.chunks.is_empty() {
            let delay_ms = segment_delay_ms(segment);
            backlog.set_delay(delay_ms as u32);
            *playback = RealtimePlaybackBridge::new(hwnd.0 as isize, stop_signal, playback_epoch);
            if let Some(bridge) = playback.as_ref() {
                admitted.retain(|id| TTS_MANAGER.is_speaking(*id));
                admitted.push(bridge.id);
            }
            if playback.is_none() {
                segment.chunks.clear();
            }
        }
        if let Some(player) = playback.as_ref() {
            while let Some(bytes) = segment.chunks.pop_front() {
                player.push(bytes);
            }
        }
        if segment.done && segment.chunks.is_empty() {
            let next_id = *next_play_id + 1;
            let _ = segment;
            let next_ready = segments
                .get(&next_id)
                .is_some_and(|next| !next.chunks.is_empty());
            segments.remove(next_play_id);
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
    inputs: &SegmentTexts,
    outputs: &SegmentTexts,
) {
    let source_display = inputs.visuals();
    let target_display = outputs.visuals();
    if let Ok(mut s) = state.lock() {
        s.transcript_committed_pos = source_display.committed_len;
        s.last_committed_pos = s.transcript_committed_pos;
        s.uncommitted_source_start = source_display.committed_len;
        s.uncommitted_source_end = source_display.full.len();
        s.full_transcript = source_display.full;
        s.set_display_transcript("\n\n");
        s.committed_translation = target_display.committed;
        s.uncommitted_translation = target_display.draft;
        s.display_translation = target_display.full;
    }
    notify_transcript_changed(overlay_hwnd);
    if let Some(hwnd) = translation_hwnd {
        notify_translation_changed(hwnd);
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
    if let Ok(mut s) = state.lock() {
        s.transcript_committed_pos = source_committed_len;
        s.last_committed_pos = source_committed_len;
        s.uncommitted_source_start = source_committed_len;
        s.uncommitted_source_end = source_full.len();
        s.full_transcript = source_full;
        s.set_display_transcript("\n\n");
        s.committed_translation = target_committed;
        s.uncommitted_translation = target_draft;
        s.display_translation = target_full;
    }
    notify_transcript_changed(overlay_hwnd);
    if let Some(hwnd) = translation_hwnd {
        notify_translation_changed(hwnd);
    }
}

fn clamp_to_char_boundary(value: &str, mut index: usize) -> usize {
    index = index.min(value.len());
    while index > 0 && !value.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn push_completed_context(
    id: u64,
    inputs: &SegmentTexts,
    outputs: &SegmentTexts,
    context_memory: &Arc<Mutex<S2sContextMemory>>,
) {
    let source = inputs.get(id);
    let target = outputs.get(id);
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
    super::backlog::current_backlog().audio_ms
}

fn update_s2s_ready_backlog(
    segments: &BTreeMap<u64, SegmentPlayback>,
    next_play_id: u64,
    backlog: &super::backlog::S2sBacklogRun,
) {
    let ready_ms = segments
        .range((next_play_id + 1)..)
        .filter(|(_, segment)| !segment.chunks.is_empty())
        .map(|(_, segment)| segment.source_audio_ms as u32)
        .sum();
    backlog.set_ready(ready_ms);
}

struct RealtimePlaybackBridge {
    tx: mpsc::Sender<AudioEvent>,
    id: u64,
}

impl RealtimePlaybackBridge {
    fn new(hwnd: isize, session_stopped: &AtomicBool, epoch: u64) -> Option<Self> {
        let (tx, rx) = mpsc::channel();
        let id = TTS_MANAGER.enqueue_realtime_audio(
            rx,
            hwnd,
            session_stopped,
            &crate::overlay::realtime_webview::state::TRANS_VISIBLE,
            epoch,
        )?;
        Some(Self { tx, id })
    }

    fn push(&self, bytes: Vec<u8>) {
        let _ = self.tx.send(AudioEvent::Data(bytes));
    }

    fn end(self) {
        let _ = self.tx.send(AudioEvent::End);
    }
}

#[cfg(test)]
#[path = "output_tests.rs"]
mod tests;
