use std::collections::{BTreeMap, VecDeque};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

use anyhow::Result;
use base64::{Engine as _, engine::general_purpose};
use tungstenite::Message;
use windows::Win32::Foundation::HWND;

use crate::APP;
use crate::api::tts::TTS_MANAGER;
use crate::api::tts::types::AudioEvent;
use crate::config::Preset;
use crate::overlay::realtime_webview::{
    AUDIO_SOURCE_CHANGE, LANGUAGE_CHANGE, SELECTED_APP_PID, TRANSCRIPTION_MODEL_CHANGE,
};

use super::capture::{start_mic_capture, start_per_app_capture};
use super::state::SharedRealtimeState;
use super::utils::{update_overlay_text, update_translation_text};
use super::websocket::{
    connect_websocket, send_audio_chunk, send_audio_stream_end, set_socket_nonblocking,
    set_socket_short_timeout,
};
use super::{REALTIME_RMS, WM_VOLUME_UPDATE};

const SESSION_COUNT: usize = 3;
const FRAME_SAMPLES: usize = 1600;
const PREROLL_SAMPLES: usize = 4000;
const MIN_SEGMENT_SAMPLES: usize = 16_000;
const TARGET_SEGMENT_SAMPLES: usize = 48_000;
const MAX_SEGMENT_SAMPLES: usize = 80_000;
const END_SILENCE_FRAMES: usize = 3;
const AUDIO_IDLE_FINISH_MS: u128 = 1_200;
const DISPLAY_SEGMENT_LIMIT: usize = 8;
const FIRST_AUDIO_RETRY_MS: u128 = 5_500;
const CONTEXT_SEGMENT_LIMIT: usize = 5;
const CONTEXT_LINE_CHAR_LIMIT: usize = 240;
const CONTEXT_TOTAL_CHAR_LIMIT: usize = 1_500;

static S2S_PLAYBACK_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
struct S2sContextEntry {
    id: u64,
    source: String,
    target: String,
}

#[derive(Default)]
struct S2sContextMemory {
    completed: VecDeque<S2sContextEntry>,
}

struct S2sContextSnapshot {
    text: String,
    segment_count: usize,
    char_count: usize,
}

impl S2sContextMemory {
    fn push_completed(&mut self, id: u64, source: &str, target: &str) {
        let source = truncate_chars(source.trim(), CONTEXT_LINE_CHAR_LIMIT);
        let target = truncate_chars(target.trim(), CONTEXT_LINE_CHAR_LIMIT);
        if source.is_empty() && target.is_empty() {
            return;
        }
        if self.completed.iter().any(|entry| entry.id == id) {
            return;
        }
        self.completed
            .push_back(S2sContextEntry { id, source, target });
        while self.completed.len() > CONTEXT_SEGMENT_LIMIT {
            self.completed.pop_front();
        }
    }

    fn snapshot(&self) -> S2sContextSnapshot {
        let entries = self
            .completed
            .iter()
            .rev()
            .take(CONTEXT_SEGMENT_LIMIT)
            .cloned()
            .collect::<Vec<_>>();
        if entries.is_empty() {
            return S2sContextSnapshot {
                text: String::new(),
                segment_count: 0,
                char_count: 0,
            };
        }

        let mut ordered = entries;
        ordered.reverse();
        let mut text = String::from(
            "\n\nPrevious context for continuity only. Do not translate or speak this again.\n",
        );
        text.push_str("Use it only for pronouns, names, terminology, and topic continuity.\n");
        for (index, entry) in ordered.iter().enumerate() {
            let number = index + 1;
            if !entry.source.is_empty() {
                text.push_str(&format!("Previous source {number}: {}\n", entry.source));
            }
            if !entry.target.is_empty() {
                text.push_str(&format!(
                    "Previous translation {number}: {}\n",
                    entry.target
                ));
            }
        }
        text.push_str("Now translate only the new incoming audio segment.");
        if text.chars().count() > CONTEXT_TOTAL_CHAR_LIMIT {
            text = truncate_chars(&text, CONTEXT_TOTAL_CHAR_LIMIT);
        }
        let char_count = text.chars().count();
        S2sContextSnapshot {
            text,
            segment_count: ordered.len(),
            char_count,
        }
    }
}

