use crate::api::audio::extract_pcm_from_wav;
use crate::api::realtime_audio::qwen3::Qwen3ModelVariant;
use crate::api::realtime_audio::qwen3::{assets, runtime};
use crate::api::realtime_audio::transcript_state::MonotonicTranscriptState;
use crate::overlay::screen_record::ipc::subtitles::types::CompactSubtitleSegment;
use std::sync::OnceLock;
use std::sync::atomic::Ordering;
use std::sync::mpsc;

use super::{
    SubtitleBackend, SubtitleBackendProgress, SubtitleBackendRequest, normalize_qwen_language_hint,
    normalize_subtitle_text,
};
mod streaming;

use self::streaming::{
    QwenDiagnosticState, QwenPulseSnapshot, StreamingSubtitleAssembler,
    build_visible_progress_segments, compute_rms, finalize_visible_segments, stable_commit_time,
};

const SAMPLE_RATE_HZ: usize = 16_000;
const SAMPLE_RATE_SEC: f64 = SAMPLE_RATE_HZ as f64;
const STREAMING_CHUNK_MS: u32 = 1_000;
const STREAMING_UNFIXED_CHUNKS: usize = 2;
const STREAMING_UNFIXED_TOKENS: usize = 5;
const FEED_CHUNK_SAMPLES: usize = SAMPLE_RATE_HZ / 2;
const SILENCE_COMMIT_SEC: f64 = 0.9;
const MAX_PENDING_BLOCK_SEC: f64 = 6.0;
const VOICE_ACTIVITY_RMS: f32 = 0.015;
const SOFT_SESSION_AUDIO_SEC: f64 = 60.0;
const HARD_SESSION_AUDIO_SEC: f64 = 72.0;
const SOFT_SESSION_KV_CACHE_BYTES: usize = 192 * 1024 * 1024;
const HARD_SESSION_KV_CACHE_BYTES: usize = 224 * 1024 * 1024;
const FIXED_TEXT_LAG_SEC: f64 =
    (STREAMING_CHUNK_MS as f64 * STREAMING_UNFIXED_CHUNKS as f64) / 1_000.0;
const DIAGNOSTIC_LOG_EVERY_STEPS: usize = 32;
const DIAGNOSTIC_KV_GROWTH_BYTES: usize = 128 * 1024 * 1024;
const PROGRESS_UPDATE_STEP_INTERVAL: usize = 8;
const FINAL_FALLBACK_WINDOW_SEC: f64 = MAX_PENDING_BLOCK_SEC;

enum QwenWorkerCommand {
    Transcribe {
        request: SubtitleBackendRequest,
        events: mpsc::Sender<QwenWorkerEvent>,
    },
}

enum QwenWorkerEvent {
    Progress(SubtitleBackendProgress),
    Done(Result<Vec<CompactSubtitleSegment>, String>),
}

static QWEN_SMALL_WORKER: OnceLock<mpsc::Sender<QwenWorkerCommand>> = OnceLock::new();
static QWEN_LARGE_WORKER: OnceLock<mpsc::Sender<QwenWorkerCommand>> = OnceLock::new();

pub struct QwenSubtitleBackend {
    variant: Qwen3ModelVariant,
}

struct QwenDirectSubtitleBackend {
    runtime: runtime::Qwen3Runtime,
    model_label: &'static str,
}

fn qwen_model_info(variant: Qwen3ModelVariant) -> (std::path::PathBuf, bool, &'static str) {
    match variant {
        Qwen3ModelVariant::Small => (
            assets::get_qwen3_model_dir(),
            assets::is_qwen3_model_downloaded(),
            "Qwen3-ASR 0.6B",
        ),
        Qwen3ModelVariant::Large => (
            assets::get_qwen3_1_7b_model_dir(),
            assets::is_qwen3_1_7b_model_downloaded(),
            "Qwen3-ASR 1.7B",
        ),
    }
}

fn validate_qwen_backend_available(variant: Qwen3ModelVariant) -> Result<(), String> {
    let (_model_dir, is_downloaded, model_label) = qwen_model_info(variant);
    if !is_downloaded {
        return Err(format!(
            "Qwen Local subtitles require the {model_label} model from Downloaded Tools."
        ));
    }
    if !runtime::has_discoverable_qwen3_runtime() {
        return Err(
            "Qwen Local subtitles require the Qwen3-ASR CUDA Runtime from Downloaded Tools."
                .to_string(),
        );
    }
    Ok(())
}

