use std::collections::{BTreeMap, VecDeque};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::api::audio::extract_pcm_from_wav;
use crate::api::realtime_audio::websocket::{
    connect_websocket, parse_input_transcription, send_audio_chunk, send_audio_stream_end,
    set_socket_nonblocking, set_socket_short_timeout,
};
use crate::model_config::GEMINI_LIVE_API_MODEL_3_1;
use crate::overlay::screen_record::ipc::subtitles::wav_chunks::{
    SubtitleWavChunk, split_subtitle_wav_into_chunks,
};
use crate::APP;

use super::{SubtitleBackend, SubtitleBackendProgress, normalize_subtitle_text};
use crate::overlay::screen_record::ipc::subtitles::audio::MIN_SUBTITLE_DURATION_SEC;
use crate::overlay::screen_record::ipc::subtitles::types::CompactSubtitleSegment;

const MAX_CONCURRENT_GEMINI_SUBTITLE_JOBS: usize = 5;
const GEMINI_SUBTITLE_CHUNK_PCM_SAMPLES: usize = 1600;
const GEMINI_SUBTITLE_STREAM_SLEEP_MS: u64 = 10;
const GEMINI_SUBTITLE_SETUP_TIMEOUT: Duration = Duration::from_secs(20);
const GEMINI_SUBTITLE_DRAIN_TIMEOUT: Duration = Duration::from_millis(1800);
const GEMINI_SUBTITLE_CHUNK_WALL_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_CHUNK_RETRIES: usize = 6;

type LiveSocket = tungstenite::WebSocket<native_tls::TlsStream<std::net::TcpStream>>;

pub struct GeminiLiveSubtitleBackend {
    api_key: String,
    model: String,
}

impl GeminiLiveSubtitleBackend {
    pub fn new() -> Result<Self, String> {
        let app = APP.lock().map_err(|_| "APP lock poisoned".to_string())?;
        if !app.config.use_gemini {
            return Err("Gemini Live subtitles require Gemini to be enabled in settings.".to_string());
        }
        if app.config.gemini_api_key.trim().is_empty() {
            return Err("Gemini Live subtitles require a Gemini API key.".to_string());
        }

        Ok(Self {
            api_key: app.config.gemini_api_key.clone(),
            model: GEMINI_LIVE_API_MODEL_3_1.to_string(),
        })
    }
}

