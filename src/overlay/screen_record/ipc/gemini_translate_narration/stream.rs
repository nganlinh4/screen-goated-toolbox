use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::APP;
use crate::api::audio::encode_wav;
use crate::api::gemini_live::transport::{
    connect_websocket, set_socket_nonblocking, set_socket_short_timeout,
};
use crate::api::realtime_audio::websocket::{
    send_audio_chunk, send_audio_stream_end, send_live_translate_setup_message,
};

use super::super::wav_decode::decode_wav_mono_i16;
use super::output_vad::{OutputRegion, OutputVad, speech_active_seconds};
use super::resegment::resegment;
use super::socket_io::{drain_socket, wait_for_setup};
use super::word_distribute::redistribute_words_by_weight;
use super::{ClipResult, GeminiTranslateNarrationRequest, JobSnapshot, ResultEvent, SegmentResult};
use crate::overlay::screen_record::ipc::media_server;
use crate::overlay::screen_record::ipc::subtitles::audio::compact_to_source_time;
use crate::overlay::screen_record::ipc::subtitles::media::prepare_clip_media;
use crate::overlay::screen_record::ipc::subtitles::types::{
    SubtitleClipRequest, SubtitleGenerationMethod,
};

const INPUT_FRAME_SAMPLES: usize = 1600;
const INPUT_SAMPLE_RATE: f64 = 16_000.0;
const OUTPUT_SAMPLE_RATE: f64 = 24_000.0;
/// Absolute i16 amplitude below which an output sample is treated as silence when
/// locating the end of the real speech. Kept low so quiet word-tails are never
/// mistaken for the trailing-silence pad and trimmed.
const SILENCE_LEVEL: u16 = 50;
/// Fallback only: approximate Gemini input-transcription latency, used to anchor
/// the narration when the source onset can't be detected from the audio.
const NARRATION_ANCHOR_LEAD_SEC: f64 = 0.5;

/// Detect the source narrator's onset (compact seconds) directly from the input
/// audio. This anchors the narration on the *source* speech and is
/// latency-independent — unlike anchoring on Gemini's output/transcript timing,
/// which carries Gemini's variable translation/transcription latency. A quiet
/// title or lead-in stays below the threshold; the first sustained real speech
/// clears it.
pub(super) fn detect_source_speech_onset(samples: &[i16]) -> Option<f64> {
    const FRAME: usize = 320; // 20 ms @ 16 kHz
    const RMS_THRESHOLD: f32 = 0.02; // ≈ -34 dB; above a quiet lead-in
    const SUSTAIN_FRAMES: usize = 8; // 160 ms of sustained speech
    let mut run = 0usize;
    for (index, frame) in samples.chunks(FRAME).enumerate() {
        let sum: f32 = frame
            .iter()
            .map(|sample| {
                let value = *sample as f32 / i16::MAX as f32;
                value * value
            })
            .sum();
        let rms = (sum / frame.len().max(1) as f32).sqrt();
        if rms >= RMS_THRESHOLD {
            run += 1;
            if run >= SUSTAIN_FRAMES {
                let onset_frame = (index + 1).saturating_sub(SUSTAIN_FRAMES);
                return Some(onset_frame as f64 * FRAME as f64 / INPUT_SAMPLE_RATE);
            }
        } else {
            run = 0;
        }
    }
    None
}

