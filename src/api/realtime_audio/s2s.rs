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
const FIRST_AUDIO_SILENT_RETRY_MS: u128 = 3_800;
const FIRST_AUDIO_ACTIVE_RETRY_MS: u128 = 5_200;
const S2S_HEDGE_TIMEOUT_MS: u128 = 45_000;
const S2S_HEDGE_FINAL_TIMEOUT_MS: u128 = 60_000;
const S2S_ORDERED_PENDING_SKIP_MS: u128 = 8_000;
const S2S_ORDERED_TRANSCRIPT_PENDING_SKIP_MS: u128 = 28_000;
const CONTEXT_SEGMENT_LIMIT: usize = 5;
const CONTEXT_LINE_CHAR_LIMIT: usize = 240;
const CONTEXT_TOTAL_CHAR_LIMIT: usize = 1_500;
const SPEECH_THRESHOLD_MULTIPLIER: f32 = 2.2;
const MIN_SPEECH_THRESHOLD: f32 = 0.012;
const MAX_SPEECH_THRESHOLD: f32 = 0.035;
const ABSOLUTE_SPEECH_RMS: f32 = 0.045;
const NOISE_LEARN_MAX_RMS: f32 = 0.018;
const NOISE_LEARN_THRESHOLD_RATIO: f32 = 0.60;
const MIN_SEGMENT_SPEECH_FRAMES: usize = 4;
const MIN_SEGMENT_PEAK_RMS: f32 = 0.025;
const MIN_SEGMENT_SPEECH_RATIO: f32 = 0.08;
const MIN_SPEECH_LIKE_RATIO: f32 = 0.18;
const STRICT_MIN_SPEECH_LIKE_RATIO: f32 = 0.32;
const STRICT_MIN_SPEECH_CONFIDENCE: f32 = 0.38;
const MIN_TEXT_OVERLAP_CHARS: usize = 3;
const S2S_BATCH_SEGMENT_ATTEMPTS: usize = 4;

fn grouped_first_audio_timeout_ms(source_audio_ms: u128, text_updates: usize) -> u128 {
    let base = if text_updates == 0 {
        FIRST_AUDIO_SILENT_RETRY_MS
    } else {
        FIRST_AUDIO_ACTIVE_RETRY_MS
    };
    (base + source_audio_ms.saturating_mul(2)).clamp(5_500, 30_000)
}

fn grouped_hard_timeout_ms(source_audio_ms: u128, final_attempt: bool) -> u128 {
    let base = if final_attempt {
        S2S_HEDGE_FINAL_TIMEOUT_MS
    } else {
        S2S_HEDGE_TIMEOUT_MS
    };
    (base + source_audio_ms.saturating_mul(4)).min(180_000)
}

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

#[derive(Clone)]
struct S2sContextSnapshot {
    text: String,
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
        S2sContextSnapshot { text }
    }
}

#[derive(Clone)]
struct Segment {
    id: u64,
    samples: Vec<i16>,
    speech_frames: usize,
    peak_rms: f32,
    mean_rms: f32,
    energetic_frames: usize,
    speech_like_frames: usize,
}

impl Segment {
    fn new(id: u64, samples: Vec<i16>, speech_frames: usize, peak_rms: f32) -> Self {
        let metrics = analyze_segment_samples(&samples);
        Self {
            id,
            samples,
            speech_frames,
            peak_rms: peak_rms.max(metrics.peak_rms),
            mean_rms: metrics.mean_rms,
            energetic_frames: metrics.energetic_frames,
            speech_like_frames: metrics.speech_like_frames,
        }
    }
}

fn segment_audio_ms(segment: &Segment) -> usize {
    samples_to_ms(segment.samples.len())
}

fn segment_peak_sample(segment: &Segment) -> f32 {
    segment
        .samples
        .iter()
        .map(|sample| (*sample as f32).abs() / i16::MAX as f32)
        .fold(0.0, f32::max)
}

#[derive(Clone, Copy, Default)]
struct SegmentSampleMetrics {
    mean_rms: f32,
    peak_rms: f32,
    energetic_frames: usize,
    speech_like_frames: usize,
}

#[derive(Clone, Copy, Default)]
struct AdaptiveS2sVadSnapshot {
    strictness: f32,
}

#[derive(Default)]
struct AdaptiveS2sVadState {
    strictness: f32,
    consecutive_empty_no_input: usize,
    last_logged_bucket: i32,
}

impl AdaptiveS2sVadState {
    fn snapshot(&self) -> AdaptiveS2sVadSnapshot {
        let backlog_pressure = (s2s_backlog_ms() as f32 / 30_000.0).clamp(0.0, 0.55);
        AdaptiveS2sVadSnapshot {
            strictness: self.strictness.max(backlog_pressure),
        }
    }

    fn observe(&mut self, outcome: SegmentOutcome, segment: &Segment) {
        match outcome {
            SegmentOutcome::Healthy => {
                self.consecutive_empty_no_input = 0;
                self.strictness = (self.strictness - 0.10).max(0.0);
            }
            SegmentOutcome::EmptyNoInput => {
                self.consecutive_empty_no_input += 1;
                let high_energy = segment.mean_rms >= 0.025
                    || segment.peak_rms >= 0.060
                    || segment_speech_ratio(segment) >= 0.60;
                let step = if high_energy { 0.22 } else { 0.12 };
                self.strictness = (self.strictness + step).min(1.0);
            }
            SegmentOutcome::RetryFresh => {}
        }
        self.log_if_changed(outcome, segment);
    }