impl SubtitleBackend for GeminiLiveSubtitleBackend {
    fn transcribe_clip(
        &mut self,
        audio_data: Vec<u8>,
        _language_hint: Option<&str>,
        on_progress: &mut dyn FnMut(SubtitleBackendProgress) -> Result<(), String>,
    ) -> Result<Vec<CompactSubtitleSegment>, String> {
        let chunks = split_subtitle_wav_into_chunks(&audio_data)?;
        if chunks.is_empty() {
            return Ok(Vec::new());
        }

        let total_steps = chunks.len();
        let max_parallel = MAX_CONCURRENT_GEMINI_SUBTITLE_JOBS.min(total_steps);
        let (result_tx, result_rx) = mpsc::channel::<GeminiWorkerMessage>();
        let mut pending = chunks
            .into_iter()
            .enumerate()
            .map(|(chunk_index, chunk)| GeminiChunkTask {
                chunk_index,
                attempt: 0,
                ready_at: Instant::now(),
                chunk,
            })
            .collect::<VecDeque<_>>();
        let mut completed = BTreeMap::<usize, Vec<CompactSubtitleSegment>>::new();
        let mut partial = BTreeMap::<usize, Vec<CompactSubtitleSegment>>::new();
        let workers = spawn_gemini_workers(max_parallel, &self.api_key, &self.model, result_tx);
        let mut idle_workers = (0..workers.len()).collect::<VecDeque<_>>();
        let mut active_jobs = 0usize;
        let mut concurrency_limit = max_parallel;
        let mut success_streak = 0usize;

        while completed.len() < total_steps {
            dispatch_ready_gemini_jobs(
                &workers,
                &mut idle_workers,
                &mut pending,
                &mut active_jobs,
                concurrency_limit,
            );

            if active_jobs == 0 {
                if let Some(delay) = next_pending_delay(&pending) {
                    std::thread::sleep(delay);
                    continue;
                }
                break;
            }

            let recv_timeout = next_pending_delay(&pending).unwrap_or(Duration::from_millis(250));
            let event = match result_rx.recv_timeout(recv_timeout) {
                Ok(event) => event,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("Gemini Live subtitle workers disconnected unexpectedly.".to_string());
                }
            };
            match event {
                GeminiWorkerMessage::Partial {
                    worker_id: _worker_id,
                    chunk_index,
                    segments,
                } => {
                    if !completed.contains_key(&chunk_index) {
                        partial.insert(chunk_index, segments);
                        on_progress(SubtitleBackendProgress {
                            completed_steps: completed.len(),
                            total_steps,
                            segments: flatten_progress_segments(&completed, &partial),
                        })?;
                    }
                }
                GeminiWorkerMessage::Finished(event) => {
                    active_jobs = active_jobs.saturating_sub(1);
                    idle_workers.push_back(event.worker_id);
                    partial.remove(&event.chunk.task.chunk_index);

                    match event.chunk.result {
                        Ok(segments) => {
                            completed.insert(event.chunk.task.chunk_index, segments);
                            success_streak += 1;
                            if success_streak >= concurrency_limit && concurrency_limit < max_parallel {
                                concurrency_limit += 1;
                                success_streak = 0;
                            }

                            on_progress(SubtitleBackendProgress {
                                completed_steps: completed.len(),
                                total_steps,
                                segments: flatten_progress_segments(&completed, &partial),
                            })?;
                        }
                        Err(error)
                            if is_retryable_gemini_error(&error)
                                && event.chunk.task.attempt + 1 < MAX_CHUNK_RETRIES =>
                        {
                            success_streak = 0;
                            concurrency_limit = concurrency_limit.saturating_sub(1).max(1);
                            pending.push_back(GeminiChunkTask {
                                chunk_index: event.chunk.task.chunk_index,
                                attempt: event.chunk.task.attempt + 1,
                                ready_at: Instant::now()
                                    + gemini_retry_backoff(event.chunk.task.attempt + 1),
                                chunk: event.chunk.task.chunk,
                            });
                            on_progress(SubtitleBackendProgress {
                                completed_steps: completed.len(),
                                total_steps,
                                segments: flatten_progress_segments(&completed, &partial),
                            })?;
                        }
                        Err(error) => {
                            return Err(format!(
                                "Gemini Live subtitle request failed for chunk {}/{}: {}",
                                event.chunk.task.chunk_index + 1,
                                total_steps,
                                error
                            ));
                        }
                    }
                }
            }
        }

        Ok(flatten_completed_segments(&completed))
    }
}

struct GeminiChunkTask {
    chunk_index: usize,
    attempt: usize,
    ready_at: Instant,
    chunk: SubtitleWavChunk,
}

struct GeminiChunkResult {
    task: GeminiChunkTask,
    result: Result<Vec<CompactSubtitleSegment>, String>,
}

struct GeminiWorkerResult {
    worker_id: usize,
    chunk: GeminiChunkResult,
}

enum GeminiWorkerMessage {
    Partial {
        worker_id: usize,
        chunk_index: usize,
        segments: Vec<CompactSubtitleSegment>,
    },
    Finished(GeminiWorkerResult),
}

struct GeminiWorkerHandle {
    task_tx: mpsc::Sender<GeminiChunkTask>,
}

fn dispatch_ready_gemini_jobs(
    workers: &[GeminiWorkerHandle],
    idle_workers: &mut VecDeque<usize>,
    pending: &mut VecDeque<GeminiChunkTask>,
    active_jobs: &mut usize,
    concurrency_limit: usize,
) {
    while *active_jobs < concurrency_limit {
        let Some(worker_id) = idle_workers.pop_front() else {
            break;
        };
        let Some(ready_index) = pending.iter().position(|task| task.ready_at <= Instant::now()) else {
            idle_workers.push_front(worker_id);
            break;
        };
        let task = pending
            .remove(ready_index)
            .expect("ready_index resolved against pending queue");
        if workers[worker_id].task_tx.send(task).is_err() {
            continue;
        }
        *active_jobs += 1;
    }
}

fn next_pending_delay(pending: &VecDeque<GeminiChunkTask>) -> Option<Duration> {
    pending
        .iter()
        .map(|task| task.ready_at)
        .min()
        .map(|ready_at| ready_at.saturating_duration_since(Instant::now()))
}