struct Segment {
    id: u64,
    samples: Vec<i16>,
    queued_at: Instant,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SegmentOutcome {
    Healthy,
    RetryFresh,
}

enum S2sEvent {
    Queued {
        id: u64,
        audio_ms: usize,
        queued_at: Instant,
    },
    InputText {
        id: u64,
        text: String,
    },
    OutputText {
        id: u64,
        text: String,
    },
    Audio {
        id: u64,
        bytes: Vec<u8>,
    },
    Done {
        id: u64,
    },
    Error {
        id: u64,
        message: String,
    },
    Interrupt,
}

pub fn run_gemini_live_s2s(
    preset: Preset,
    stop_signal: Arc<AtomicBool>,
    overlay_hwnd: HWND,
    translation_hwnd: Option<HWND>,
    state: SharedRealtimeState,
    session_id: u64,
) -> Result<()> {
    let settings = load_settings()?;
    apply_tts_speed_for_s2s(&settings.speed);
    let audio_buffer = Arc::new(Mutex::new(Vec::<i16>::new()));
    let pause = Arc::new(AtomicBool::new(false));
    let selected_pid = SELECTED_APP_PID.load(Ordering::SeqCst);
    let _stream = if preset.audio_source == "device" {
        let selected_pid = if selected_pid == 0 {
            crate::overlay::realtime_webview::app_selection::show_audio_app_selector_overlay();
            wait_for_selected_app(stop_signal.clone(), session_id)
        } else {
            Some(selected_pid)
        };
        if let Some(selected_pid) = selected_pid {
            #[cfg(target_os = "windows")]
            start_per_app_capture(
                selected_pid,
                audio_buffer.clone(),
                stop_signal.clone(),
                pause.clone(),
            )?;
            None
        } else {
            return Err(anyhow::anyhow!(
                "S2S device mode needs a selected app to avoid capturing its own translated audio"
            ));
        }
    } else {
        Some(start_mic_capture(
            audio_buffer.clone(),
            stop_signal.clone(),
            pause.clone(),
        )?)
    };

    if let Ok(mut s) = state.lock() {
        s.set_transcription_method(super::state::TranscriptionMethod::GeminiLiveS2s);
    }

    let (event_tx, event_rx) = mpsc::channel::<S2sEvent>();
    let context_memory = Arc::new(Mutex::new(S2sContextMemory::default()));
    let coordinator_stop = stop_signal.clone();
    let coordinator_state = state.clone();
    let coordinator_overlay = overlay_hwnd.0 as isize;
    let coordinator_translation = translation_hwnd.map(|hwnd| hwnd.0 as isize);
    let coordinator_context = context_memory.clone();
    std::thread::spawn(move || {
        coordinate_output(
            event_rx,
            coordinator_stop,
            HWND(coordinator_overlay as *mut std::ffi::c_void),
            coordinator_translation.map(|hwnd| HWND(hwnd as *mut std::ffi::c_void)),
            coordinator_state,
            coordinator_context,
        );
    });

    let mut segment_senders = Vec::with_capacity(SESSION_COUNT);
    for session_index in 0..SESSION_COUNT {
        let (segment_tx, segment_rx) = mpsc::channel::<Segment>();
        segment_senders.push(segment_tx);
        let worker_stop = stop_signal.clone();
        let worker_events = event_tx.clone();
        let worker_settings = settings.clone();
        let worker_context = context_memory.clone();
        std::thread::spawn(move || {
            session_worker(
                session_index,
                segment_rx,
                worker_events,
                worker_stop,
                worker_settings,
                worker_context,
            );
        });
    }

    run_vad_loop(
        audio_buffer,
        stop_signal,
        segment_senders,
        event_tx,
        overlay_hwnd,
        session_id,
    );

    Ok(())
}

fn apply_tts_speed_for_s2s(speed: &str) {
    let mapped_speed = match speed {
        "Slow" => 85,
        "Fast" => 125,
        _ => 100,
    };
    crate::overlay::realtime_webview::state::REALTIME_TTS_SPEED
        .store(mapped_speed, Ordering::SeqCst);
    crate::overlay::realtime_webview::state::CURRENT_TTS_SPEED
        .store(mapped_speed, Ordering::SeqCst);
}

fn wait_for_selected_app(stop_signal: Arc<AtomicBool>, session_id: u64) -> Option<u32> {
    let started = Instant::now();
    while !stop_signal.load(Ordering::SeqCst) && !is_stale_session(session_id) {
        let pid = SELECTED_APP_PID.load(Ordering::SeqCst);
        if pid > 0 {
            return Some(pid);
        }
        if started.elapsed() > Duration::from_secs(30) {
            return None;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    None
}

#[derive(Clone)]
struct S2sSettings {
    api_key: String,
    model: String,
    voice: String,
    speed: String,
    custom_instruction: String,
    target_language: String,
}

fn load_settings() -> Result<S2sSettings> {
    let app = APP.lock().unwrap();
    let api_key = app.config.gemini_api_key.trim().to_string();
    if api_key.is_empty() {
        return Err(anyhow::anyhow!("NO_API_KEY:google"));
    }
    let model = app.config.tts_gemini_live_model.trim();
    let voice = app.config.tts_voice.trim();
    let speed = app.config.tts_speed.trim();
    let target_language = app.config.realtime_target_language.clone();
    let custom_instruction =
        tts_instruction_for_target(&target_language, &app.config.tts_language_conditions);
    Ok(S2sSettings {
        api_key,
        model: if model.is_empty() {
            crate::model_config::GEMINI_LIVE_API_MODEL_3_1.to_string()
        } else {
            crate::model_config::normalize_tts_gemini_model(model).to_string()
        },
        voice: if voice.is_empty() {
            "Aoede".to_string()
        } else {
            voice.to_string()
        },
        speed: if speed.is_empty() {
            "Normal".to_string()
        } else {
            speed.to_string()
        },
        custom_instruction,
        target_language,
    })
}

fn run_vad_loop(
    audio_buffer: Arc<Mutex<Vec<i16>>>,
    stop_signal: Arc<AtomicBool>,
    segment_senders: Vec<mpsc::Sender<Segment>>,
    event_tx: mpsc::Sender<S2sEvent>,
    overlay_hwnd: HWND,
    session_id: u64,
) {
    let mut pending = Vec::<i16>::new();
    let mut preroll = VecDeque::<i16>::new();
    let mut active = Vec::<i16>::new();
    let mut segment_id = 0u64;
    let mut silence_frames = 0usize;
    let mut noise_floor = 0.004f32;

    while !stop_signal.load(Ordering::Relaxed) {
        if is_stale_session(session_id)
            || AUDIO_SOURCE_CHANGE.load(Ordering::SeqCst)
            || TRANSCRIPTION_MODEL_CHANGE.load(Ordering::SeqCst)
            || LANGUAGE_CHANGE.load(Ordering::SeqCst)
        {
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

            let speech_threshold = (noise_floor * 3.0).max(0.012);
            let is_speech = rms >= speech_threshold;
            if !is_speech {
                noise_floor = (noise_floor * 0.98) + (rms * 0.02);
            }

            if active.is_empty() {
                preroll.extend(frame.iter().copied());
                while preroll.len() > PREROLL_SAMPLES {
                    preroll.pop_front();
                }
                if is_speech {
                    active.extend(preroll.drain(..));
                    active.extend(frame);
                    silence_frames = 0;
                }
                continue;
            }

            active.extend(frame);
            silence_frames = if is_speech { 0 } else { silence_frames + 1 };

            let long_enough = active.len() >= MIN_SEGMENT_SAMPLES;
            let target_hit = active.len() >= TARGET_SEGMENT_SAMPLES;
            let max_hit = active.len() >= MAX_SEGMENT_SAMPLES;
            let silence_hit = target_hit && silence_frames >= END_SILENCE_FRAMES;
            if long_enough && (silence_hit || max_hit) {
                let queued_at = Instant::now();
                let segment = Segment {
                    id: segment_id,
                    samples: std::mem::take(&mut active),
                    queued_at,
                };
                let worker = (segment.id as usize) % segment_senders.len();
                let audio_ms = samples_to_ms(segment.samples.len());
                eprintln!(
                    "[RealtimeS2S] queued segment={} worker={} audio_ms={} reason={} backlog={}",
                    segment.id,
                    worker,
                    audio_ms,
                    if max_hit { "max" } else { "silence" },
                    crate::overlay::realtime_webview::state::REALTIME_S2S_AUDIO_BACKLOG
                        .load(Ordering::Relaxed)
                );
                let _ = event_tx.send(S2sEvent::Queued {
                    id: segment.id,
                    audio_ms,
                    queued_at,
                });
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
        let _ = event_tx.send(S2sEvent::Queued {
            id: segment_id,
            audio_ms,
            queued_at,
        });
        let _ = segment_senders[worker].send(Segment {
            id: segment_id,
            samples: active,
            queued_at,
        });
        eprintln!(
            "[RealtimeS2S] queued segment={} worker={} audio_ms={} reason=final backlog={}",
            segment_id,
            worker,
            audio_ms,
            crate::overlay::realtime_webview::state::REALTIME_S2S_AUDIO_BACKLOG
                .load(Ordering::Relaxed)
        );
        crate::overlay::realtime_webview::state::REALTIME_S2S_AUDIO_BACKLOG
            .fetch_add(1, Ordering::Relaxed);
        crate::overlay::realtime_webview::state::REALTIME_S2S_AUDIO_BACKLOG_MS
            .fetch_add(audio_ms as u32, Ordering::Relaxed);
    }
}

fn session_worker(
    session_index: usize,
    segment_rx: mpsc::Receiver<Segment>,
    event_tx: mpsc::Sender<S2sEvent>,
    stop_signal: Arc<AtomicBool>,
    settings: S2sSettings,
    context_memory: Arc<Mutex<S2sContextMemory>>,
) {
    let mut generation = 0u64;
    while !stop_signal.load(Ordering::SeqCst) {
        match segment_rx.recv_timeout(Duration::from_millis(50)) {
            Ok(segment) => {
                let segment_id = segment.id;
                generation += 1;
                if let Err(err) = run_single_segment_session(
                    session_index,
                    generation,
                    segment,
                    &event_tx,
                    stop_signal.clone(),
                    &settings,
                    context_memory.clone(),
                ) {
                    eprintln!(
                        "[RealtimeS2S] session={session_index} segment={segment_id} error: {err}"
                    );
                    let _ = event_tx.send(S2sEvent::Error {
                        id: segment_id,
                        message: err.to_string(),
                    });
                    std::thread::sleep(Duration::from_millis(250));
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

fn run_single_segment_session(
    session_index: usize,
    generation: u64,
    segment: Segment,
    event_tx: &mpsc::Sender<S2sEvent>,
    stop_signal: Arc<AtomicBool>,
    settings: &S2sSettings,
    context_memory: Arc<Mutex<S2sContextMemory>>,
) -> Result<()> {
    let context = context_memory
        .lock()
        .map(|memory| memory.snapshot())
        .unwrap_or_else(|_| S2sContextSnapshot {
            text: String::new(),
            segment_count: 0,
            char_count: 0,
        });
    let mut socket =
        open_fresh_socket_session(session_index, generation, settings, &context, &stop_signal)?;
    let outcome = process_segment(
        session_index,
        generation,
        &mut socket,
        &segment,
        event_tx,
        &stop_signal,
        false,
    )?;
    let _ = socket.close(None);
    if outcome == SegmentOutcome::RetryFresh && !stop_signal.load(Ordering::SeqCst) {
        let retry_generation = generation + 1_000_000;
        eprintln!(
            "[RealtimeS2S] retry segment={} session={} gen={} fresh_gen={}",
            segment.id, session_index, generation, retry_generation
        );
        let mut retry_socket = open_fresh_socket_session(
            session_index,
            retry_generation,
            settings,
            &context,
            &stop_signal,
        )?;
        let retry_outcome = process_segment(
            session_index,
            retry_generation,
            &mut retry_socket,
            &segment,
            event_tx,
            &stop_signal,
            true,
        )?;
        let _ = retry_socket.close(None);
        if retry_outcome == SegmentOutcome::RetryFresh {
            let _ = event_tx.send(S2sEvent::Done { id: segment.id });
        }
    }
    Ok(())
}

fn open_fresh_socket_session(
    session_index: usize,
    generation: u64,
    settings: &S2sSettings,
    context: &S2sContextSnapshot,
    stop_signal: &Arc<AtomicBool>,
) -> Result<tungstenite::WebSocket<native_tls::TlsStream<std::net::TcpStream>>> {
    let setup_started = Instant::now();
    let mut socket = connect_websocket(&settings.api_key)?;
    send_s2s_setup(&mut socket, settings, context)?;
    set_socket_short_timeout(&mut socket)?;
    wait_for_setup(&mut socket, stop_signal.clone())?;
    set_socket_nonblocking(&mut socket)?;
    eprintln!(
        "[RealtimeS2S] session={session_index} ready gen={} setup_ms={} model={} voice={} speed={} target={} custom_len={} context_segments={} context_chars={} playback_speed={}",
        generation,
        setup_started.elapsed().as_millis(),
        settings.model,
        settings.voice,
        settings.speed,
        settings.target_language,
        settings.custom_instruction.len(),
        context.segment_count,
        context.char_count,
        crate::overlay::realtime_webview::state::REALTIME_TTS_SPEED.load(Ordering::Relaxed)
    );
    Ok(socket)
}

fn send_s2s_setup(
    socket: &mut tungstenite::WebSocket<native_tls::TlsStream<std::net::TcpStream>>,
    settings: &S2sSettings,
    context: &S2sContextSnapshot,
) -> Result<()> {
    let instruction = format!(
        "You are a low-latency live interpreter. Translate every input speech segment into {}. \
         Speak only the translated content. Do not explain, summarize, answer, or add commentary. \
         Keep the output natural and concise, preserving names and technical terms. {}{}{}",
        settings.target_language,
        speed_instruction(&settings.speed),
        if settings.custom_instruction.trim().is_empty() {
            String::new()
        } else {
            format!(
                " Additional speaking instructions: {}",
                settings.custom_instruction.trim()
            )
        },
        context.text
    );
    let payload = serde_json::json!({
        "setup": {
            "model": format!("models/{}", settings.model),
            "generationConfig": {
                "responseModalities": ["AUDIO"],
                "mediaResolution": "MEDIA_RESOLUTION_LOW",
                "thinkingConfig": { "thinkingBudget": 0 },
                "speechConfig": {
                    "voiceConfig": {
                        "prebuiltVoiceConfig": {
                            "voiceName": settings.voice
                        }
                    }
                }
            },
            "systemInstruction": {
                "parts": [{ "text": instruction }]
            },
            "contextWindowCompression": {
                "slidingWindow": {}
            },
            "inputAudioTranscription": {},
            "outputAudioTranscription": {}
        }
    });

    socket.write(Message::Text(payload.to_string().into()))?;
    socket.flush()?;
    Ok(())
}

fn wait_for_setup(
    socket: &mut tungstenite::WebSocket<native_tls::TlsStream<std::net::TcpStream>>,
    stop_signal: Arc<AtomicBool>,
) -> Result<()> {
    let started = Instant::now();
    while !stop_signal.load(Ordering::SeqCst) {
        match socket.read() {
            Ok(Message::Text(msg)) => {
                let update = parse_s2s_update(msg.as_str());
                if let Some(error) = update.error {
                    return Err(anyhow::anyhow!(error));
                }
                if update.setup_complete {
                    return Ok(());
                }
            }
            Ok(Message::Binary(data)) => {
                if let Ok(text) = String::from_utf8(data.to_vec()) {
                    let update = parse_s2s_update(&text);
                    if let Some(error) = update.error {
                        return Err(anyhow::anyhow!(error));
                    }
                    if update.setup_complete {
                        return Ok(());
                    }
                }
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(ref err))
                if err.kind() == std::io::ErrorKind::WouldBlock
                    || err.kind() == std::io::ErrorKind::TimedOut =>
            {
                if started.elapsed() > Duration::from_secs(15) {
                    return Err(anyhow::anyhow!("S2S setup timeout"));
                }
                std::thread::sleep(Duration::from_millis(40));
            }
            Err(err) => return Err(err.into()),
        }
    }
    Err(anyhow::anyhow!("stopped"))
}

fn process_segment(
    session_index: usize,
    generation: u64,
    socket: &mut tungstenite::WebSocket<native_tls::TlsStream<std::net::TcpStream>>,
    segment: &Segment,
    event_tx: &mpsc::Sender<S2sEvent>,
    stop_signal: &Arc<AtomicBool>,
    final_attempt: bool,
) -> Result<SegmentOutcome> {
    let segment_id = segment.id;
    let audio_ms = samples_to_ms(segment.samples.len());
    let queued_wait_ms = segment.queued_at.elapsed().as_millis();
    eprintln!(
        "[RealtimeS2S] start segment={} session={} gen={} audio_ms={} queued_wait_ms={}",
        segment_id, session_index, generation, audio_ms, queued_wait_ms
    );
    for chunk in segment.samples.chunks(FRAME_SAMPLES) {
        if stop_signal.load(Ordering::SeqCst) {
            return Ok(SegmentOutcome::RetryFresh);
        }
        send_audio_chunk(socket, chunk)?;
    }
    send_audio_stream_end(socket)?;

    let started = Instant::now();
    let mut last_update = Instant::now();
    let mut last_audio_at: Option<Instant> = None;
    let mut first_audio_ms: Option<u128> = None;
    let mut audio_chunks = 0usize;
    while !stop_signal.load(Ordering::SeqCst) {
        match socket.read() {
            Ok(Message::Text(msg)) => {
                let new_chunks = handle_s2s_message(segment_id, msg.as_str(), event_tx)?;
                if new_chunks > 0 && first_audio_ms.is_none() {
                    last_audio_at = Some(Instant::now());
                    first_audio_ms = Some(started.elapsed().as_millis());
                    eprintln!(
                        "[RealtimeS2S] first-audio segment={} session={} gen={} elapsed_ms={}",
                        segment_id,
                        session_index,
                        generation,
                        first_audio_ms.unwrap()
                    );
                }
                if new_chunks > 0 {
                    last_audio_at = Some(Instant::now());
                }
                audio_chunks += new_chunks;
                last_update = Instant::now();
                if parse_s2s_update(msg.as_str()).turn_complete {
                    eprintln!(
                        "[RealtimeS2S] done segment={} session={} gen={} elapsed_ms={} reason=turn_complete chunks={} first_audio_ms={}",
                        segment_id,
                        session_index,
                        generation,
                        started.elapsed().as_millis(),
                        audio_chunks,
                        first_audio_ms
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "none".to_string())
                    );
                    if audio_chunks == 0 && !final_attempt {
                        return Ok(SegmentOutcome::RetryFresh);
                    }
                    let _ = event_tx.send(S2sEvent::Done { id: segment_id });
                    return Ok(if audio_chunks > 0 {
                        SegmentOutcome::Healthy
                    } else {
                        SegmentOutcome::RetryFresh
                    });
                }
            }
            Ok(Message::Binary(data)) => {
                if let Ok(text) = String::from_utf8(data.to_vec()) {
                    let new_chunks = handle_s2s_message(segment_id, &text, event_tx)?;
                    if new_chunks > 0 && first_audio_ms.is_none() {
                        last_audio_at = Some(Instant::now());
                        first_audio_ms = Some(started.elapsed().as_millis());
                        eprintln!(
                            "[RealtimeS2S] first-audio segment={} session={} gen={} elapsed_ms={}",
                            segment_id,
                            session_index,
                            generation,
                            first_audio_ms.unwrap()
                        );
                    }
                    if new_chunks > 0 {
                        last_audio_at = Some(Instant::now());
                    }
                    audio_chunks += new_chunks;
                    last_update = Instant::now();
                    if parse_s2s_update(&text).turn_complete {
                        eprintln!(
                            "[RealtimeS2S] done segment={} session={} gen={} elapsed_ms={} reason=turn_complete chunks={} first_audio_ms={}",
                            segment_id,
                            session_index,
                            generation,
                            started.elapsed().as_millis(),
                            audio_chunks,
                            first_audio_ms
                                .map(|value| value.to_string())
                                .unwrap_or_else(|| "none".to_string())
                        );
                        if audio_chunks == 0 && !final_attempt {
                            return Ok(SegmentOutcome::RetryFresh);
                        }
                        let _ = event_tx.send(S2sEvent::Done { id: segment_id });
                        return Ok(if audio_chunks > 0 {
                            SegmentOutcome::Healthy
                        } else {
                            SegmentOutcome::RetryFresh
                        });
                    }
                }
            }
            Ok(Message::Close(frame)) => {
                let detail = frame
                    .map(|f| format!("connection closed ({}: {})", f.code, f.reason))
                    .unwrap_or_else(|| "connection closed".to_string());
                return Err(anyhow::anyhow!(detail));
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(ref err))
                if err.kind() == std::io::ErrorKind::WouldBlock
                    || err.kind() == std::io::ErrorKind::TimedOut =>
            {
                if audio_chunks > 0
                    && last_audio_at
                        .map(|last| last.elapsed().as_millis() >= AUDIO_IDLE_FINISH_MS)
                        .unwrap_or(false)
                {
                    let _ = event_tx.send(S2sEvent::Done { id: segment_id });
                    eprintln!(
                        "[RealtimeS2S] done segment={} session={} gen={} elapsed_ms={} reason=audio_idle idle_ms={} chunks={} first_audio_ms={}",
                        segment_id,
                        session_index,
                        generation,
                        started.elapsed().as_millis(),
                        last_audio_at
                            .map(|last| last.elapsed().as_millis())
                            .unwrap_or_default(),
                        audio_chunks,
                        first_audio_ms
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "none".to_string())
                    );
                    return Ok(SegmentOutcome::Healthy);
                }
                if audio_chunks == 0 && started.elapsed().as_millis() >= FIRST_AUDIO_RETRY_MS {
                    eprintln!(
                        "[RealtimeS2S] done segment={} session={} gen={} elapsed_ms={} reason=no_first_audio_retry chunks=0 first_audio_ms=none",
                        segment_id,
                        session_index,
                        generation,
                        started.elapsed().as_millis()
                    );
                    if final_attempt {
                        let _ = event_tx.send(S2sEvent::Done { id: segment_id });
                    }
                    return Ok(SegmentOutcome::RetryFresh);
                }
                if last_update.elapsed() > Duration::from_secs(8)
                    || started.elapsed() > Duration::from_secs(30)
                {
                    eprintln!(
                        "[RealtimeS2S] done segment={} session={} gen={} elapsed_ms={} reason=timeout idle_ms={} chunks={} first_audio_ms={}",
                        segment_id,
                        session_index,
                        generation,
                        started.elapsed().as_millis(),
                        last_update.elapsed().as_millis(),
                        audio_chunks,
                        first_audio_ms
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "none".to_string())
                    );
                    if audio_chunks == 0 && !final_attempt {
                        return Ok(SegmentOutcome::RetryFresh);
                    }
                    let _ = event_tx.send(S2sEvent::Done { id: segment_id });
                    return Ok(SegmentOutcome::Healthy);
                }
                std::thread::sleep(Duration::from_millis(15));
            }
            Err(err) => return Err(err.into()),
        }
    }
    Ok(SegmentOutcome::RetryFresh)
}

fn handle_s2s_message(id: u64, message: &str, event_tx: &mpsc::Sender<S2sEvent>) -> Result<usize> {
    let update = parse_s2s_update(message);
    if let Some(error) = update.error {
        let _ = event_tx.send(S2sEvent::Error { id, message: error });
        return Ok(0);
    }
    if update.interrupted {
        let _ = event_tx.send(S2sEvent::Interrupt);
    }
    if let Some(text) = update.input_transcript {
        let _ = event_tx.send(S2sEvent::InputText { id, text });
    }
    if let Some(text) = update.output_transcript {
        let _ = event_tx.send(S2sEvent::OutputText { id, text });
    }
    let chunk_count = update.audio_chunks.len();
    for bytes in update.audio_chunks {
        let _ = event_tx.send(S2sEvent::Audio { id, bytes });
    }
    Ok(chunk_count)
}

struct S2sParsedUpdate {
    setup_complete: bool,
    input_transcript: Option<String>,
    output_transcript: Option<String>,
    audio_chunks: Vec<Vec<u8>>,
    turn_complete: bool,
    interrupted: bool,
    error: Option<String>,
}

fn parse_s2s_update(message: &str) -> S2sParsedUpdate {
    let mut update = S2sParsedUpdate {
        setup_complete: message.contains("setupComplete"),
        input_transcript: None,
        output_transcript: None,
        audio_chunks: Vec::new(),
        turn_complete: false,
        interrupted: false,
        error: None,
    };

    let Ok(json) = serde_json::from_str::<serde_json::Value>(message) else {
        return update;
    };

    if let Some(error) = json.get("error") {
        update.error = error
            .get("message")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned)
            .or_else(|| Some(error.to_string()));
        return update;
    }

    let Some(server_content) = json.get("serverContent") else {
        return update;
    };

    update.turn_complete = server_content
        .get("turnComplete")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
        || server_content
            .get("generationComplete")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
    update.interrupted = server_content
        .get("interrupted")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    update.input_transcript = server_content
        .get("inputTranscription")
        .and_then(|value| value.get("text"))
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    update.output_transcript = server_content
        .get("outputTranscription")
        .and_then(|value| value.get("text"))
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if let Some(parts) = server_content
        .get("modelTurn")
        .and_then(|value| value.get("parts"))
        .and_then(|value| value.as_array())
    {
        for part in parts {
            if let Some(inline) = part.get("inlineData")
                && let Some(data) = inline.get("data").and_then(|value| value.as_str())
                && let Ok(bytes) = general_purpose::STANDARD.decode(data)
            {
                update.audio_chunks.push(bytes);
            }
        }
    }

    update
}

struct SegmentPlayback {
    chunks: VecDeque<Vec<u8>>,
    chunk_count: usize,
    byte_count: usize,
    source_audio_ms: usize,
    queued_at: Option<Instant>,
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
            done: false,
            error: false,
        }
    }
}

fn coordinate_output(
    event_rx: mpsc::Receiver<S2sEvent>,
    stop_signal: Arc<AtomicBool>,
    overlay_hwnd: HWND,
    translation_hwnd: Option<HWND>,
    state: SharedRealtimeState,
    context_memory: Arc<Mutex<S2sContextMemory>>,
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
                    let segment = segments.entry(id).or_insert_with(SegmentPlayback::new);
                    segment.source_audio_ms = audio_ms;
                    segment.queued_at = Some(queued_at);
                }
                S2sEvent::InputText { id, text } => {
                    inputs.insert(id, text);
                    publish_text(&state, overlay_hwnd, translation_hwnd, &inputs, &outputs);
                }
                S2sEvent::OutputText { id, text } => {
                    merge_segment_text(outputs.entry(id).or_default(), &text);
                    publish_text(&state, overlay_hwnd, translation_hwnd, &inputs, &outputs);
                }
                S2sEvent::Audio { id, bytes } => {
                    let segment = segments.entry(id).or_insert_with(SegmentPlayback::new);
                    if segment.chunk_count == 0 {
                        eprintln!(
                            "[RealtimeS2S] audio-ready segment={} bytes={}",
                            id,
                            bytes.len()
                        );
                    }
                    segment.chunk_count += 1;
                    segment.byte_count += bytes.len();
                    segment.chunks.push_back(bytes);
                }
                S2sEvent::Done { id } => {
                    segments.entry(id).or_insert_with(SegmentPlayback::new).done = true;
                    push_completed_context(id, &inputs, &outputs, &context_memory);
                }
                S2sEvent::Error { id, message } => {
                    eprintln!("[RealtimeS2S] segment={id} error: {message}");
                    let segment = segments.entry(id).or_insert_with(SegmentPlayback::new);
                    segment.error = true;
                    segment.done = true;
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
) {
    loop {
        let Some(segment) = segments.get_mut(next_play_id) else {
            return;
        };
        crate::overlay::realtime_webview::state::REALTIME_S2S_AUDIO_DELAY_MS
            .store(segment_delay_ms(segment) as u32, Ordering::Relaxed);
        if playback.is_none() && segment.chunks.is_empty() && (segment.done || segment.error) {
            let reason = if segment.error { "error" } else { "empty" };
            eprintln!(
                "[RealtimeS2S] skip-play segment={} reason={reason} delay_ms={} backlog_ms={}",
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
            eprintln!(
                "[RealtimeS2S] play-start segment={} chunks={} bytes={} delay_ms={} backlog_ms={} speed={}",
                next_play_id,
                segment.chunk_count,
                segment.byte_count,
                delay_ms,
                s2s_backlog_ms(),
                crate::overlay::realtime_webview::state::CURRENT_TTS_SPEED.load(Ordering::Relaxed)
            );
            *playback = Some(RealtimePlaybackBridge::new(hwnd.0 as isize));
        }
        if let Some(player) = playback.as_ref() {
            while let Some(bytes) = segment.chunks.pop_front() {
                player.push(bytes);
            }
        }
        if segment.done && segment.chunks.is_empty() {
            let delay_ms = segment_delay_ms(segment);
            let chunk_count = segment.chunk_count;
            let byte_count = segment.byte_count;
            let source_audio_ms = segment.source_audio_ms;
            let next_id = *next_play_id + 1;
            let _ = segment;
            let next_ready = segments
                .get(&next_id)
                .is_some_and(|next| !next.chunks.is_empty());
            eprintln!(
                "[RealtimeS2S] play-end segment={} chunks={} bytes={} delay_ms={} backlog_ms={} next_ready={}",
                next_play_id,
                chunk_count,
                byte_count,
                delay_ms,
                s2s_backlog_ms(),
                next_ready
            );
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
    let source_history = join_ordered(inputs);
    let target_history = join_ordered(outputs);
    let source_recent = join_recent_ordered(inputs, DISPLAY_SEGMENT_LIMIT);
    let target_recent = join_recent_ordered(outputs, DISPLAY_SEGMENT_LIMIT);
    let (source_display, target_display) = if let Ok(mut s) = state.lock() {
        s.full_transcript = source_history;
        s.display_transcript = if s.frozen_prefix.is_empty() {
            source_recent
        } else if source_recent.is_empty() {
            s.frozen_prefix.clone()
        } else {
            format!("{}\n\n{}", s.frozen_prefix, source_recent)
        };
        s.committed_translation = target_history;
        s.uncommitted_translation.clear();
        s.display_translation = target_recent;
        (s.display_transcript.clone(), s.display_translation.clone())
    } else {
        (String::new(), String::new())
    };
    update_overlay_text(overlay_hwnd, &source_display);
    if let Some(hwnd) = translation_hwnd {
        update_translation_text(hwnd, &target_display);
    }
}

fn join_ordered(items: &BTreeMap<u64, String>) -> String {
    items
        .values()
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.trim())
        .collect::<Vec<_>>()
        .join(" ")
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

fn merge_segment_text(existing: &mut String, incoming: &str) {
    let incoming = incoming.trim();
    if incoming.is_empty() {
        return;
    }
    if existing.trim().is_empty() || incoming.starts_with(existing.trim()) {
        existing.clear();
        existing.push_str(incoming);
        return;
    }
    if existing.trim_end().ends_with(incoming) {
        return;
    }

    let overlap = largest_suffix_prefix_overlap(existing.trim_end(), incoming);
    if overlap > 0 {
        existing.push_str(&incoming[overlap..]);
        return;
    }

    let needs_space = existing
        .chars()
        .last()
        .is_some_and(|ch| !ch.is_whitespace())
        && incoming.chars().next().is_some_and(|ch| {
            !ch.is_whitespace() && !matches!(ch, '.' | ',' | '?' | '!' | ';' | ':')
        });
    if needs_space {
        existing.push(' ');
    }
    existing.push_str(incoming);
}

fn largest_suffix_prefix_overlap(existing: &str, incoming: &str) -> usize {
    let max = existing.len().min(incoming.len());
    incoming
        .char_indices()
        .map(|(idx, _)| idx)
        .chain(std::iter::once(incoming.len()))
        .filter(|&len| len > 0 && len <= max && existing.ends_with(&incoming[..len]))
        .max()
        .unwrap_or(0)
}

fn join_recent_ordered(items: &BTreeMap<u64, String>, limit: usize) -> String {
    let mut recent = items
        .iter()
        .rev()
        .filter_map(|(_, value)| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        })
        .take(limit)
        .collect::<Vec<_>>();
    recent.reverse();
    recent.join(" ")
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

fn calculate_rms(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum = samples
        .iter()
        .map(|sample| {
            let normalized = *sample as f32 / i16::MAX as f32;
            normalized * normalized
        })
        .sum::<f32>();
    (sum / samples.len() as f32).sqrt()
}

fn samples_to_ms(samples: usize) -> usize {
    samples.saturating_mul(1000) / 16_000
}

fn speed_instruction(speed: &str) -> &'static str {
    match speed {
        "Slow" => "Speak slowly, clearly, and with deliberate pacing.",
        "Fast" => "Speak quickly, efficiently, and with a brisk pace.",
        _ => "Speak naturally and clearly.",
    }
}

fn tts_instruction_for_target(
    target_language: &str,
    conditions: &[crate::config::TtsLanguageCondition],
) -> String {
    let target_code = language_to_639_3(target_language);
    conditions
        .iter()
        .find(|condition| {
            condition.language_code.eq_ignore_ascii_case(&target_code)
                || condition
                    .language_name
                    .eq_ignore_ascii_case(target_language.trim())
        })
        .map(|condition| condition.instruction.trim().to_string())
        .filter(|instruction| !instruction.is_empty())
        .unwrap_or_default()
}

fn language_to_639_3(language: &str) -> String {
    let language = language.trim();
    if language.len() == 3 && isolang::Language::from_639_3(language).is_some() {
        return language.to_ascii_lowercase();
    }
    if language.len() == 2
        && let Some(lang) = isolang::Language::from_639_1(language)
    {
        return lang.to_639_3().to_string();
    }
    isolang::Language::from_name(language)
        .map(|lang| lang.to_639_3())
        .map(|code| code.to_string())
        .unwrap_or_else(|| language.to_ascii_lowercase())
}

fn segment_delay_ms(segment: &SegmentPlayback) -> u128 {
    segment
        .queued_at
        .map(|queued_at| queued_at.elapsed().as_millis())
        .unwrap_or_default()
}

fn s2s_backlog_ms() -> u32 {
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

fn truncate_chars(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let mut output = value
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect::<String>();
    output.push_str("...");
    output
}

fn is_stale_session(session_id: u64) -> bool {
    crate::overlay::realtime_webview::state::REALTIME_SESSION_ID.load(Ordering::SeqCst)
        != session_id
}
