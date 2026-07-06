//! Standalone CLI harness for the Gemini Translate narration streaming, so the
//! exact input-feed + drain + output-assembly + VAD-segmentation path can be
//! exercised on a real audio file and validated (e.g. with Groq Whisper)
//! without driving the app.
//!
//! Run via `--gt-narration-test <input.wav> [--gt-narration-lang vi]`. Writes,
//! next to the input, three WAVs:
//!
//! - `<input>.narration.wav`: the raw assembled Gemini output.
//! - `<input>.takes-raw.wav`: speech the export would keep using bare VAD regions
//!   (this is where words get dropped).
//! - `<input>.takes-gapfree.wav`: speech the export keeps with the gap-free take
//!   coverage fix (nothing dropped).
//!
//! Comparing the last two with Groq proves the under-segmentation bug + the fix.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use crate::api::audio::encode_wav;
use crate::api::realtime_audio::websocket::{
    connect_websocket, send_audio_chunk, send_audio_stream_end, send_live_translate_setup_message,
    set_socket_nonblocking, set_socket_short_timeout,
};

use super::super::wav_decode::decode_wav_mono_i16;
use super::output_vad::OutputVad;
use super::resegment::resegment;
use super::socket_io::{drain_socket, wait_for_setup};
use super::stream::detect_source_speech_onset;

const INPUT_FRAME_SAMPLES: usize = 1600;
const INPUT_SAMPLE_RATE: f64 = 16_000.0;
const OUTPUT_SAMPLE_RATE: u32 = 24_000;
const SILENCE_LEVEL: u16 = 50;

/// A detected output speech region as `[start_sample, end_sample)` into the
/// assembled output buffer.
type Region = (usize, usize);

struct StreamResult {
    full_output: Vec<i16>,
    regions: Vec<Region>,
    source_text: String,
    target_text: String,
    turn_complete: bool,
}