fn flatten_completed_segments(
    completed: &BTreeMap<usize, Vec<CompactSubtitleSegment>>,
) -> Vec<CompactSubtitleSegment> {
    completed
        .values()
        .flat_map(|segments| segments.iter().cloned())
        .collect()
}

fn flatten_progress_segments(
    completed: &BTreeMap<usize, Vec<CompactSubtitleSegment>>,
    partial: &BTreeMap<usize, Vec<CompactSubtitleSegment>>,
) -> Vec<CompactSubtitleSegment> {
    let mut merged = BTreeMap::<usize, Vec<CompactSubtitleSegment>>::new();
    for (&chunk_index, segments) in partial {
        merged.insert(chunk_index, segments.clone());
    }
    for (&chunk_index, segments) in completed {
        merged.insert(chunk_index, segments.clone());
    }
    merged
        .values()
        .flat_map(|segments| segments.iter().cloned())
        .collect()
}

fn gemini_retry_backoff(attempt: usize) -> Duration {
    Duration::from_millis((300 * (1usize << attempt.min(4))) as u64)
}

fn spawn_gemini_workers(
    worker_count: usize,
    api_key: &str,
    model: &str,
    result_tx: mpsc::Sender<GeminiWorkerMessage>,
) -> Vec<GeminiWorkerHandle> {
    (0..worker_count)
        .map(|worker_id| {
            let (task_tx, task_rx) = mpsc::channel::<GeminiChunkTask>();
            let api_key = api_key.to_string();
            let model = model.to_string();
            let result_tx = result_tx.clone();
            std::thread::spawn(move || {
                let mut socket: Option<LiveSocket> = None;
                while let Ok(task) = task_rx.recv() {
                    let result = transcribe_gemini_chunk(
                        &api_key,
                        &model,
                        &mut socket,
                        &task.chunk,
                        |segments| {
                            let _ = result_tx.send(GeminiWorkerMessage::Partial {
                                worker_id,
                                chunk_index: task.chunk_index,
                                segments,
                            });
                        },
                    );
                    if result.is_err() {
                        reset_gemini_session(&mut socket);
                    }
                    let _ = result_tx.send(GeminiWorkerMessage::Finished(GeminiWorkerResult {
                        worker_id,
                        chunk: GeminiChunkResult { task, result },
                    }));
                }
                reset_gemini_session(&mut socket);
            });
            GeminiWorkerHandle { task_tx }
        })
        .collect()
}