    fn log_if_changed(&mut self, outcome: SegmentOutcome, segment: &Segment) {
        let bucket = (self.strictness * 4.0).round() as i32;
        let should_log = bucket != self.last_logged_bucket
            || matches!(outcome, SegmentOutcome::EmptyNoInput)
                && self.consecutive_empty_no_input <= 3;
        if !should_log {
            return;
        }
        self.last_logged_bucket = bucket;
        eprintln!(
            "[RealtimeS2S][AdaptiveVAD] outcome={:?} strictness={:.2} consecutive_empty={} segment={} confidence={:.2} speech_like_ratio={:.2} speech_ratio={:.2} mean_rms={:.4} peak_rms={:.4}",
            outcome,
            self.strictness,
            self.consecutive_empty_no_input,
            segment.id,
            segment_speech_confidence(segment),
            segment_speech_like_ratio(segment),
            segment_speech_ratio(segment),
            segment.mean_rms,
            segment.peak_rms
        );
    }
}

#[derive(Clone, Debug)]
pub struct S2sBatchSettings {
    pub model: String,
    pub voice: String,
    pub speed: String,
    pub target_language: String,
    pub custom_instruction: String,
    pub parallel_requests: usize,
    pub vad_group_budget: usize,
}

#[derive(Clone, Debug)]
pub struct S2sBatchSegment {
    pub id: u64,
    pub source_start_sec: f64,
    pub source_end_sec: f64,
    pub source_text: String,
    pub target_text: String,
    pub audio_pcm_24k: Vec<i16>,
}

