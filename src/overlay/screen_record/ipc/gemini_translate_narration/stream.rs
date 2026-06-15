use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::APP;
use crate::api::audio::encode_wav;
use crate::api::realtime_audio::websocket::{
    connect_websocket, send_audio_chunk, send_audio_stream_end, send_live_translate_setup_message,
    set_socket_nonblocking, set_socket_short_timeout,
};

use super::super::wav_decode::decode_wav_mono_i16;
use super::output_vad::{OutputRegion, OutputVad};
use super::socket_io::{drain_socket, wait_for_setup};
use super::text_delta::{nonempty_text, take_text_delta};
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
        "[GeminiTranslateNarration][job={}] start clips={} source={} target={} group_budget={}",
        job_id,
        request.clips.len(),
        request.source_type,
        request.target_language,
        request.group_text_budget
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

    let mut vad = OutputVad::new();
    let mut full_output = Vec::new();
    let mut emitted = Vec::<RegionMeta>::new();
    let mut source_text = String::new();
    let mut target_text = String::new();
    let mut last_source_text = String::new();
    let mut last_target_text = String::new();
    let mut shift_sec: Option<f64> = None;
    let mut last_subtitle_compact_end = 0.0f64;
    let mut last_audio_out_point = 0.0f64;
    let mut sent_chunks = 0usize;
    let mut received_audio_chunks = 0usize;
    let mut received_audio_samples = 0usize;
    let mut inserted_silence_samples = 0usize;
    let mut saw_turn_complete = false;
    let stream_started = Instant::now();
    let output_clock = Instant::now();

    macro_rules! drain_and_emit {
        () => {{
            let drain = drain_socket(
                &mut socket,
                &mut vad,
                &mut full_output,
                output_clock,
                &mut source_text,
                &mut target_text,
                |region, current_source_text, current_target_text, stream_duration| {
                    emit_region(
                        job_id,
                        clip,
                        clip_index,
                        region,
                        &mut shift_sec,
                        &mut last_subtitle_compact_end,
                        &mut last_audio_out_point,
                        current_source_text,
                        current_target_text,
                        &mut last_source_text,
                        &mut last_target_text,
                        stream_duration,
                        None,
                        snapshot,
                    )
                    .map(|meta| emitted.push(meta))
                },
            )?;
            received_audio_chunks += drain.audio_chunks;
            received_audio_samples += drain.audio_samples;
            inserted_silence_samples += drain.silence_samples;
            saw_turn_complete |= drain.turn_complete;
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
        if !drain.had_activity && last_activity.elapsed() > Duration::from_secs(8) {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    if let Some(region) = vad.finish() {
        let meta = emit_region(
            job_id,
            clip,
            clip_index,
            region,
            &mut shift_sec,
            &mut last_subtitle_compact_end,
            &mut last_audio_out_point,
            &source_text,
            &target_text,
            &mut last_source_text,
            &mut last_target_text,
            full_output.len() as f64 / OUTPUT_SAMPLE_RATE,
            None,
            snapshot,
        )?;
        emitted.push(meta);
    }
    if emitted.is_empty() && !full_output.is_empty() {
        let region = OutputRegion {
            index: 0,
            start_sample: 0,
            end_sample: full_output.len(),
            samples: full_output.clone(),
        };
        let meta = emit_region(
            job_id,
            clip,
            clip_index,
            region,
            &mut shift_sec,
            &mut last_subtitle_compact_end,
            &mut last_audio_out_point,
            &source_text,
            &target_text,
            &mut last_source_text,
            &mut last_target_text,
            full_output.len() as f64 / OUTPUT_SAMPLE_RATE,
            None,
            snapshot,
        )?;
        emitted.push(meta);
    }
    if !full_output.is_empty() {
        crate::log_info!(
            "[GeminiTranslateNarration][job={}] clip={} output complete sent_chunks={} source_ms={} wall_ms={} received_audio_chunks={} received_audio_ms={} inserted_silence_ms={} timeline_ms={} regions={} turn_complete={}",
            job_id,
            clip.clip_id,
            sent_chunks,
            samples.len() as u64 * 1000 / 16_000,
            stream_started.elapsed().as_millis(),
            received_audio_chunks,
            received_audio_samples as u64 * 1000 / 24_000,
            inserted_silence_samples as u64 * 1000 / 24_000,
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
}

impl RegionMeta {
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

#[expect(
    clippy::too_many_arguments,
    reason = "emission needs stream, alignment, and job state"
)]
fn emit_region(
    job_id: &str,
    clip: &SubtitleClipRequest,
    clip_index: usize,
    region: OutputRegion,
    shift_sec: &mut Option<f64>,
    last_subtitle_compact_end: &mut f64,
    last_audio_out_point: &mut f64,
    source_text: &str,
    target_text: &str,
    last_source_text: &mut String,
    last_target_text: &mut String,
    stream_duration: f64,
    final_path: Option<String>,
    snapshot: &Arc<Mutex<JobSnapshot>>,
) -> Result<RegionMeta, String> {
    let output_start = region.start_sample as f64 / OUTPUT_SAMPLE_RATE;
    let output_end = region.end_sample as f64 / OUTPUT_SAMPLE_RATE;
    let audio_in_point = output_start.max(*last_audio_out_point);
    let audio_out_point = output_end.max(audio_in_point + 0.05);
    let audio_trim_delta = audio_in_point - output_start;
    let audio_shift = *shift_sec.get_or_insert_with(|| output_start.clamp(0.0, 4.0));
    let subtitle_shift = audio_shift.min(0.35);
    let mut compact_start = (output_start + audio_trim_delta - subtitle_shift)
        .max(0.0)
        .max(*last_subtitle_compact_end);
    let mut compact_end = (output_end - subtitle_shift).max(compact_start + 0.05);
    let mut narration_compact_start = (audio_in_point - audio_shift).max(0.0);
    let source_duration = clip.source_duration.max(0.0);
    compact_start = compact_start.min(source_duration);
    compact_end = compact_end.min(source_duration.max(compact_start + 0.05));
    narration_compact_start = narration_compact_start.min(source_duration);
    *last_subtitle_compact_end = compact_end;
    *last_audio_out_point = audio_out_point;
    let start_time =
        compact_to_source_time(compact_start, &clip.trim_segments, clip.source_duration);
    let end_time = compact_to_source_time(compact_end, &clip.trim_segments, clip.source_duration)
        .max(start_time + 0.05);
    let narration_start_time = compact_to_source_time(
        narration_compact_start,
        &clip.trim_segments,
        clip.source_duration,
    );
    let is_final = final_path.is_some();
    let path = if let Some(path) = final_path {
        path
    } else {
        let wav = encode_wav(&region.samples, 24_000, 1);
        media_server::write_managed_narration_wav(job_id, clip_index * 10_000 + region.index, &wav)?
    };
    let source_delta = take_text_delta(source_text, last_source_text);
    let target_delta = take_text_delta(target_text, last_target_text);
    let duration = if is_final {
        stream_duration
    } else {
        region.samples.len() as f64 / OUTPUT_SAMPLE_RATE
    };
    let local_audio_in_point = audio_trim_delta.max(0.0);
    let result = SegmentResult {
        id: format!("{}-gemini-translate-{}", clip.clip_id, region.index),
        clip_id: clip.clip_id.clone(),
        source_text: nonempty_text(source_delta, source_text, "Speech"),
        target_text: nonempty_text(target_delta, target_text, "Translation"),
        start_time,
        end_time,
        narration_start_time: Some(narration_start_time),
        path,
        duration: duration.max(0.05),
        audio_in_point: Some(local_audio_in_point),
        audio_out_point: Some((output_end - output_start).max(local_audio_in_point + 0.05)),
        narration_group_take_id: None,
        narration_group_source_start_time: None,
        alignment_mode: Some("estimated".to_string()),
        alignment_confidence: Some(0.68),
        tts_profile_method: Some("gemini-live-translate".to_string()),
    };
    push_result(
        snapshot,
        ClipResult {
            clip_id: clip.clip_id.clone(),
            is_partial: !is_final,
            segments: vec![result.clone()],
        },
    )?;
    if let Ok(mut locked) = snapshot.lock() {
        locked.vad_segment_done = region.index + 1;
        locked.vad_segment_total = 0;
        locked.message = format!(
            "Generating Gemini Translate narration · live output VAD {}",
            region.index + 1
        );
    }
    Ok(RegionMeta {
        result,
        audio_in_point,
        audio_out_point,
    })
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