fn is_retryable_gemini_error(error: &str) -> bool {
    let normalized = error.to_ascii_lowercase();
    [
        "429",
        "quota",
        "resource exhausted",
        "rate limit",
        "timed out",
        "timeout",
        "wall timeout",
        "try again",
        "temporarily unavailable",
        "connection reset",
        "broken pipe",
        "connection closed",
        "closed normally",
        "closed before setupcomplete",
        "goaway",
        "unavailable",
        "eof",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
}

fn transcribe_gemini_chunk(
    api_key: &str,
    model: &str,
    socket: &mut Option<LiveSocket>,
    chunk: &SubtitleWavChunk,
    mut on_partial: impl FnMut(Vec<CompactSubtitleSegment>),
) -> Result<Vec<CompactSubtitleSegment>, String> {
    let chunk_duration = wav_duration_seconds(&chunk.wav_data)?;
    let transcript = transcribe_wav_with_gemini_live(
        api_key,
        model,
        socket,
        &chunk.wav_data,
        |text| {
            let normalized = normalize_subtitle_text(text);
            if normalized.is_empty() {
                return;
            }
            let segments = estimate_subtitle_segments(&normalized, chunk_duration)
                .into_iter()
                .map(|mut segment| {
                    segment.start_time += chunk.start_time_sec;
                    segment.end_time += chunk.start_time_sec;
                    segment
                })
                .collect::<Vec<_>>();
            on_partial(segments);
        },
    )?;
    let normalized = normalize_subtitle_text(&transcript);
    if normalized.is_empty() {
        return Ok(Vec::new());
    }

    let estimated = estimate_subtitle_segments(&normalized, chunk_duration);
    Ok(estimated
        .into_iter()
        .map(|mut segment| {
            segment.start_time += chunk.start_time_sec;
            segment.end_time += chunk.start_time_sec;
            segment
        })
        .collect())
}

fn transcribe_wav_with_gemini_live(
    api_key: &str,
    model: &str,
    socket: &mut Option<LiveSocket>,
    wav_data: &[u8],
    mut on_partial_text: impl FnMut(&str),
) -> Result<String, String> {
    let socket = ensure_gemini_session(api_key, model, socket)?;
    let pcm_samples =
        extract_pcm_from_wav(wav_data).map_err(|err| format!("extract PCM from WAV: {err}"))?;
    let mut transcript = String::new();
    let turn_started = Instant::now();

    for chunk in pcm_samples.chunks(GEMINI_SUBTITLE_CHUNK_PCM_SAMPLES) {
        if turn_started.elapsed() >= GEMINI_SUBTITLE_CHUNK_WALL_TIMEOUT {
            return Err("chunk wall timeout".to_string());
        }
        send_audio_chunk(socket, chunk)
            .map_err(|err| format!("send audio chunk: {err}"))?;
        drain_live_input_messages(
            socket,
            &mut transcript,
            Duration::from_millis(60),
            turn_started,
            &mut on_partial_text,
        )?;
        std::thread::sleep(Duration::from_millis(GEMINI_SUBTITLE_STREAM_SLEEP_MS));
    }

    if turn_started.elapsed() >= GEMINI_SUBTITLE_CHUNK_WALL_TIMEOUT {
        return Err("chunk wall timeout".to_string());
    }
    send_audio_stream_end(socket).map_err(|err| format!("send audioStreamEnd: {err}"))?;
    drain_live_input_messages(
        socket,
        &mut transcript,
        GEMINI_SUBTITLE_DRAIN_TIMEOUT,
        turn_started,
        &mut on_partial_text,
    )?;

    Ok(transcript)
}

fn ensure_gemini_session<'a>(
    api_key: &str,
    model: &str,
    socket: &'a mut Option<LiveSocket>,
) -> Result<&'a mut LiveSocket, String> {
    if socket.is_none() {
        *socket = Some(connect_gemini_session(api_key, model)?);
    }
    socket
        .as_mut()
        .ok_or_else(|| "Gemini Live session was not initialized".to_string())
}

fn connect_gemini_session(api_key: &str, model: &str) -> Result<LiveSocket, String> {
    let mut socket = connect_websocket(api_key).map_err(|err| format!("connect websocket: {err}"))?;
    send_gemini_subtitle_setup_message(&mut socket, model)?;
    set_socket_short_timeout(&mut socket).map_err(|err| format!("set setup timeout: {err}"))?;
    wait_for_gemini_setup_complete(&mut socket)?;
    set_socket_nonblocking(&mut socket).map_err(|err| format!("set non-blocking: {err}"))?;
    Ok(socket)
}

fn reset_gemini_session(socket: &mut Option<LiveSocket>) {
    if let Some(mut live_socket) = socket.take() {
        let _ = live_socket.close(None);
    }
}

fn send_gemini_subtitle_setup_message(
    socket: &mut tungstenite::WebSocket<native_tls::TlsStream<std::net::TcpStream>>,
    model: &str,
) -> Result<(), String> {
    let setup = serde_json::json!({
        "setup": {
            "model": format!("models/{model}"),
            "generationConfig": {
                "responseModalities": ["AUDIO"],
                "mediaResolution": "MEDIA_RESOLUTION_LOW",
                "thinkingConfig": {
                    "thinkingLevel": "minimal"
                }
            },
            "inputAudioTranscription": {},
            "systemInstruction": {
                "parts": [{
                    "text": "Listen to the incoming audio and do not answer. The input transcription is the only output that matters."
                }]
            }
        }
    });

    socket
        .write(tungstenite::Message::Text(setup.to_string().into()))
        .map_err(|err| format!("send Gemini Live setup: {err}"))?;
    socket
        .flush()
        .map_err(|err| format!("flush Gemini Live setup: {err}"))
}