#[derive(Clone)]
struct TimedSegment {
    segment: Segment,
    start_sample: usize,
    end_sample: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SegmentOutcome {
    Healthy,
    RetryFresh,
    EmptyNoInput,
}

#[derive(Clone)]
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

enum S2sRaceEvent {
    Event {
        attempt: usize,
        event: S2sEvent,
    },
    Finished {
        attempt: usize,
        outcome: SegmentOutcome,
    },
    Error {
        attempt: usize,
        message: String,
    },
}

fn s2s_event_counts(events: &[S2sEvent]) -> (usize, usize, usize) {
    let mut input_text = 0usize;
    let mut output_text = 0usize;
    let mut audio = 0usize;
    for event in events {
        match event {
            S2sEvent::InputText { .. } => input_text += 1,
            S2sEvent::OutputText { .. } => output_text += 1,
            S2sEvent::Audio { .. } => audio += 1,
            _ => {}
        }
    }
    (input_text, output_text, audio)
}

fn s2s_attempt_counts(events: &[Vec<S2sEvent>]) -> (usize, usize, usize) {
    events
        .iter()
        .map(|buffered| s2s_event_counts(buffered))
        .fold((0usize, 0usize, 0usize), |acc, counts| {
            (acc.0 + counts.0, acc.1 + counts.1, acc.2 + counts.2)
        })
}

fn format_s2s_attempt_counts(events: &[Vec<S2sEvent>]) -> String {
    events
        .iter()
        .enumerate()
        .map(|(attempt, buffered)| {
            let (input_text, output_text, audio) = s2s_event_counts(buffered);
            format!("{attempt}:in={input_text},out={output_text},audio={audio}")
        })
        .collect::<Vec<_>>()
        .join(";")
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
    let mut per_app_capture_stop: Option<Arc<AtomicBool>> = None;
    let mut per_app_initial_pid: Option<u32> = None;
    let _stream = if preset.audio_source == "device" {
        let selected_pid = if selected_pid == 0 {
            crate::overlay::realtime_webview::app_selection::show_audio_app_selector_overlay();
            wait_for_selected_app(stop_signal.clone(), session_id)
        } else {
            Some(selected_pid)
        };
        if let Some(selected_pid) = selected_pid {
            per_app_initial_pid = Some(selected_pid);
            #[cfg(target_os = "windows")]
            {
                let capture_stop = Arc::new(AtomicBool::new(false));
                per_app_capture_stop = Some(capture_stop.clone());
                start_per_app_capture(
                    selected_pid,
                    audio_buffer.clone(),
                    capture_stop,
                    pause.clone(),
                )?;
            }
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
    if let (Some(capture_stop), Some(initial_pid)) =
        (per_app_capture_stop.clone(), per_app_initial_pid)
    {
        spawn_s2s_per_app_audio_pid_refresh(
            initial_pid,
            capture_stop,
            audio_buffer.clone(),
            stop_signal.clone(),
            pause.clone(),
            session_id,
        );
    }

    if let Ok(mut s) = state.lock() {
        s.set_transcription_method(super::state::TranscriptionMethod::GeminiLiveS2s);
    }

    let (event_tx, event_rx) = mpsc::channel::<S2sEvent>();
    let context_memory = Arc::new(Mutex::new(S2sContextMemory::default()));
    let adaptive_vad = Arc::new(Mutex::new(AdaptiveS2sVadState::default()));
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
        let worker_adaptive_vad = adaptive_vad.clone();
        std::thread::spawn(move || {
            session_worker(
                session_index,
                segment_rx,
                worker_events,
                worker_stop,
                worker_settings,
                worker_context,
                worker_adaptive_vad,
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
        adaptive_vad,
    );

    Ok(())
}

fn spawn_s2s_per_app_audio_pid_refresh(
    initial_pid: u32,
    capture_stop: Arc<AtomicBool>,
    audio_buffer: Arc<Mutex<Vec<i16>>>,
    stop_signal: Arc<AtomicBool>,
    pause: Arc<AtomicBool>,
    session_id: u64,
) {
    std::thread::spawn(move || {
        let started = Instant::now();
        let mut last_observed_samples = 0usize;
        while !stop_signal.load(Ordering::Relaxed)
            && !is_stale_session(session_id)
            && started.elapsed() < Duration::from_secs(10)
        {
            std::thread::sleep(Duration::from_millis(500));
            let observed_samples = audio_buffer.lock().map(|buffer| buffer.len()).unwrap_or(0);
            if observed_samples > last_observed_samples + FRAME_SAMPLES {
                return;
            }
            last_observed_samples = observed_samples;

            let Some(refreshed_pid) =
                crate::overlay::realtime_webview::app_selection::refresh_selected_audio_capture_pid(
                )
            else {
                continue;
            };
            if refreshed_pid == 0 || refreshed_pid == initial_pid {
                continue;
            }

            crate::log_info!(
                "[RealtimeS2S] restart per-app capture initial_pid={} refreshed_pid={} elapsed_ms={}",
                initial_pid,
                refreshed_pid,
                started.elapsed().as_millis()
            );
            capture_stop.store(true, Ordering::SeqCst);
            SELECTED_APP_PID.store(refreshed_pid, Ordering::SeqCst);
            if let Err(error) = start_per_app_capture(
                refreshed_pid,
                audio_buffer.clone(),
                stop_signal.clone(),
                pause.clone(),
            ) {
                crate::log_info!(
                    "[RealtimeS2S] restart per-app capture failed refreshed_pid={} error={}",
                    refreshed_pid,
                    error
                );
            }
            return;
        }
    });
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
        let result = run_single_segment_session(
            0,
            id + 1,
            timed.segment.clone(),
            &event_tx,
            stop_signal.clone(),
            &settings,
            context_memory.clone(),
            adaptive_vad,
        );
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
            return Err(err);
        }
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
        let result = run_single_segment_session(
            0,
            id + 1 + (attempt as u64 * 10_000_000),
            timed.segment.clone(),
            &event_tx,
            stop_signal.clone(),
            &settings,
            context_memory,
            adaptive_vad,
        );
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

fn run_vad_loop(
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

fn collect_vad_segments(samples: Vec<i16>) -> Vec<TimedSegment> {
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

fn group_timed_segments(
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

fn session_worker(
    session_index: usize,
    segment_rx: mpsc::Receiver<Segment>,
    event_tx: mpsc::Sender<S2sEvent>,
    stop_signal: Arc<AtomicBool>,
    settings: S2sSettings,
    context_memory: Arc<Mutex<S2sContextMemory>>,
    adaptive_vad: Arc<Mutex<AdaptiveS2sVadState>>,
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
                    adaptive_vad.clone(),
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
    adaptive_vad: Arc<Mutex<AdaptiveS2sVadState>>,
) -> Result<()> {
    let context = context_memory
        .lock()
        .map(|memory| memory.snapshot())
        .unwrap_or_else(|_| S2sContextSnapshot {
            text: String::new(),
        });
    let outcome = run_hedged_segment_session(
        session_index,
        generation,
        segment.clone(),
        event_tx.clone(),
        stop_signal.clone(),
        settings.clone(),
        context.clone(),
        false,
    )?;
    observe_adaptive_vad(&adaptive_vad, outcome, &segment);
    if outcome == SegmentOutcome::EmptyNoInput {
        let _ = event_tx.send(S2sEvent::Done { id: segment.id });
    } else if outcome == SegmentOutcome::RetryFresh && !stop_signal.load(Ordering::SeqCst) {
        let retry_generation = generation + 1_000_000;
        eprintln!(
            "[RealtimeS2S] retry segment={} session={} gen={} fresh_gen={}",
            segment.id, session_index, generation, retry_generation
        );
        let retry_outcome = run_hedged_segment_session(
            session_index,
            retry_generation,
            segment.clone(),
            event_tx.clone(),
            stop_signal.clone(),
            settings.clone(),
            context,
            true,
        )?;
        observe_adaptive_vad(&adaptive_vad, retry_outcome, &segment);
        if matches!(
            retry_outcome,
            SegmentOutcome::RetryFresh | SegmentOutcome::EmptyNoInput
        ) {
            let _ = event_tx.send(S2sEvent::Done { id: segment.id });
        }
    }
    Ok(())
}

fn run_hedged_segment_session(
    session_index: usize,
    generation: u64,
    segment: Segment,
    event_tx: mpsc::Sender<S2sEvent>,
    stop_signal: Arc<AtomicBool>,
    settings: S2sSettings,
    context: S2sContextSnapshot,
    final_attempt: bool,
) -> Result<SegmentOutcome> {
    const HEDGE_ATTEMPTS: usize = 2;

    let (race_tx, race_rx) = mpsc::channel::<S2sRaceEvent>();
    let mut cancel_flags = Vec::with_capacity(HEDGE_ATTEMPTS);
    let mut saw_audio = [false; HEDGE_ATTEMPTS];
    let mut saw_input_text = [false; HEDGE_ATTEMPTS];
    let mut saw_output_text = [false; HEDGE_ATTEMPTS];
    let mut finished = [false; HEDGE_ATTEMPTS];
    let mut buffered_events = [Vec::<S2sEvent>::new(), Vec::<S2sEvent>::new()];
    let mut winner: Option<usize> = None;
    let started = Instant::now();
    let source_audio_ms = samples_to_ms(segment.samples.len()) as u128;
    let hard_timeout_ms = grouped_hard_timeout_ms(source_audio_ms, final_attempt);

    for attempt in 0..HEDGE_ATTEMPTS {
        let attempt_generation = generation + (attempt as u64 * 100_000);
        let attempt_cancel = Arc::new(AtomicBool::new(false));
        cancel_flags.push(attempt_cancel.clone());
        spawn_hedged_attempt(
            session_index,
            attempt,
            attempt_generation,
            segment.clone(),
            settings.clone(),
            context.clone(),
            stop_signal.clone(),
            attempt_cancel,
            race_tx.clone(),
            final_attempt,
        );
    }
    drop(race_tx);

    while !stop_signal.load(Ordering::SeqCst) {
        if started.elapsed().as_millis() >= hard_timeout_ms {
            for cancel in &cancel_flags {
                cancel.store(true, Ordering::SeqCst);
            }
            eprintln!(
                "[RealtimeS2S][Segment] timeout id={} session={} gen={} elapsed_ms={} timeout_ms={} audio_ms={} speech_ratio={:.2} peak_rms={:.4} peak_sample={:.4} winner={:?} saw_audio={:?} finished={:?} events={} final_attempt={}",
                segment.id,
                session_index,
                generation,
                started.elapsed().as_millis(),
                hard_timeout_ms,
                segment_audio_ms(&segment),
                segment_speech_ratio(&segment),
                segment.peak_rms,
                segment_peak_sample(&segment),
                winner,
                saw_audio,
                finished,
                format_s2s_attempt_counts(&buffered_events),
                final_attempt
            );
            if let Some(attempt) = winner
                && saw_audio[attempt]
            {
                let _ = event_tx.send(S2sEvent::Done { id: segment.id });
                return Ok(SegmentOutcome::Healthy);
            }
            let (input_text_events, output_text_events, audio_events) =
                s2s_attempt_counts(&buffered_events);
            if input_text_events == 0 && output_text_events == 0 && audio_events == 0 {
                if final_attempt {
                    let _ = event_tx.send(S2sEvent::Done { id: segment.id });
                }
                return Ok(SegmentOutcome::EmptyNoInput);
            }
            if final_attempt {
                let _ = event_tx.send(S2sEvent::Done { id: segment.id });
            }
            return Ok(SegmentOutcome::RetryFresh);
        }
        match race_rx.recv_timeout(Duration::from_millis(50)) {
            Ok(S2sRaceEvent::Event { attempt, event }) => {
                if matches!(event, S2sEvent::Audio { .. }) {
                    saw_audio[attempt] = true;
                } else if matches!(event, S2sEvent::InputText { .. }) {
                    saw_input_text[attempt] = true;
                } else if matches!(event, S2sEvent::OutputText { .. }) {
                    saw_output_text[attempt] = true;
                }

                if matches!(event, S2sEvent::Audio { .. }) {
                    if winner.is_none() {
                        winner = Some(attempt);
                        for (index, cancel) in cancel_flags.iter().enumerate() {
                            if index != attempt {
                                cancel.store(true, Ordering::SeqCst);
                            }
                        }
                        for buffered in buffered_events[attempt].drain(..) {
                            let _ = event_tx.send(buffered);
                        }
                    }
                }

                if winner == Some(attempt) {
                    let done = matches!(event, S2sEvent::Done { .. } | S2sEvent::Error { .. });
                    let _ = event_tx.send(event);
                    if done {
                        return Ok(if saw_audio[attempt] {
                            SegmentOutcome::Healthy
                        } else if !saw_input_text[attempt] && !saw_output_text[attempt] {
                            SegmentOutcome::EmptyNoInput
                        } else {
                            SegmentOutcome::RetryFresh
                        });
                    }
                } else if winner.is_none() {
                    buffered_events[attempt].push(event);
                }
            }
            Ok(S2sRaceEvent::Finished { attempt, outcome }) => {
                finished[attempt] = true;
                if winner == Some(attempt) {
                    continue;
                }
                if winner.is_none() && outcome == SegmentOutcome::Healthy && saw_audio[attempt] {
                    winner = Some(attempt);
                    for (index, cancel) in cancel_flags.iter().enumerate() {
                        if index != attempt {
                            cancel.store(true, Ordering::SeqCst);
                        }
                    }
                    for buffered in buffered_events[attempt].drain(..) {
                        let _ = event_tx.send(buffered);
                    }
                    continue;
                }
                if finished.iter().all(|done| *done) && winner.is_none() {
                    let (input_text_events, output_text_events, audio_events) =
                        s2s_attempt_counts(&buffered_events);
                    eprintln!(
                        "[RealtimeS2S][Segment] empty id={} session={} gen={} elapsed_ms={} attempts={} audio_ms={} speech_ratio={:.2} peak_rms={:.4} peak_sample={:.4} events={} final_attempt={}",
                        segment.id,
                        session_index,
                        generation,
                        started.elapsed().as_millis(),
                        HEDGE_ATTEMPTS,
                        segment_audio_ms(&segment),
                        segment_speech_ratio(&segment),
                        segment.peak_rms,
                        segment_peak_sample(&segment),
                        format_s2s_attempt_counts(&buffered_events),
                        final_attempt
                    );
                    if input_text_events == 0 && output_text_events == 0 && audio_events == 0 {
                        return Ok(SegmentOutcome::EmptyNoInput);
                    }
                    return Ok(SegmentOutcome::RetryFresh);
                }
            }
            Ok(S2sRaceEvent::Error { attempt, message }) => {
                finished[attempt] = true;
                eprintln!(
                    "[RealtimeS2S] hedge-attempt-error segment={} session={} gen={} attempt={} error={}",
                    segment.id, session_index, generation, attempt, message
                );
                if winner == Some(attempt) {
                    let _ = event_tx.send(S2sEvent::Error {
                        id: segment.id,
                        message,
                    });
                    return Ok(SegmentOutcome::RetryFresh);
                }
                if finished.iter().all(|done| *done) && winner.is_none() {
                    return Ok(SegmentOutcome::RetryFresh);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    Ok(SegmentOutcome::RetryFresh)
}

fn spawn_hedged_attempt(
    session_index: usize,
    attempt: usize,
    generation: u64,
    segment: Segment,
    settings: S2sSettings,
    context: S2sContextSnapshot,
    stop_signal: Arc<AtomicBool>,
    cancel_signal: Arc<AtomicBool>,
    race_tx: mpsc::Sender<S2sRaceEvent>,
    final_attempt: bool,
) {
    std::thread::spawn(move || {
        let (attempt_tx, attempt_rx) = mpsc::channel::<S2sEvent>();
        let forward_tx = race_tx.clone();
        let forwarder = std::thread::spawn(move || {
            while let Ok(event) = attempt_rx.recv() {
                if forward_tx
                    .send(S2sRaceEvent::Event { attempt, event })
                    .is_err()
                {
                    break;
                }
            }
        });

        let outcome = match open_fresh_socket_session(
            session_index,
            generation,
            &settings,
            &context,
            &stop_signal,
        ) {
            Ok(mut socket) => {
                let result = process_segment(
                    session_index,
                    generation,
                    &mut socket,
                    &segment,
                    &attempt_tx,
                    &stop_signal,
                    Some(&cancel_signal),
                    final_attempt,
                );
                let _ = socket.close(None);
                result
            }
            Err(err) => Err(err),
        };
        drop(attempt_tx);
        let _ = forwarder.join();

        match outcome {
            Ok(outcome) => {
                let _ = race_tx.send(S2sRaceEvent::Finished { attempt, outcome });
            }
            Err(err) => {
                let _ = race_tx.send(S2sRaceEvent::Error {
                    attempt,
                    message: err.to_string(),
                });
            }
        }
    });
}

fn open_fresh_socket_session(
    _session_index: usize,
    _generation: u64,
    settings: &S2sSettings,
    context: &S2sContextSnapshot,
    stop_signal: &Arc<AtomicBool>,
) -> Result<tungstenite::WebSocket<native_tls::TlsStream<std::net::TcpStream>>> {
    let mut socket = connect_websocket(&settings.api_key)?;
    send_s2s_setup(&mut socket, settings, context)?;
    set_socket_short_timeout(&mut socket)?;
    wait_for_setup(&mut socket, stop_signal.clone())?;
    set_socket_nonblocking(&mut socket)?;
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

fn s2s_should_stop(stop_signal: &Arc<AtomicBool>, cancel_signal: Option<&Arc<AtomicBool>>) -> bool {
    stop_signal.load(Ordering::SeqCst)
        || cancel_signal
            .map(|cancel| cancel.load(Ordering::SeqCst))
            .unwrap_or(false)
}

fn process_segment(
    session_index: usize,
    generation: u64,
    socket: &mut tungstenite::WebSocket<native_tls::TlsStream<std::net::TcpStream>>,
    segment: &Segment,
    event_tx: &mpsc::Sender<S2sEvent>,
    stop_signal: &Arc<AtomicBool>,
    cancel_signal: Option<&Arc<AtomicBool>>,
    final_attempt: bool,
) -> Result<SegmentOutcome> {
    let segment_id = segment.id;
    for chunk in segment.samples.chunks(FRAME_SAMPLES) {
        if s2s_should_stop(stop_signal, cancel_signal) {
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
    let mut text_updates = 0usize;
    while !s2s_should_stop(stop_signal, cancel_signal) {
        match socket.read() {
            Ok(Message::Text(msg)) => {
                let message_update = handle_s2s_message(segment_id, msg.as_str(), event_tx)?;
                let new_chunks = message_update.audio_chunks;
                text_updates += message_update.text_updates;
                if new_chunks > 0 && first_audio_ms.is_none() {
                    last_audio_at = Some(Instant::now());
                    first_audio_ms = Some(started.elapsed().as_millis());
                }
                if new_chunks > 0 {
                    last_audio_at = Some(Instant::now());
                }
                audio_chunks += new_chunks;
                last_update = Instant::now();
                if parse_s2s_update(msg.as_str()).turn_complete {
                    if audio_chunks == 0 && !final_attempt {
                        return Ok(if text_updates == 0 {
                            SegmentOutcome::EmptyNoInput
                        } else {
                            SegmentOutcome::RetryFresh
                        });
                    }
                    let _ = event_tx.send(S2sEvent::Done { id: segment_id });
                    return Ok(if audio_chunks > 0 {
                        SegmentOutcome::Healthy
                    } else if text_updates == 0 {
                        SegmentOutcome::EmptyNoInput
                    } else {
                        SegmentOutcome::RetryFresh
                    });
                }
            }
            Ok(Message::Binary(data)) => {
                if let Ok(text) = String::from_utf8(data.to_vec()) {
                    let message_update = handle_s2s_message(segment_id, &text, event_tx)?;
                    let new_chunks = message_update.audio_chunks;
                    text_updates += message_update.text_updates;
                    if new_chunks > 0 && first_audio_ms.is_none() {
                        last_audio_at = Some(Instant::now());
                        first_audio_ms = Some(started.elapsed().as_millis());
                    }
                    if new_chunks > 0 {
                        last_audio_at = Some(Instant::now());
                    }
                    audio_chunks += new_chunks;
                    last_update = Instant::now();
                    if parse_s2s_update(&text).turn_complete {
                        if audio_chunks == 0 && !final_attempt {
                            return Ok(if text_updates == 0 {
                                SegmentOutcome::EmptyNoInput
                            } else {
                                SegmentOutcome::RetryFresh
                            });
                        }
                        let _ = event_tx.send(S2sEvent::Done { id: segment_id });
                        return Ok(if audio_chunks > 0 {
                            SegmentOutcome::Healthy
                        } else if text_updates == 0 {
                            SegmentOutcome::EmptyNoInput
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
                eprintln!(
                    "[RealtimeS2S] socket-close segment={} session={} gen={} elapsed_ms={} detail={} chunks={} text_updates={}",
                    segment_id,
                    session_index,
                    generation,
                    started.elapsed().as_millis(),
                    detail,
                    audio_chunks,
                    text_updates
                );
                if audio_chunks > 0 {
                    let _ = event_tx.send(S2sEvent::Done { id: segment_id });
                    return Ok(SegmentOutcome::Healthy);
                }
                return Ok(SegmentOutcome::RetryFresh);
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
                    return Ok(SegmentOutcome::Healthy);
                }
                let source_audio_ms = samples_to_ms(segment.samples.len()) as u128;
                let no_first_audio_retry_ms =
                    grouped_first_audio_timeout_ms(source_audio_ms, text_updates);
                if audio_chunks == 0 && started.elapsed().as_millis() >= no_first_audio_retry_ms {
                    if final_attempt {
                        let _ = event_tx.send(S2sEvent::Done { id: segment_id });
                    }
                    return Ok(if text_updates == 0 {
                        SegmentOutcome::EmptyNoInput
                    } else {
                        SegmentOutcome::RetryFresh
                    });
                }
                let total_timeout_ms = no_first_audio_retry_ms.max(30_000);
                if (audio_chunks > 0 && last_update.elapsed() > Duration::from_secs(8))
                    || started.elapsed().as_millis() > total_timeout_ms
                {
                    eprintln!(
                        "[RealtimeS2S] done segment={} session={} gen={} elapsed_ms={} reason=timeout idle_ms={} total_timeout_ms={} source_audio_ms={} chunks={} first_audio_ms={}",
                        segment_id,
                        session_index,
                        generation,
                        started.elapsed().as_millis(),
                        last_update.elapsed().as_millis(),
                        total_timeout_ms,
                        source_audio_ms,
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

struct HandledS2sMessage {
    audio_chunks: usize,
    text_updates: usize,
}

fn handle_s2s_message(
    id: u64,
    message: &str,
    event_tx: &mpsc::Sender<S2sEvent>,
) -> Result<HandledS2sMessage> {
    let update = parse_s2s_update(message);
    if let Some(error) = update.error {
        let _ = event_tx.send(S2sEvent::Error { id, message: error });
        return Ok(HandledS2sMessage {
            audio_chunks: 0,
            text_updates: 0,
        });
    }
    if update.interrupted {
        let _ = event_tx.send(S2sEvent::Interrupt);
    }
    let mut text_updates = 0usize;
    if let Some(text) = update.input_transcript {
        let _ = event_tx.send(S2sEvent::InputText { id, text });
        text_updates += 1;
    }
    if let Some(text) = update.output_transcript {
        let _ = event_tx.send(S2sEvent::OutputText { id, text });
        text_updates += 1;
    }
    let chunk_count = update.audio_chunks.len();
    for bytes in update.audio_chunks {
        let _ = event_tx.send(S2sEvent::Audio { id, bytes });
    }
    Ok(HandledS2sMessage {
        audio_chunks: chunk_count,
        text_updates,
    })
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
        if playback.is_none()
            && segment.chunks.is_empty()
            && !segment.done
            && !segment.error
            && should_skip_stale_pending_segment(segment)
        {
            eprintln!(
                "[RealtimeS2S] skip-play segment={} reason=pending-timeout delay_ms={} source_audio_ms={} input_text={} output_text={} backlog_ms={}",
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
        .filter(|&len| {
            len > 0
                && len <= max
                && existing.ends_with(&incoming[..len])
                && is_meaningful_text_overlap(&incoming[..len])
        })
        .max()
        .unwrap_or(0)
}

fn is_meaningful_text_overlap(overlap: &str) -> bool {
    overlap.chars().any(char::is_whitespace) || overlap.chars().count() >= MIN_TEXT_OVERLAP_CHARS
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

fn speech_threshold_for_noise(noise_floor: f32) -> f32 {
    (noise_floor * SPEECH_THRESHOLD_MULTIPLIER).clamp(MIN_SPEECH_THRESHOLD, MAX_SPEECH_THRESHOLD)
}

fn update_noise_floor(noise_floor: f32, rms: f32, speech_threshold: f32, is_speech: bool) -> f32 {
    if is_speech {
        return noise_floor;
    }

    let learn_limit = NOISE_LEARN_MAX_RMS.min(speech_threshold * NOISE_LEARN_THRESHOLD_RATIO);
    if rms <= learn_limit {
        return (noise_floor * 0.98) + (rms * 0.02);
    }

    noise_floor.min(MAX_SPEECH_THRESHOLD / SPEECH_THRESHOLD_MULTIPLIER) * 0.995
}

fn analyze_segment_samples(samples: &[i16]) -> SegmentSampleMetrics {
    if samples.is_empty() {
        return SegmentSampleMetrics::default();
    }

    let mut rms_sum = 0.0f32;
    let mut frame_count = 0usize;
    let mut energetic_frames = 0usize;
    let mut speech_like_frames = 0usize;
    let mut peak_rms = 0.0f32;
    for frame in samples.chunks(FRAME_SAMPLES) {
        frame_count += 1;
        let rms = calculate_rms(frame);
        peak_rms = peak_rms.max(rms);
        rms_sum += rms;
        if rms >= MIN_SPEECH_THRESHOLD {
            energetic_frames += 1;
        }
        if is_speech_like_frame(frame, rms) {
            speech_like_frames += 1;
        }
    }

    SegmentSampleMetrics {
        mean_rms: rms_sum / frame_count.max(1) as f32,
        peak_rms,
        energetic_frames,
        speech_like_frames,
    }
}

fn is_speech_like_frame(frame: &[i16], rms: f32) -> bool {
    if frame.len() < 2 || rms < MIN_SPEECH_THRESHOLD {
        return false;
    }

    let peak = frame
        .iter()
        .map(|sample| (*sample as f32).abs() / i16::MAX as f32)
        .fold(0.0, f32::max);
    let crest = peak / rms.max(0.000_1);
    let zero_crossings = frame
        .windows(2)
        .filter(|pair| (pair[0] < 0 && pair[1] >= 0) || (pair[0] >= 0 && pair[1] < 0))
        .count();
    let zcr = zero_crossings as f32 / (frame.len() - 1) as f32;

    (0.015..=0.24).contains(&zcr) && (1.2..=18.0).contains(&crest)
}

fn adaptive_vad_snapshot(adaptive_vad: &Arc<Mutex<AdaptiveS2sVadState>>) -> AdaptiveS2sVadSnapshot {
    adaptive_vad
        .lock()
        .map(|state| state.snapshot())
        .unwrap_or_default()
}

fn observe_adaptive_vad(
    adaptive_vad: &Arc<Mutex<AdaptiveS2sVadState>>,
    outcome: SegmentOutcome,
    segment: &Segment,
) {
    if let Ok(mut state) = adaptive_vad.lock() {
        state.observe(outcome, segment);
    }
}

fn log_adaptive_vad_skip(segment: &Segment, vad: AdaptiveS2sVadSnapshot) {
    eprintln!(
        "[RealtimeS2S][AdaptiveVAD] skip segment={} strictness={:.2} confidence={:.2} speech_like_ratio={:.2} speech_ratio={:.2} mean_rms={:.4} peak_rms={:.4}",
        segment.id,
        vad.strictness,
        segment_speech_confidence(segment),
        segment_speech_like_ratio(segment),
        segment_speech_ratio(segment),
        segment.mean_rms,
        segment.peak_rms
    );
}

fn is_segment_worth_sending(segment: &Segment, vad: AdaptiveS2sVadSnapshot) -> bool {
    let speech_ratio = segment_speech_ratio(segment);
    let speech_like_ratio = segment_speech_like_ratio(segment);
    let confidence = segment_speech_confidence(segment);
    let baseline = segment.speech_frames >= MIN_SEGMENT_SPEECH_FRAMES
        || speech_ratio >= MIN_SEGMENT_SPEECH_RATIO
        || (segment.peak_rms >= MIN_SEGMENT_PEAK_RMS && speech_like_ratio >= 0.08);
    if !baseline {
        return false;
    }

    if vad.strictness <= 0.0 {
        return confidence >= 0.18 || speech_like_ratio >= 0.08;
    }

    let min_speech_like = MIN_SPEECH_LIKE_RATIO
        + (STRICT_MIN_SPEECH_LIKE_RATIO - MIN_SPEECH_LIKE_RATIO) * vad.strictness;
    let min_confidence = 0.24 + (STRICT_MIN_SPEECH_CONFIDENCE - 0.24) * vad.strictness;
    speech_like_ratio >= min_speech_like || confidence >= min_confidence
}

fn segment_speech_ratio(segment: &Segment) -> f32 {
    let frame_count = segment.samples.len().div_ceil(FRAME_SAMPLES).max(1);
    segment.speech_frames as f32 / frame_count as f32
}

fn segment_speech_like_ratio(segment: &Segment) -> f32 {
    let frame_count = segment.samples.len().div_ceil(FRAME_SAMPLES).max(1);
    segment.speech_like_frames as f32 / frame_count as f32
}

fn segment_energetic_ratio(segment: &Segment) -> f32 {
    let frame_count = segment.samples.len().div_ceil(FRAME_SAMPLES).max(1);
    segment.energetic_frames as f32 / frame_count as f32
}

fn segment_speech_confidence(segment: &Segment) -> f32 {
    let speech_ratio = segment_speech_ratio(segment);
    let speech_like_ratio = segment_speech_like_ratio(segment);
    let energetic_ratio = segment_energetic_ratio(segment);
    let energy_score = (segment.mean_rms / 0.055).clamp(0.0, 1.0);
    (speech_like_ratio * 0.45)
        + (speech_ratio * 0.30)
        + (energetic_ratio * 0.15)
        + (energy_score * 0.10)
}

fn samples_to_ms(samples: usize) -> usize {
    samples.saturating_mul(1000) / 16_000
}

fn pcm_bytes_to_i16(bytes: &[u8]) -> Vec<i16> {
    bytes
        .chunks_exact(2)
        .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
        .collect()
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

#[cfg(test)]
mod tests {
    use super::{
        AdaptiveS2sVadSnapshot, AdaptiveS2sVadState, FRAME_SAMPLES, Segment, SegmentOutcome,
        is_segment_worth_sending, merge_segment_text, segment_speech_like_ratio,
    };

    #[test]
    fn s2s_text_merge_preserves_short_word_boundaries() {
        let mut text = String::from("anh ta đang làm");
        merge_segment_text(&mut text, "một hệ điều hành");
        assert_eq!(text, "anh ta đang làm một hệ điều hành");

        let mut text = String::from("một việc lớn chuyên");
        merge_segment_text(&mut text, "nghiệp đâu");
        assert_eq!(text, "một việc lớn chuyên nghiệp đâu");
    }

    #[test]
    fn s2s_text_merge_keeps_real_partial_overlap() {
        let mut text = String::from("cái thú vui đó phát tri");
        merge_segment_text(&mut text, "triển thành một");
        assert_eq!(text, "cái thú vui đó phát triển thành một");
    }

    #[test]
    fn s2s_adaptive_vad_skips_silence() {
        let segment = Segment::new(1, vec![0; FRAME_SAMPLES * 8], 0, 0.0);
        assert!(!is_segment_worth_sending(
            &segment,
            AdaptiveS2sVadSnapshot::default()
        ));
    }

    #[test]
    fn s2s_adaptive_vad_skips_loud_flat_noise() {
        let segment = Segment::new(1, vec![8_000; FRAME_SAMPLES * 8], 8, 0.25);
        assert_eq!(segment.speech_like_frames, 0);
        assert!(!is_segment_worth_sending(
            &segment,
            AdaptiveS2sVadSnapshot { strictness: 1.0 }
        ));
    }

    #[test]
    fn s2s_adaptive_vad_keeps_speech_like_audio() {
        let mut samples = Vec::with_capacity(FRAME_SAMPLES * 8);
        for i in 0..FRAME_SAMPLES * 8 {
            let phase = i as f32 / 16_000.0;
            let envelope = if i % 1_600 < 1_100 { 1.0 } else { 0.45 };
            let sample =
                ((phase * 240.0 * std::f32::consts::TAU).sin() * 9_000.0 * envelope) as i16;
            samples.push(sample);
        }
        let segment = Segment::new(1, samples, 8, 0.20);
        assert!(segment_speech_like_ratio(&segment) > 0.7);
        assert!(is_segment_worth_sending(
            &segment,
            AdaptiveS2sVadSnapshot { strictness: 1.0 }
        ));
    }

    #[test]
    fn s2s_adaptive_vad_learns_from_gemini_feedback() {
        let segment = Segment::new(1, vec![8_000; FRAME_SAMPLES * 8], 8, 0.25);
        let mut state = AdaptiveS2sVadState::default();
        state.observe(SegmentOutcome::EmptyNoInput, &segment);
        assert!(state.snapshot().strictness > 0.0);
        state.observe(SegmentOutcome::Healthy, &segment);
        assert!(state.snapshot().strictness < 0.22);
    }
}