pub(super) fn run_job_inner(
    job_id: &str,
    request: &GeminiTranslateNarrationRequest,
    snapshot: &Arc<Mutex<JobSnapshot>>,
    cancelled: &Arc<AtomicBool>,
) -> Result<(), String> {
    let api_key = APP
        .lock()
        .map_err(|_| "App config lock poisoned".to_string())?
        .config
        .gemini_api_key
        .clone();
    if api_key.trim().is_empty() {
        return Err("Gemini API key is not configured".to_string());
    }
    crate::log_info!(
        "[GeminiTranslateNarration][job={}] start clips={} source={} target={} target_segment_sec={}",
        job_id,
        request.clips.len(),
        request.source_type,
        request.target_language,
        request.target_segment_sec
    );
    for (clip_index, clip) in request.clips.iter().enumerate() {
        if cancelled.load(Ordering::SeqCst) {
            return Ok(());
        }
        let prepared = prepare_clip_media(
            SubtitleGenerationMethod::GroqWhisperAccurate,
            &request.source_type,
            clip,
        )?;
        let samples = decode_wav_mono_i16(&prepared.bytes, "Gemini Translate")?;
        update_clip_state(snapshot, clip, clip_index, request.clips.len(), 0, 0, false);
        process_clip(
            job_id, request, clip, clip_index, &api_key, samples, snapshot, cancelled,
        )?;
        if let Ok(mut locked) = snapshot.lock() {
            locked.completed_clips = clip_index + 1;
            locked.progress = (clip_index + 1) as f64 / request.clips.len().max(1) as f64;
        }
    }
    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "clip processing needs job, request, clip, auth, audio, and job state"
)]
fn process_clip(
    job_id: &str,
    request: &GeminiTranslateNarrationRequest,
    clip: &SubtitleClipRequest,
    clip_index: usize,
    api_key: &str,
    samples: Vec<i16>,
    snapshot: &Arc<Mutex<JobSnapshot>>,
    cancelled: &Arc<AtomicBool>,
) -> Result<(), String> {
    let mut socket = connect_websocket(api_key).map_err(|error| error.to_string())?;
    send_live_translate_setup_message(
        &mut socket,
        crate::model_config::GEMINI_LIVE_TRANSLATE_API_MODEL,
        &request.target_language,
    )
    .map_err(|error| error.to_string())?;
    set_socket_short_timeout(&mut socket).map_err(|error| error.to_string())?;
    wait_for_setup(&mut socket, cancelled)?;
    set_socket_nonblocking(&mut socket).map_err(|error| error.to_string())?;

    // Latency-independent anchor: where the source narrator actually starts.
    let source_onset = detect_source_speech_onset(&samples);
    let mut vad = OutputVad::new();
    let mut full_output = Vec::new();
    let mut emitted = Vec::<RegionMeta>::new();
    let mut source_text = String::new();
    let mut target_text = String::new();
    let mut last_audio_out_point = 0.0f64;
    let mut sent_chunks = 0usize;
    let mut received_audio_chunks = 0usize;
    let mut received_audio_samples = 0usize;
    let mut saw_turn_complete = false;
    let mut first_input_text_elapsed: Option<f64> = None;
    let mut last_output_speech: Option<Instant> = None;
    let stream_started = Instant::now();

    macro_rules! drain_and_emit {
        () => {{
            let drain = drain_socket(
                &mut socket,
                &mut vad,
                &mut full_output,
                &mut source_text,
                &mut target_text,
                |region, _src, _tgt, _dur| {
                    emitted.push(emit_region(
                        clip,
                        &region,
                        &mut last_audio_out_point,
                        snapshot,
                    ));
                    Ok(())
                },
            )?;
            received_audio_chunks += drain.audio_chunks;
            received_audio_samples += drain.audio_samples;
            saw_turn_complete |= drain.turn_complete;
            if first_input_text_elapsed.is_none() && !source_text.trim().is_empty() {
                first_input_text_elapsed = Some(stream_started.elapsed().as_secs_f64());
            }
            if drain.had_output_speech {
                last_output_speech = Some(Instant::now());
            }
            drain
        }};
    }

    let mut sent_samples = 0usize;
    for chunk in samples.chunks(INPUT_FRAME_SAMPLES) {
        if cancelled.load(Ordering::SeqCst) {
            return Ok(());
        }
        send_audio_chunk(&mut socket, chunk).map_err(|error| error.to_string())?;
        sent_chunks += 1;
        sent_samples += chunk.len();
        let target_elapsed = Duration::from_secs_f64(sent_samples as f64 / INPUT_SAMPLE_RATE);
        loop {
            let _drain = drain_and_emit!();
            let remaining = target_elapsed.saturating_sub(stream_started.elapsed());
            if remaining.is_zero() {
                break;
            }
            std::thread::sleep(remaining.min(Duration::from_millis(20)));
        }
    }
    send_audio_stream_end(&mut socket).map_err(|error| error.to_string())?;
    let source_sec = samples.len() as f64 / INPUT_SAMPLE_RATE;
    let drain_timeout_sec = (source_sec * 1.5 + 60.0).clamp(90.0, 900.0);
    let deadline = Instant::now() + Duration::from_secs_f64(drain_timeout_sec);
    let mut last_activity = Instant::now();
    while !cancelled.load(Ordering::SeqCst) && Instant::now() < deadline {
        let drain = drain_and_emit!();
        if drain.had_activity {
            last_activity = Instant::now();
        }
        if drain.turn_complete {
            break;
        }
        // Gemini's continuous live socket keeps streaming silence after it finishes
        // speaking, so "any audio" is not an end signal. Stop once the real voice
        // has been quiet for a short tail, or if the socket goes fully idle.
        let voice_done = last_output_speech.is_some_and(|at| at.elapsed() > Duration::from_secs(4));
        let socket_idle = !drain.had_activity && last_activity.elapsed() > Duration::from_secs(8);
        if voice_done || socket_idle {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    if let Some(region) = vad.finish() {
        emitted.push(emit_region(
            clip,
            &region,
            &mut last_audio_out_point,
            snapshot,
        ));
    }
    if emitted.is_empty() && !full_output.is_empty() {
        let region = OutputRegion {
            index: 0,
            start_sample: 0,
            end_sample: full_output.len(),
            samples: full_output.clone(),
        };
        emitted.push(emit_region(
            clip,
            &region,
            &mut last_audio_out_point,
            snapshot,
        ));
    }
    // full_output is now the contiguous, complete Gemini output (no wall-clock
    // padding). The output VAD only marks natural pause boundaries; the resegment
    // pass below steers those into even cues near the target length (splitting long
    // pause-free reads, merging short ones) so the whole continuous voice is
    // covered gap-free with no too-long or too-short cue.
    if !emitted.is_empty() {
        emitted.sort_by(|a, b| {
            a.audio_in_point
                .partial_cmp(&b.audio_in_point)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let speech_end_sec = full_output
            .iter()
            .rposition(|&sample| sample.unsigned_abs() > SILENCE_LEVEL)
            .map(|idx| (idx + 1) as f64 / OUTPUT_SAMPLE_RATE + 0.4)
            .unwrap_or_else(|| full_output.len() as f64 / OUTPUT_SAMPLE_RATE);
        let keep_samples = (speech_end_sec * OUTPUT_SAMPLE_RATE).ceil() as usize;
        if keep_samples > 0 && keep_samples < full_output.len() {
            full_output.truncate(keep_samples);
        }
        // Contiguous phrase spans from the natural regions: each covers its speech
        // onset to the next region's onset (or the end of speech), so nothing is
        // dropped between regions.
        let region_count = emitted.len();
        let phrase_spans: Vec<(f64, f64)> = (0..region_count)
            .map(|index| {
                let start = emitted[index].audio_in_point;
                let end = if index + 1 < region_count {
                    emitted[index + 1].audio_in_point
                } else {
                    speech_end_sec
                };
                (start, end.max(start + 0.05))
            })
            .collect();
        // Steer toward the requested target length, then rebuild the takes from the
        // balanced spans (recomputing each cue's speech weight for redistribution).
        let target = request.target_segment_sec.clamp(2.0, 8.0);
        emitted = resegment(&phrase_spans, target)
            .iter()
            .enumerate()
            .map(|(index, &(start, end))| {
                let from = ((start * OUTPUT_SAMPLE_RATE) as usize).min(full_output.len());
                let to = ((end * OUTPUT_SAMPLE_RATE) as usize)
                    .min(full_output.len())
                    .max(from);
                let speech_seconds =
                    speech_active_seconds(&full_output[from..to], OUTPUT_SAMPLE_RATE);
                RegionMeta::new(clip, index, start, end, speech_seconds)
            })
            .collect();

        // Anchor the contiguous narration on the source timeline. A take's
        // sample-offset is NOT its source time (the audio is gap-free, the source
        // has pauses), so anchor on where the source narrator actually starts
        // (detected from the input audio — latency-independent) and tile every cue
        // across the source from there by its offset in the contiguous voice.
        // Falls back to Gemini's first-transcript arrival if the source onset
        // can't be detected. Voice and cues then line up with the source speech.
        let source_duration = clip.source_duration.max(0.0);
        let anchor_compact = source_onset
            .or_else(|| {
                first_input_text_elapsed
                    .map(|elapsed| (elapsed - NARRATION_ANCHOR_LEAD_SEC).max(0.0))
            })
            .unwrap_or(0.0)
            .clamp(0.0, source_duration);
        let first_in = emitted[0].audio_in_point;
        for meta in emitted.iter_mut() {
            let compact_start =
                (anchor_compact + (meta.audio_in_point - first_in)).clamp(0.0, source_duration);
            meta.result.narration_start_time = Some(compact_to_source_time(
                compact_start,
                &clip.trim_segments,
                clip.source_duration,
            ));
        }
    }
    redistribute_segment_text(&mut emitted, &source_text, &target_text);
    if !full_output.is_empty() {
        crate::log_info!(
            "[GeminiTranslateNarration][job={}] clip={} output complete sent_chunks={} source_ms={} wall_ms={} received_audio_chunks={} received_audio_ms={} output_ms={} regions={} turn_complete={}",
            job_id,
            clip.clip_id,
            sent_chunks,
            samples.len() as u64 * 1000 / 16_000,
            stream_started.elapsed().as_millis(),
            received_audio_chunks,
            received_audio_samples as u64 * 1000 / 24_000,
            full_output.len() as u64 * 1000 / 24_000,
            emitted.len(),
            saw_turn_complete
        );
        let wav = encode_wav(&full_output, 24_000, 1);
        let final_path =
            media_server::write_managed_narration_wav(job_id, clip_index * 10_000 + 9_999, &wav)?;
        let final_duration = full_output.len() as f64 / OUTPUT_SAMPLE_RATE;
        for meta in emitted {
            let audio_in_point = meta.audio_in_point;
            let audio_out_point = meta.audio_out_point;
            push_result(
                snapshot,
                ClipResult {
                    clip_id: clip.clip_id.clone(),
                    is_partial: false,
                    segments: vec![meta.into_final(
                        final_path.clone(),
                        final_duration,
                        audio_in_point,
                        audio_out_point,
                    )],
                },
            )?;
        }
    } else {
        let detail = format!(
            "Gemini Translate returned zero output audio after sent_chunks={} source_ms={} turn_complete={}",
            sent_chunks,
            samples.len() as u64 * 1000 / 16_000,
            saw_turn_complete
        );
        crate::log_info!(
            "[GeminiTranslateNarration][job={}] clip={} zero-output {}",
            job_id,
            clip.clip_id,
            detail
        );
        return Err(detail);
    }
    Ok(())
}

#[derive(Clone)]
struct RegionMeta {
    result: SegmentResult,
    audio_in_point: f64,
    audio_out_point: f64,
    speech_seconds: f64,
}

impl RegionMeta {
    /// A take spanning `[audio_in_point, audio_out_point]` (output seconds) with a
    /// result skeleton; text, path, duration, and timeline placement are filled at
    /// finalization.
    fn new(
        clip: &SubtitleClipRequest,
        index: usize,
        audio_in_point: f64,
        audio_out_point: f64,
        speech_seconds: f64,
    ) -> Self {
        Self {
            result: SegmentResult {
                id: format!("{}-gemini-translate-{index}", clip.clip_id),
                clip_id: clip.clip_id.clone(),
                source_text: String::new(),
                target_text: String::new(),
                start_time: 0.0,
                end_time: 0.0,
                narration_start_time: None,
                path: String::new(),
                duration: 0.05,
                audio_in_point: None,
                audio_out_point: None,
                narration_group_take_id: None,
                narration_group_source_start_time: None,
                alignment_mode: Some("aligned".to_string()),
                alignment_confidence: Some(0.82),
                tts_profile_method: Some("gemini-live-translate".to_string()),
            },
            audio_in_point,
            audio_out_point,
            speech_seconds,
        }
    }

    fn into_final(
        self,
        path: String,
        duration: f64,
        audio_in_point: f64,
        audio_out_point: f64,
    ) -> SegmentResult {
        SegmentResult {
            path,
            duration,
            audio_in_point: Some(audio_in_point),
            audio_out_point: Some(audio_out_point),
            alignment_mode: Some("aligned".to_string()),
            alignment_confidence: Some(0.82),
            ..self.result
        }
    }
}

/// Record a take from a detected output-VAD region. This captures only the take's
/// position in the contiguous voice (`audio_in_point`/`audio_out_point`) and its
/// speech weight, plus a result skeleton. The committed audio path, subtitle text,
/// and source-timeline placement are all filled once in finalization — nothing is
/// pushed to the snapshot here, so streaming never commits a result that differs
/// from what concludes (no preview-vs-final drift).
fn emit_region(
    clip: &SubtitleClipRequest,
    region: &OutputRegion,
    last_audio_out_point: &mut f64,
    snapshot: &Arc<Mutex<JobSnapshot>>,
) -> RegionMeta {
    let output_start = region.start_sample as f64 / OUTPUT_SAMPLE_RATE;
    let output_end = region.end_sample as f64 / OUTPUT_SAMPLE_RATE;
    let speech_seconds = speech_active_seconds(&region.samples, OUTPUT_SAMPLE_RATE);
    let audio_in_point = output_start.max(*last_audio_out_point);
    let audio_out_point = output_end.max(audio_in_point + 0.05);
    *last_audio_out_point = audio_out_point;
    if let Ok(mut locked) = snapshot.lock() {
        locked.vad_segment_done = region.index + 1;
        locked.vad_segment_total = 0;
        locked.message = format!(
            "Generating Gemini Translate narration · output VAD {}",
            region.index + 1
        );
    }
    RegionMeta::new(
        clip,
        region.index,
        audio_in_point,
        audio_out_point,
        speech_seconds,
    )
}

/// Lock every subtitle cue onto its narration take and re-deal the transcript by
/// speech timing.
///
/// 1. Each cue's `[start_time, end_time]` is set to exactly span its narration
///    take (`narration_start_time` + the take's audio length), so the subtitle
///    sits on the audio segment — same count, same start/end as the voice.
/// 2. The complete `source_text`/`target_text` are re-dealt across the cues
///    proportional to each cue's **speech-active duration** (how much real voice
///    it carries), so the words line up with where they are spoken: a dense
///    cue's trailing words cascade forward and every take keeps at least one
///    word. Working from the complete transcripts (not the per-region deltas)
///    also discards the cumulative-replay dumps and placeholder lines.
fn redistribute_segment_text(emitted: &mut [RegionMeta], source_text: &str, target_text: &str) {
    if emitted.is_empty() {
        return;
    }
    for meta in emitted.iter_mut() {
        let take_start = meta
            .result
            .narration_start_time
            .unwrap_or(meta.result.start_time);
        let take_duration = (meta.audio_out_point - meta.audio_in_point).max(0.05);
        meta.result.start_time = take_start;
        meta.result.end_time = take_start + take_duration;
    }
    let weights: Vec<f64> = emitted.iter().map(|meta| meta.speech_seconds).collect();
    let source_lines = redistribute_words_by_weight(&weights, source_text);
    let target_lines = redistribute_words_by_weight(&weights, target_text);
    for (index, meta) in emitted.iter_mut().enumerate() {
        meta.result.source_text = source_lines[index].clone();
        meta.result.target_text = target_lines[index].clone();
    }
}

fn push_result(snapshot: &Arc<Mutex<JobSnapshot>>, result: ClipResult) -> Result<(), String> {
    let mut locked = snapshot
        .lock()
        .map_err(|_| "Gemini Translate narration snapshot lock poisoned".to_string())?;
    locked.results_revision += 1;
    let revision = locked.results_revision;
    locked.results = vec![result.clone()];
    locked.result_events.push(ResultEvent { revision, result });
    Ok(())
}

fn update_clip_state(
    snapshot: &Arc<Mutex<JobSnapshot>>,
    clip: &SubtitleClipRequest,
    clip_index: usize,
    total: usize,
    done: usize,
    vad_total: usize,
    no_speech: bool,
) {
    if let Ok(mut locked) = snapshot.lock() {
        locked.state = "running".to_string();
        locked.active_clip_id = Some(clip.clip_id.clone());
        locked.vad_segment_done = done;
        locked.vad_segment_total = vad_total;
        locked.vad_no_speech = no_speech;
        locked.message = format!(
            "Generating Gemini Translate narration {}/{} · live VAD {}/{}",
            clip_index + 1,
            total,
            done,
            vad_total
        );
    }
}