fn qwen_worker_slot(
    variant: Qwen3ModelVariant,
) -> &'static OnceLock<mpsc::Sender<QwenWorkerCommand>> {
    match variant {
        Qwen3ModelVariant::Small => &QWEN_SMALL_WORKER,
        Qwen3ModelVariant::Large => &QWEN_LARGE_WORKER,
    }
}

fn qwen_worker_sender(
    variant: Qwen3ModelVariant,
) -> Result<mpsc::Sender<QwenWorkerCommand>, String> {
    validate_qwen_backend_available(variant)?;
    Ok(qwen_worker_slot(variant)
        .get_or_init(|| {
            let (tx, rx) = mpsc::channel();
            std::thread::Builder::new()
                .name(format!("qwen-subtitle-worker-{variant:?}"))
                .spawn(move || qwen_worker_loop(variant, rx))
                .expect("spawn qwen subtitle worker");
            tx
        })
        .clone())
}

fn qwen_worker_loop(variant: Qwen3ModelVariant, rx: mpsc::Receiver<QwenWorkerCommand>) {
    let mut backend: Option<QwenDirectSubtitleBackend> = None;
    for command in rx {
        match command {
            QwenWorkerCommand::Transcribe { request, events } => {
                if backend.is_none() {
                    match QwenDirectSubtitleBackend::new(variant) {
                        Ok(next_backend) => backend = Some(next_backend),
                        Err(error) => {
                            let _ = events.send(QwenWorkerEvent::Done(Err(error)));
                            continue;
                        }
                    }
                }
                let result = {
                    let Some(backend) = backend.as_mut() else {
                        let _ = events.send(QwenWorkerEvent::Done(Err(
                            "Qwen subtitle worker failed to initialize backend".to_string(),
                        )));
                        continue;
                    };
                    let mut progress = |progress: SubtitleBackendProgress| -> Result<(), String> {
                        events
                            .send(QwenWorkerEvent::Progress(progress))
                            .map_err(|_| "Qwen subtitle progress receiver closed".to_string())
                    };
                    backend.transcribe_clip(request, &mut progress)
                };
                let _ = events.send(QwenWorkerEvent::Done(result));
            }
        }
    }
}

fn transcribe_with_persistent_worker(
    variant: Qwen3ModelVariant,
    request: SubtitleBackendRequest,
    on_progress: &mut dyn FnMut(SubtitleBackendProgress) -> Result<(), String>,
) -> Result<Vec<CompactSubtitleSegment>, String> {
    let cancel_token = request.cancel_token.clone();
    let sender = qwen_worker_sender(variant)?;
    let (events_tx, events_rx) = mpsc::channel();
    sender
        .send(QwenWorkerCommand::Transcribe {
            request,
            events: events_tx,
        })
        .map_err(|_| "Qwen subtitle worker is unavailable".to_string())?;

    loop {
        match events_rx
            .recv()
            .map_err(|_| "Qwen subtitle worker stopped before returning a result".to_string())?
        {
            QwenWorkerEvent::Progress(progress) => {
                if let Err(error) = on_progress(progress) {
                    cancel_token.store(true, Ordering::SeqCst);
                    return Err(error);
                }
            }
            QwenWorkerEvent::Done(result) => return result,
        }
    }
}

fn final_text_fallback_start(session_start_sec: f64, end_time_sec: f64) -> f64 {
    (end_time_sec - FINAL_FALLBACK_WINDOW_SEC).max(session_start_sec)
}

impl QwenSubtitleBackend {
    pub fn new(variant: Qwen3ModelVariant) -> Result<Self, String> {
        validate_qwen_backend_available(variant)?;
        Ok(Self { variant })
    }
}

impl SubtitleBackend for QwenSubtitleBackend {
    fn transcribe_clip(
        &mut self,
        request: SubtitleBackendRequest,
        on_progress: &mut dyn FnMut(SubtitleBackendProgress) -> Result<(), String>,
    ) -> Result<Vec<CompactSubtitleSegment>, String> {
        transcribe_with_persistent_worker(self.variant, request, on_progress)
    }
}

impl QwenDirectSubtitleBackend {
    fn new(variant: Qwen3ModelVariant) -> Result<Self, String> {
        let (model_dir, is_downloaded, model_label) = qwen_model_info(variant);
        if !is_downloaded {
            return Err(format!(
                "Qwen Local subtitles require the {model_label} model from Downloaded Tools."
            ));
        }
        if !runtime::has_discoverable_qwen3_runtime() {
            return Err(
                "Qwen Local subtitles require the Qwen3-ASR CUDA Runtime from Downloaded Tools."
                    .to_string(),
            );
        }

        let runtime = runtime::Qwen3Runtime::load_with_kv_cache_mode(
            &model_dir,
            Some(runtime::QWEN3_RUNTIME_KV_MODE_EXPERIMENTAL_TURBOQUANT),
        )
        .map_err(|err| format!("Failed to load Qwen Local subtitle runtime: {err}"))?;
        Ok(Self {
            runtime,
            model_label,
        })
    }
}