/// Stream `samples` (16 kHz mono) through the live-translate pipeline and return
/// the assembled output plus the VAD regions. Mirrors `stream::process_clip`
/// exactly (same input pacing, same contiguous-concat `drain_socket`, same
/// `OutputVad`) minus the snapshot bookkeeping, so behavior is identical.
fn stream_to_output(
    samples: &[i16],
    api_key: &str,
    target_language: &str,
) -> Result<StreamResult, String> {
    let cancelled = Arc::new(AtomicBool::new(false));
    let mut socket = connect_websocket(api_key).map_err(|e| e.to_string())?;
    send_live_translate_setup_message(
        &mut socket,
        crate::model_config::GEMINI_LIVE_TRANSLATE_API_MODEL,
        target_language,
    )
    .map_err(|e| e.to_string())?;
    set_socket_short_timeout(&mut socket).map_err(|e| e.to_string())?;
    wait_for_setup(&mut socket, &cancelled)?;
    set_socket_nonblocking(&mut socket).map_err(|e| e.to_string())?;

    let mut vad = OutputVad::new();
    let mut full_output = Vec::new();
    let mut source_text = String::new();
    let mut target_text = String::new();
    let mut regions: Vec<Region> = Vec::new();
    let mut saw_turn_complete = false;

    macro_rules! drain {
        () => {{
            drain_socket(
                &mut socket,
                &mut vad,
                &mut full_output,
                &mut source_text,
                &mut target_text,
                |region, _src, _tgt, _dur| {
                    regions.push((region.start_sample, region.end_sample));
                    Ok(())
                },
            )?
        }};
    }

    let stream_started = Instant::now();
    let mut sent_samples = 0usize;
    let mut last_output_speech: Option<Instant> = None;
    for chunk in samples.chunks(INPUT_FRAME_SAMPLES) {
        send_audio_chunk(&mut socket, chunk).map_err(|e| e.to_string())?;
        sent_samples += chunk.len();
        let target_elapsed = Duration::from_secs_f64(sent_samples as f64 / INPUT_SAMPLE_RATE);
        loop {
            if drain!().had_output_speech {
                last_output_speech = Some(Instant::now());
            }
            let remaining = target_elapsed.saturating_sub(stream_started.elapsed());
            if remaining.is_zero() {
                break;
            }
            std::thread::sleep(remaining.min(Duration::from_millis(20)));
        }
    }
    send_audio_stream_end(&mut socket).map_err(|e| e.to_string())?;

    let source_sec = samples.len() as f64 / INPUT_SAMPLE_RATE;
    let drain_timeout_sec = (source_sec * 1.5 + 60.0).clamp(90.0, 900.0);
    let deadline = Instant::now() + Duration::from_secs_f64(drain_timeout_sec);
    let mut last_activity = Instant::now();
    while Instant::now() < deadline {
        let drained = drain!();
        if drained.had_activity {
            last_activity = Instant::now();
        }
        if drained.had_output_speech {
            last_output_speech = Some(Instant::now());
        }
        if drained.turn_complete {
            saw_turn_complete = true;
            break;
        }
        let voice_done = last_output_speech.is_some_and(|at| at.elapsed() > Duration::from_secs(4));
        let socket_idle = !drained.had_activity && last_activity.elapsed() > Duration::from_secs(8);
        if voice_done || socket_idle {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    if let Some(region) = vad.finish() {
        regions.push((region.start_sample, region.end_sample));
    }
    regions.sort_by_key(|&(start, _)| start);

    Ok(StreamResult {
        full_output,
        regions,
        source_text,
        target_text,
        turn_complete: saw_turn_complete,
    })
}

fn secs(samples: usize) -> f64 {
    samples as f64 / OUTPUT_SAMPLE_RATE as f64
}

/// Concatenate `full_output[start..end]` for each `(start, end)` span.
fn gather(full_output: &[i16], spans: &[Region]) -> Vec<i16> {
    let mut out = Vec::new();
    for &(start, end) in spans {
        let start = start.min(full_output.len());
        let end = end.min(full_output.len()).max(start);
        out.extend_from_slice(&full_output[start..end]);
    }
    out
}

/// Index just past the last non-silent sample in `full_output`.
fn speech_end_sample(full_output: &[i16]) -> usize {
    full_output
        .iter()
        .rposition(|&sample| sample.unsigned_abs() > SILENCE_LEVEL)
        .map(|idx| idx + 1)
        .unwrap_or(full_output.len())
}

/// Apply the gap-free coverage fix: each take's out-point becomes the next
/// take's in-point; the last extends to the end of the real speech.
fn gap_free_spans(regions: &[Region], speech_end: usize) -> Vec<Region> {
    regions
        .iter()
        .enumerate()
        .map(|(index, &(start, _))| {
            let end = regions
                .get(index + 1)
                .map(|&(next, _)| next)
                .unwrap_or(speech_end);
            (start, end.max(start))
        })
        .collect()
}

/// CLI entry: decode `input_wav` (16 kHz mono PCM WAV), stream it, write the raw
/// output + bare-VAD-region takes + gap-free takes, and print a coverage report.
pub(crate) fn run_cli(input_wav: &str, target_language: &str) -> Result<(), String> {
    let api_key = std::env::var("GEMINI_API_KEY")
        .ok()
        .filter(|key| !key.trim().is_empty())
        .ok_or_else(|| "GEMINI_API_KEY env var is not set".to_string())?;
    let bytes = std::fs::read(input_wav).map_err(|e| format!("read {input_wav}: {e}"))?;
    let samples = decode_wav_mono_i16(&bytes, "GtTest")?;
    eprintln!(
        "[gt-test] input={} samples={} ({:.1}s @16k) target={} source_onset={}",
        input_wav,
        samples.len(),
        samples.len() as f64 / INPUT_SAMPLE_RATE,
        target_language,
        detect_source_speech_onset(&samples)
            .map(|onset| format!("{onset:.2}s"))
            .unwrap_or_else(|| "none".to_string())
    );

    let result = stream_to_output(&samples, &api_key, target_language)?;
    let StreamResult {
        full_output,
        regions,
        source_text,
        target_text,
        turn_complete,
    } = result;

    let speech_end = speech_end_sample(&full_output);
    let gapfree = gap_free_spans(&regions, speech_end);

    eprintln!(
        "[gt-test] raw_output={:.1}s speech_end={:.1}s regions={} turn_complete={} source_chars={} target_chars={}",
        secs(full_output.len()),
        secs(speech_end),
        regions.len(),
        turn_complete,
        source_text.len(),
        target_text.len()
    );
    // Resegment the contiguous phrase spans toward the default target (4.0s) — the
    // same balancing the app applies — and report the resulting cue durations so we
    // can see there are no too-long or too-short cues.
    let phrase_spans_sec: Vec<(f64, f64)> = gapfree
        .iter()
        .map(|&(start, end)| (secs(start), secs(end)))
        .collect();
    let balanced = resegment(&phrase_spans_sec, 4.0);
    let balanced_durs: Vec<f64> = balanced.iter().map(|&(start, end)| end - start).collect();
    let min_dur = balanced_durs.iter().copied().fold(f64::INFINITY, f64::min);
    let max_dur = balanced_durs.iter().copied().fold(0.0_f64, f64::max);
    eprintln!(
        "[gt-test] resegment(target=4.0s) -> {} cues  min={:.2}s max={:.2}s",
        balanced.len(),
        min_dur,
        max_dur
    );
    for (index, &(start, end)) in balanced.iter().enumerate() {
        eprintln!(
            "[gt-test]   cue {:2}: {:6.2}-{:6.2}s  dur={:4.2}s",
            index,
            start,
            end,
            end - start
        );
    }

    let write = |suffix: &str, pcm: &[i16]| -> Result<String, String> {
        let path = format!("{input_wav}.{suffix}.wav");
        std::fs::write(&path, encode_wav(pcm, OUTPUT_SAMPLE_RATE, 1))
            .map_err(|e| format!("write {path}: {e}"))?;
        Ok(path)
    };

    let raw_path = write("narration", &full_output)?;
    let takes_raw = gather(&full_output, &regions);
    let takes_raw_path = write("takes-raw", &takes_raw)?;
    let takes_gapfree = gather(&full_output, &gapfree);
    let takes_gapfree_path = write("takes-gapfree", &takes_gapfree)?;

    eprintln!(
        "[gt-test] DONE raw={} takes_raw={} ({:.1}s) takes_gapfree={} ({:.1}s)",
        raw_path,
        takes_raw_path,
        secs(takes_raw.len()),
        takes_gapfree_path,
        secs(takes_gapfree.len())
    );
    println!("{takes_gapfree_path}");
    Ok(())
}