fn wait_for_gemini_setup_complete(
    socket: &mut tungstenite::WebSocket<native_tls::TlsStream<std::net::TcpStream>>,
) -> Result<(), String> {
    let started = Instant::now();
    loop {
        match socket.read() {
            Ok(tungstenite::Message::Text(msg)) => {
                let message = msg.as_str();
                if message.contains("setupComplete") {
                    return Ok(());
                }
                if message.contains("\"error\"") {
                    eprintln!("[GeminiSubtitle] setup error payload: {message}");
                    return Err(format!("setup error: {message}"));
                }
            }
            Ok(tungstenite::Message::Binary(data)) => {
                let message = String::from_utf8_lossy(&data);
                if message.contains("setupComplete") {
                    return Ok(());
                }
                if message.contains("\"error\"") {
                    eprintln!("[GeminiSubtitle] setup binary error payload: {message}");
                    return Err(format!("setup error: {message}"));
                }
            }
            Ok(tungstenite::Message::Close(frame)) => {
                let close_info = frame
                    .map(|value| format!("code={}, reason={}", value.code, value.reason))
                    .unwrap_or_else(|| "no close frame".to_string());
                eprintln!("[GeminiSubtitle] setup close frame: {close_info}");
                return Err(format!("setup closed before setupComplete: {close_info}"));
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(ref err))
                if err.kind() == std::io::ErrorKind::WouldBlock
                    || err.kind() == std::io::ErrorKind::TimedOut =>
            {
                if started.elapsed() >= GEMINI_SUBTITLE_SETUP_TIMEOUT {
                    return Err("setup timeout".to_string());
                }
            }
            Err(tungstenite::Error::ConnectionClosed) => {
                eprintln!("[GeminiSubtitle] setup connection closed before setupComplete");
                return Err("setup closed before setupComplete".to_string());
            }
            Err(tungstenite::Error::AlreadyClosed) => {
                eprintln!("[GeminiSubtitle] setup already closed before setupComplete");
                return Err("setup closed before setupComplete".to_string());
            }
            Err(err) => {
                eprintln!("[GeminiSubtitle] setup read failed: {err}");
                return Err(format!("setup read failed: {err}"));
            }
        }
    }
}

fn drain_live_input_messages(
    socket: &mut tungstenite::WebSocket<native_tls::TlsStream<std::net::TcpStream>>,
    transcript: &mut String,
    idle_timeout: Duration,
    turn_started: Instant,
    on_partial_text: &mut dyn FnMut(&str),
) -> Result<(), String> {
    let started = Instant::now();
    let mut last_transcript_at = Instant::now();

    loop {
        if turn_started.elapsed() >= GEMINI_SUBTITLE_CHUNK_WALL_TIMEOUT {
            return Err("chunk wall timeout".to_string());
        }
        match socket.read() {
            Ok(tungstenite::Message::Text(msg)) => {
                let message = msg.as_str();
                if let Some(text) = parse_input_transcription(message) {
                    if merge_input_transcription(transcript, &text) {
                        on_partial_text(transcript);
                    }
                    last_transcript_at = Instant::now();
                } else if is_live_turn_complete_message(message) {
                    return Ok(());
                } else if message.contains("\"error\"") {
                    return Err(format!("server error: {message}"));
                }
            }
            Ok(tungstenite::Message::Binary(data)) => {
                let message = String::from_utf8_lossy(&data);
                if let Some(text) = parse_input_transcription(&message) {
                    if merge_input_transcription(transcript, &text) {
                        on_partial_text(transcript);
                    }
                    last_transcript_at = Instant::now();
                } else if is_live_turn_complete_message(&message) {
                    return Ok(());
                } else if message.contains("\"error\"") {
                    return Err(format!("server error: {message}"));
                }
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(ref err))
                if err.kind() == std::io::ErrorKind::WouldBlock
                    || err.kind() == std::io::ErrorKind::TimedOut =>
            {
                if last_transcript_at.elapsed() >= idle_timeout || started.elapsed() >= idle_timeout {
                    return Ok(());
                }
            }
            Err(tungstenite::Error::ConnectionClosed)
            | Err(tungstenite::Error::AlreadyClosed) => {
                return Ok(());
            }
            Err(err) => return Err(format!("read live transcription: {err}")),
        }
    }
}

fn is_live_turn_complete_message(message: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(message)
        .ok()
        .and_then(|json| json.get("serverContent").cloned())
        .is_some_and(|server_content| {
            server_content
                .get("turnComplete")
                .and_then(|value| value.as_bool())
                .unwrap_or(false)
                || server_content
                    .get("generationComplete")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false)
        })
}