impl SubtitleBackend for QwenDirectSubtitleBackend {
    fn transcribe_clip(
        &mut self,
        request: SubtitleBackendRequest,
        on_progress: &mut dyn FnMut(SubtitleBackendProgress) -> Result<(), String>,
    ) -> Result<Vec<CompactSubtitleSegment>, String> {
        if request.media.mime_type != "audio/wav" {
            return Err(format!(
                "Qwen Local subtitles require audio/wav input, got {}",
                request.media.mime_type
            ));
        }

        let samples = extract_pcm_from_wav(&request.media.bytes)
            .map_err(|err| format!("Extract WAV PCM for Qwen Local subtitles: {err}"))?;
        if samples.is_empty() {
            return Ok(Vec::new());
        }

        let normalized_language_hint =
            normalize_qwen_language_hint(request.language_hint.as_deref());
        let total_samples = samples.len();
        let total_steps = total_samples.div_ceil(FEED_CHUNK_SAMPLES);
        crate::log_info!(
            "[SubtitleGen][Qwen] start model={} samples={} steps={} mode=continuous language_hint={:?}",
            self.model_label,
            total_samples,
            total_steps,
            normalized_language_hint
        );

        let mut diagnostics = QwenDiagnosticState::default();
        let mut committed_segments = Vec::new();
        let mut window_assembler = StreamingSubtitleAssembler::default();
        let mut transcript_state = MonotonicTranscriptState::default();
        let mut max_progress_sample = 0usize;
        let mut session = self
            .runtime
            .create_session_with_language(
                STREAMING_CHUNK_MS,
                STREAMING_UNFIXED_CHUNKS,
                STREAMING_UNFIXED_TOKENS,
                normalized_language_hint.as_deref(),
            )
            .map_err(|err| format!("Failed to create Qwen Local subtitle session: {err}"))?;
        let mut current_speech_start_sec: Option<f64> = None;
        let mut last_voice_time_sec: Option<f64> = None;
        let mut session_audio_samples = 0usize;
        let mut session_start_sample = 0usize;
        let mut last_progress_emit_step = 0usize;
        let mut resource_checkpoint_pending = false;

        diagnostics.begin_session();
        crate::log_info!(
            "[SubtitleGen][Qwen] session-start model={} session={} mode=continuous start={:.3}s",
            self.model_label,
            diagnostics.session_count,
            0.0
        );

        for (chunk_index, chunk) in samples.chunks(FEED_CHUNK_SAMPLES).enumerate() {
            if request.cancel_token.load(Ordering::SeqCst) {
                return Err("Subtitle generation cancelled".to_string());
            }
            let chunk_start_sample = chunk_index * FEED_CHUNK_SAMPLES;
            let chunk_end_sample = (chunk_start_sample + chunk.len()).min(total_samples);
            let chunk_start_sec = chunk_start_sample as f64 / SAMPLE_RATE_SEC;
            let chunk_end_sec = chunk_end_sample as f64 / SAMPLE_RATE_SEC;
            let stable_time_sec = stable_commit_time(chunk_end_sec);
            let rms = compute_rms(chunk);
            if rms > VOICE_ACTIVITY_RMS {
                current_speech_start_sec.get_or_insert(chunk_start_sec);
                last_voice_time_sec = Some(chunk_end_sec);
            }

            session.append_pcm16(chunk, false).map_err(|err| {
                format!("Failed to stream audio into Qwen Local subtitles: {err}")
            })?;
            session_audio_samples += chunk.len();

            let transcript = session
                .step()
                .map_err(|err| format!("Qwen Local subtitle streaming step failed: {err}"))?;
            let monotonic_snapshot = transcript_state.ingest(&transcript);
            let fixed_text = monotonic_snapshot.committed_text;

            window_assembler.observe_text(
                &fixed_text,
                current_speech_start_sec,
                chunk_start_sec,
                stable_time_sec,
                false,
            )?;

            if window_assembler.pending_duration(stable_time_sec) >= MAX_PENDING_BLOCK_SEC
                && window_assembler.flush_pending_from_text(&fixed_text, stable_time_sec)?
            {
                current_speech_start_sec = Some(stable_time_sec);
            }

            max_progress_sample = max_progress_sample.max(chunk_end_sample);
            diagnostics.maybe_log_pulse(
                self.model_label,
                &QwenPulseSnapshot {
                    step: max_progress_sample.div_ceil(FEED_CHUNK_SAMPLES),
                    total_steps,
                    stable_time_sec,
                    pending_duration_sec: window_assembler.pending_duration(stable_time_sec),
                    session_audio_samples: transcript.audio_samples,
                    kv_cache_bytes: transcript.kv_cache_bytes,
                    kv_cache_dense_bytes: transcript.kv_cache_dense_bytes,
                    latency_ms: transcript.latency_ms,
                    fixed_chars: transcript.fixed_text.chars().count(),
                    draft_chars: transcript.draft_text.chars().count(),
                },
            );

            let progress_step = max_progress_sample.div_ceil(FEED_CHUNK_SAMPLES);
            if progress_step == 1
                || progress_step == total_steps
                || progress_step.saturating_sub(last_progress_emit_step)
                    >= PROGRESS_UPDATE_STEP_INTERVAL
            {
                let session_progress_segments =
                    window_assembler.progress_segments(&fixed_text, stable_time_sec);
                let progress_segments = build_visible_progress_segments(
                    &committed_segments,
                    &[],
                    &session_progress_segments,
                );
                on_progress(SubtitleBackendProgress {
                    completed_steps: progress_step,
                    total_steps,
                    segments: progress_segments,
                })?;
                last_progress_emit_step = progress_step;
            }

            let silence_commit = last_voice_time_sec.is_some_and(|last_voice_at_sec| {
                current_speech_start_sec.is_some()
                    && chunk_end_sec - last_voice_at_sec >= SILENCE_COMMIT_SEC
            });
            let boundary_ready = window_assembler.pending_duration(stable_time_sec) == 0.0;
            let soft_resource_limit =
                exceeds_soft_session_limits(session_audio_samples, transcript.kv_cache_bytes);
            let hard_resource_limit =
                exceeds_hard_session_limits(session_audio_samples, transcript.kv_cache_bytes);
            resource_checkpoint_pending |= soft_resource_limit;
            let resource_checkpoint = !silence_commit
                && ((resource_checkpoint_pending && boundary_ready) || hard_resource_limit);

            if !silence_commit && !resource_checkpoint {
                continue;
            }

            if request.cancel_token.load(Ordering::SeqCst) {
                return Err("Subtitle generation cancelled".to_string());
            }

            if silence_commit && !resource_checkpoint {
                window_assembler.flush_pending_from_text(&fixed_text, chunk_end_sec)?;
                let committed_before = committed_segments.len();
                committed_segments.extend(std::mem::take(&mut window_assembler.segments));
                on_progress(SubtitleBackendProgress {
                    completed_steps: progress_step,
                    total_steps,
                    segments: committed_segments.clone(),
                })?;
                crate::log_info!(
                    "[SubtitleGen][Qwen] silence-commit model={} session={} start={:.3}s end={:.3}s committed_add={} total_segments={} session_audio_sec={:.1}",
                    self.model_label,
                    diagnostics.session_count,
                    session_start_sample as f64 / SAMPLE_RATE_SEC,
                    chunk_end_sec,
                    committed_segments.len().saturating_sub(committed_before),
                    committed_segments.len(),
                    session_audio_samples as f64 / SAMPLE_RATE_SEC,
                );
                current_speech_start_sec = None;
                last_voice_time_sec = None;
                continue;
            }

            session.append_pcm16(&[], true).map_err(|err| {
                format!("Failed to finalize Qwen Local subtitle stream checkpoint: {err}")
            })?;
            let final_transcript = session.step().map_err(|err| {
                format!("Final Qwen Local subtitle checkpoint step failed: {err}")
            })?;
            let final_snapshot = transcript_state.ingest(&final_transcript);
            let final_text = final_snapshot.committed_text.clone();
            let fallback_start_sec = final_text_fallback_start(
                session_start_sample as f64 / SAMPLE_RATE_SEC,
                chunk_end_sec,
            );
            window_assembler.observe_text(
                &final_text,
                current_speech_start_sec,
                fallback_start_sec,
                chunk_end_sec,
                true,
            )?;
            window_assembler.flush_pending_from_text(&final_text, chunk_end_sec)?;

            let committed_before = committed_segments.len();
            committed_segments.extend(std::mem::take(&mut window_assembler.segments));
            on_progress(SubtitleBackendProgress {
                completed_steps: progress_step,
                total_steps,
                segments: committed_segments.clone(),
            })?;
            crate::log_info!(
                "[SubtitleGen][Qwen] session-checkpoint model={} session={} reason={} start={:.3}s end={:.3}s committed_add={} total_segments={} session_audio_sec={:.1} kv_cache_mb={:.1} dense_mb={:.1}",
                self.model_label,
                diagnostics.session_count,
                if silence_commit {
                    "silence"
                } else if hard_resource_limit {
                    "resource_hard"
                } else {
                    "resource_soft"
                },
                session_start_sample as f64 / SAMPLE_RATE_SEC,
                chunk_end_sec,
                committed_segments.len().saturating_sub(committed_before),
                committed_segments.len(),
                final_transcript.audio_samples as f64 / SAMPLE_RATE_SEC,
                final_transcript.kv_cache_bytes as f64 / (1024.0 * 1024.0),
                final_transcript.kv_cache_dense_bytes as f64 / (1024.0 * 1024.0),
            );

            if chunk_end_sample >= total_samples {
                session_audio_samples = 0;
                current_speech_start_sec = None;
                break;
            }
            drop(session);
            session = self
                .runtime
                .create_session_with_language(
                    STREAMING_CHUNK_MS,
                    STREAMING_UNFIXED_CHUNKS,
                    STREAMING_UNFIXED_TOKENS,
                    normalized_language_hint.as_deref(),
                )
                .map_err(|err| {
                    format!("Failed to create resumed Qwen Local subtitle session: {err}")
                })?;
            diagnostics.begin_session();
            session_audio_samples = 0;
            session_start_sample = chunk_end_sample;
            last_progress_emit_step = progress_step;
            resource_checkpoint_pending = false;
            current_speech_start_sec = None;
            last_voice_time_sec = None;
            crate::log_info!(
                "[SubtitleGen][Qwen] session-start model={} session={} mode=continuous start={:.3}s",
                self.model_label,
                diagnostics.session_count,
                chunk_end_sec
            );
        }

        if session_audio_samples > 0
            || !window_assembler.segments.is_empty()
            || current_speech_start_sec.is_some()
        {
            if request.cancel_token.load(Ordering::SeqCst) {
                return Err("Subtitle generation cancelled".to_string());
            }
            session
                .append_pcm16(&[], true)
                .map_err(|err| format!("Failed to finalize Qwen Local subtitle stream: {err}"))?;
            let final_transcript = session
                .step()
                .map_err(|err| format!("Final Qwen Local subtitle step failed: {err}"))?;
            let final_snapshot = transcript_state.ingest(&final_transcript);
            let final_text = final_snapshot.committed_text;
            let end_time_sec = total_samples as f64 / SAMPLE_RATE_SEC;
            let fallback_start_sec = final_text_fallback_start(
                session_start_sample as f64 / SAMPLE_RATE_SEC,
                end_time_sec,
            );
            window_assembler.observe_text(
                &final_text,
                current_speech_start_sec,
                fallback_start_sec,
                end_time_sec,
                true,
            )?;
            window_assembler.flush_pending_from_text(&final_text, end_time_sec)?;
            committed_segments.extend(std::mem::take(&mut window_assembler.segments));
            on_progress(SubtitleBackendProgress {
                completed_steps: max_progress_sample.div_ceil(FEED_CHUNK_SAMPLES),
                total_steps,
                segments: committed_segments.clone(),
            })?;
        }

        crate::log_info!(
            "[SubtitleGen][Qwen] complete model={} segments={} duration_sec={:.3}",
            self.model_label,
            committed_segments.len(),
            total_samples as f64 / SAMPLE_RATE_SEC
        );

        Ok(finalize_visible_segments(committed_segments, Vec::new()))
    }
}

fn exceeds_soft_session_limits(session_audio_samples: usize, kv_cache_bytes: usize) -> bool {
    session_audio_samples as f64 / SAMPLE_RATE_SEC >= SOFT_SESSION_AUDIO_SEC
        || kv_cache_bytes >= SOFT_SESSION_KV_CACHE_BYTES
}

fn exceeds_hard_session_limits(session_audio_samples: usize, kv_cache_bytes: usize) -> bool {
    session_audio_samples as f64 / SAMPLE_RATE_SEC >= HARD_SESSION_AUDIO_SEC
        || kv_cache_bytes >= HARD_SESSION_KV_CACHE_BYTES
}