fn merge_input_transcription(accumulated: &mut String, incoming: &str) -> bool {
    let incoming = normalize_subtitle_text(incoming);
    if incoming.is_empty() {
        return false;
    }

    if accumulated.is_empty() {
        *accumulated = incoming;
        return true;
    }
    if accumulated == &incoming || accumulated.ends_with(&incoming) {
        return false;
    }
    if incoming.starts_with(accumulated.as_str()) {
        *accumulated = incoming;
        return true;
    }

    let overlap = largest_overlap_suffix_prefix(accumulated, &incoming);
    if overlap > 0 {
        accumulated.push_str(&incoming[overlap..]);
    } else {
        accumulated.push(' ');
        accumulated.push_str(&incoming);
    }
    *accumulated = normalize_subtitle_text(accumulated);
    true
}

fn largest_overlap_suffix_prefix(left: &str, right: &str) -> usize {
    let max = left.len().min(right.len());
    (1..=max)
        .rev()
        .find(|&len| right.is_char_boundary(len) && left.ends_with(&right[..len]))
        .unwrap_or(0)
}

fn wav_duration_seconds(wav_data: &[u8]) -> Result<f64, String> {
    let samples =
        extract_pcm_from_wav(wav_data).map_err(|err| format!("extract WAV duration PCM: {err}"))?;
    Ok(samples.len() as f64 / 16_000.0)
}

fn estimate_subtitle_segments(text: &str, duration_sec: f64) -> Vec<CompactSubtitleSegment> {
    let normalized = normalize_subtitle_text(text);
    if normalized.is_empty() {
        return Vec::new();
    }

    let mut parts = split_subtitle_parts(&normalized);
    if parts.len() > 1 && duration_sec < parts.len() as f64 * MIN_SUBTITLE_DURATION_SEC {
        parts = vec![normalized];
    }

    let total_weight = parts
        .iter()
        .map(|part| part.split_whitespace().count().max(1) as f64)
        .sum::<f64>()
        .max(1.0);
    let total_duration = duration_sec.max(MIN_SUBTITLE_DURATION_SEC);
    let part_count = parts.len();
    let mut cursor = 0.0;
    let mut segments = Vec::with_capacity(part_count);

    for (index, part) in parts.into_iter().enumerate() {
        let remaining = total_duration - cursor;
        let weight = part.split_whitespace().count().max(1) as f64;
        let mut segment_duration = if index + 1 == part_count {
            remaining.max(MIN_SUBTITLE_DURATION_SEC)
        } else {
            (total_duration * (weight / total_weight)).max(MIN_SUBTITLE_DURATION_SEC)
        };
        if segment_duration > remaining && remaining > 0.0 {
            segment_duration = remaining;
        }
        let start_time = cursor;
        let end_time = if index + 1 == part_count {
            total_duration.max(start_time + MIN_SUBTITLE_DURATION_SEC)
        } else {
            (start_time + segment_duration).min(total_duration)
        };
        segments.push(CompactSubtitleSegment {
            start_time,
            end_time: end_time.max(start_time + MIN_SUBTITLE_DURATION_SEC),
            text: part,
        });
        cursor = end_time;
    }

    segments
}

fn split_subtitle_parts(text: &str) -> Vec<String> {
    let mut phrases = Vec::new();
    let mut current = String::new();
    let mut word_count = 0usize;
    let mut in_word = false;

    for ch in text.chars() {
        current.push(ch);
        if ch.is_whitespace() {
            in_word = false;
            if word_count >= 12 {
                push_phrase(&mut phrases, &mut current);
                word_count = 0;
            }
            continue;
        }
        if !in_word {
            word_count += 1;
            in_word = true;
        }

        let is_sentence_end = matches!(ch, '.' | '!' | '?' | '…');
        let is_soft_break = matches!(ch, ',' | ';' | ':' | '，' | '、') && word_count >= 8;
        if is_sentence_end || is_soft_break {
            push_phrase(&mut phrases, &mut current);
            word_count = 0;
            in_word = false;
        }
    }

    push_phrase(&mut phrases, &mut current);

    if phrases.is_empty() {
        return vec![text.to_string()];
    }

    phrases
}

fn push_phrase(phrases: &mut Vec<String>, buffer: &mut String) {
    let phrase = normalize_subtitle_text(buffer);
    if !phrase.is_empty() {
        phrases.push(phrase);
    }
    buffer.clear();
}
