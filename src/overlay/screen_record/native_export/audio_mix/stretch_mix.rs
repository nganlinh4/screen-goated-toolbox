//! Mixes one audio source onto the output timeline through the WSOLA stretcher.
//!
//! This is the path everything takes that a single FFmpeg `atempo` cannot handle —
//! speed ramps, stepped speed levels, trim cuts inside a source. It walks each kept
//! region emitting fixed-size output hops, drawing each grain from wherever the
//! shared time map says that output instant sits in the source. Because the walker
//! and the mixer integrate the same curve with the same step, the audio lands where
//! the video frames do.

use std::collections::VecDeque;

use super::super::super::mf_audio::MfAudioDecoder;
use super::super::config::{DeviceAudioPoint, SpeedPoint, TrimSegment};
use super::mix_buffer::FloatMixBuffer;
use super::time_map::{
    OutputTimeMapper, SegmentWalker, get_audio_volume, implicit_edge_fade_multiplier,
    source_project_start_time, source_timeline_end_time, volume_points_in_output_time,
};
use super::wav_fast::{DecodedAudioChunk, read_wav_fast_chunks};
use super::wsola::Wsola;
use super::{ExportAudioSource, MIX_OUTPUT_CHANNELS, MIX_OUTPUT_SAMPLE_RATE};

const TIME_EPSILON: f64 = 1e-9;

fn frames_to_time(frames: usize) -> f64 {
    frames as f64 / MIX_OUTPUT_SAMPLE_RATE as f64
}

fn time_to_frame(time: f64) -> i64 {
    (time * MIX_OUTPUT_SAMPLE_RATE as f64).round() as i64
}

enum ChunkReader {
    Wav(VecDeque<DecodedAudioChunk>),
    Mf(Box<MfAudioDecoder>),
}

impl ChunkReader {
    fn open(path: &str) -> Result<(Self, &'static str), String> {
        if let Some(chunks) = read_wav_fast_chunks(path)? {
            return Ok((ChunkReader::Wav(chunks), "wsola_wav"));
        }
        let decoder = MfAudioDecoder::new_with_output_format(
            path,
            Some(MIX_OUTPUT_SAMPLE_RATE),
            Some(MIX_OUTPUT_CHANNELS),
        )?;
        Ok((ChunkReader::Mf(Box::new(decoder)), "wsola_mf"))
    }

    /// Positions the reader at or before `source_time_sec`. Landing early is fine —
    /// the stretcher indexes by absolute frame and ignores what it does not need.
    fn seek(&mut self, source_time_sec: f64) {
        let target = source_time_sec.max(0.0);
        match self {
            ChunkReader::Wav(chunks) => {
                while let Some(chunk) = chunks.front() {
                    let chunk_end = chunk.decoded_time + frames_to_time(chunk.frames());
                    if chunk_end <= target {
                        chunks.pop_front();
                    } else {
                        break;
                    }
                }
            }
            ChunkReader::Mf(decoder) => {
                let _ = decoder.seek((target * 10_000_000.0) as i64);
            }
        }
    }

    fn next_chunk(&mut self) -> Result<Option<DecodedAudioChunk>, String> {
        match self {
            ChunkReader::Wav(chunks) => Ok(chunks.pop_front()),
            ChunkReader::Mf(decoder) => {
                let Some((pcm, ts_100ns)) = decoder.read_samples()? else {
                    return Ok(None);
                };
                let samples = pcm
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|bytes| f32::from_le_bytes(*bytes))
                    .collect();
                Ok(Some(DecodedAudioChunk {
                    samples,
                    decoded_time: ts_100ns as f64 / 10_000_000.0,
                    channels: decoder.channels() as usize,
                }))
            }
        }
    }
}

/// The source's audible footprint on the output timeline, used to place the implicit
/// edge fade that keeps clip boundaries from clicking.
struct OutputFootprint {
    start: f64,
    end: f64,
}

fn apply_output_envelope(
    samples: &mut [f32],
    start_frame: usize,
    channels: usize,
    points: &[DeviceAudioPoint],
    fade: Option<(&OutputFootprint, f64)>,
) {
    if samples.is_empty() || channels == 0 {
        return;
    }
    let shapes_volume = points
        .iter()
        .any(|point| (point.volume.clamp(0.0, 1.0) - 1.0).abs() >= 0.0001);
    if !shapes_volume && fade.is_none() {
        return;
    }
    for (index, frame) in samples.chunks_exact_mut(channels).enumerate() {
        let time = frames_to_time(start_frame + index) + 0.5 / MIX_OUTPUT_SAMPLE_RATE as f64;
        let edge = fade
            .map(|(footprint, fade_sec)| {
                implicit_edge_fade_multiplier(time, footprint.start, footprint.end, fade_sec)
            })
            .unwrap_or(1.0);
        let gain = (get_audio_volume(time, points) * edge) as f32;
        if (gain - 1.0).abs() < 0.0001 {
            continue;
        }
        for sample in frame {
            *sample = (*sample * gain).clamp(-1.0, 1.0);
        }
    }
}

/// One kept region intersected with the source's own footprint, already resolved
/// onto the output timeline.
struct PlaySpan {
    project_start: f64,
    project_end: f64,
    output_start: f64,
    output_frames: usize,
}

fn play_spans(
    source: &ExportAudioSource,
    trim_segments: &[TrimSegment],
    speed_points: &[SpeedPoint],
    fallback_duration: f64,
) -> Vec<PlaySpan> {
    let project_start = source_project_start_time(source);
    let project_end = source_timeline_end_time(source, trim_segments, fallback_duration);
    let mut mapper = OutputTimeMapper::new(trim_segments.to_vec(), speed_points.to_vec());
    let mut spans = Vec::new();
    for segment in trim_segments {
        let span_start = segment.start_time.max(project_start);
        let span_end = segment.end_time.min(project_end);
        if span_end <= span_start + TIME_EPSILON {
            continue;
        }
        let (Some(output_start), Some(output_end)) = (
            mapper.map_source_time(span_start),
            mapper.map_source_time(span_end),
        ) else {
            break;
        };
        let output_frames = (time_to_frame(output_end) - time_to_frame(output_start)).max(0);
        if output_frames == 0 {
            continue;
        }
        spans.push(PlaySpan {
            project_start: span_start,
            project_end: span_end,
            output_start,
            output_frames: output_frames as usize,
        });
    }
    spans
}

struct SpanMixContext<'a> {
    source: &'a ExportAudioSource,
    speed_points: &'a [SpeedPoint],
    volume_points: &'a [DeviceAudioPoint],
    footprint: &'a OutputFootprint,
    channels: usize,
}

fn mix_one_span(
    mixer: &mut FloatMixBuffer,
    reader: &mut ChunkReader,
    span: &PlaySpan,
    context: &SpanMixContext<'_>,
) -> Result<(), String> {
    let SpanMixContext {
        source,
        speed_points,
        volume_points,
        footprint,
        channels,
    } = *context;
    let playback_rate = source.playback_rate.max(0.0001);
    let source_time_of =
        |project_time: f64| ((project_time - source.start_offset_sec) * playback_rate).max(0.0);

    reader.seek(source_time_of(span.project_start));
    let mut stretcher = Wsola::new(channels);
    let mut walker = SegmentWalker::new(
        span.project_start,
        span.project_end,
        span.output_start,
        speed_points.to_vec(),
    );

    let hop_frames = Wsola::output_hop_frames();
    let hop_seconds = frames_to_time(hop_frames);
    let base_frame = time_to_frame(span.output_start).max(0) as usize;
    let fade =
        (source.implicit_edge_fade_sec > 0.0).then_some((footprint, source.implicit_edge_fade_sec));

    let mut emitted = 0usize;
    let mut reader_drained = false;
    let mut hop_samples: Vec<f32> = Vec::with_capacity(hop_frames * channels);

    while emitted < span.output_frames {
        let analysis = time_to_frame(source_time_of(walker.project_time()));
        while !reader_drained && Wsola::required_input_end(analysis) > stretcher.buffered_end() {
            match reader.next_chunk()? {
                Some(chunk) => {
                    stretcher.push(&chunk.samples, time_to_frame(chunk.decoded_time));
                }
                None => {
                    stretcher.end_input();
                    reader_drained = true;
                }
            }
        }

        hop_samples.clear();
        if !stretcher.hop(analysis, &mut hop_samples) {
            break;
        }
        let keep = (span.output_frames - emitted).min(hop_frames);
        hop_samples.truncate(keep * channels);
        apply_output_envelope(
            &mut hop_samples,
            base_frame + emitted,
            channels,
            volume_points,
            fade,
        );
        mixer.mix_f32_at_frame(base_frame + emitted, &hop_samples, channels)?;
        emitted += keep;

        if !walker.advance(hop_seconds) {
            break;
        }
    }
    Ok(())
}

/// Mixes `source` onto the output timeline following the speed curve. Returns which
/// decoder path was used, for the export log.
pub(super) fn mix_source_with_stretch(
    mixer: &mut FloatMixBuffer,
    source: &ExportAudioSource,
    trim_segments: &[TrimSegment],
    speed_points: &[SpeedPoint],
    fallback_duration: f64,
) -> Result<&'static str, String> {
    let spans = play_spans(source, trim_segments, speed_points, fallback_duration);
    let (Some(first), Some(last)) = (spans.first(), spans.last()) else {
        return Ok("wsola_silent");
    };
    let footprint = OutputFootprint {
        start: first.output_start,
        end: last.output_start + frames_to_time(last.output_frames),
    };
    let volume_points = volume_points_in_output_time(
        &source.volume_points,
        trim_segments,
        speed_points,
        footprint.end,
    );

    let (mut reader, path_kind) = ChunkReader::open(&source.path)?;
    let context = SpanMixContext {
        source,
        speed_points,
        volume_points: &volume_points,
        footprint: &footprint,
        channels: MIX_OUTPUT_CHANNELS as usize,
    };
    for span in &spans {
        mix_one_span(mixer, &mut reader, span, &context)?;
    }
    Ok(path_kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source_at(offset: f64) -> ExportAudioSource {
        ExportAudioSource {
            path: "device.wav".to_string(),
            volume_points: Vec::new(),
            start_offset_sec: offset,
            source_in_sec: None,
            source_out_sec: None,
            playback_rate: 1.0,
            implicit_edge_fade_sec: 0.0,
        }
    }

    fn speed_curve(points: &[(f64, f64)]) -> Vec<SpeedPoint> {
        points
            .iter()
            .map(|&(time, speed)| SpeedPoint { time, speed })
            .collect()
    }

    fn trim(spans: &[(f64, f64)]) -> Vec<TrimSegment> {
        spans
            .iter()
            .map(|&(start_time, end_time)| TrimSegment {
                start_time,
                end_time,
            })
            .collect()
    }

    #[test]
    fn a_cut_becomes_two_spans_that_are_contiguous_in_output() {
        let spans = play_spans(
            &source_at(0.0),
            &trim(&[(0.0, 2.0), (6.0, 10.0)]),
            &speed_curve(&[(0.0, 2.0), (10.0, 2.0)]),
            10.0,
        );

        assert_eq!(spans.len(), 2);
        assert!((spans[0].output_start - 0.0).abs() < 1e-6);
        assert_eq!(spans[0].output_frames, 48_000);
        // The cut is dropped, not squeezed: the second span picks up right where the
        // first left off on the output timeline.
        assert!((spans[1].output_start - 1.0).abs() < 1e-3);
        assert_eq!(spans[1].output_frames, 96_000);
    }

    #[test]
    fn a_ramp_span_matches_the_shared_mix_duration() {
        let speed_points = speed_curve(&[(0.0, 1.0), (4.0, 4.0)]);
        let trim_segments = trim(&[(0.0, 4.0)]);
        let spans = play_spans(&source_at(0.0), &trim_segments, &speed_points, 4.0);

        assert_eq!(spans.len(), 1);
        let expected =
            super::super::calculate_mix_output_duration(0.0, 4.0, &trim_segments, &speed_points);
        let planned = frames_to_time(spans[0].output_frames);
        assert!(
            (planned - expected).abs() < 1e-3,
            "planned {planned} vs mix duration {expected}"
        );
    }

    #[test]
    fn a_track_delay_shifts_the_span_and_clips_the_tail() {
        let spans = play_spans(
            &source_at(0.75),
            &trim(&[(0.0, 10.0)]),
            &speed_curve(&[(0.0, 2.0), (10.0, 2.0)]),
            10.0,
        );

        assert_eq!(spans.len(), 1);
        assert!((spans[0].project_start - 0.75).abs() < 1e-6);
        assert!((spans[0].project_end - 10.0).abs() < 1e-6);
        // 0.75s of delay at 2x lands the audio at output 0.375s.
        assert!((spans[0].output_start - 0.375).abs() < 1e-3);
        assert!((frames_to_time(spans[0].output_frames) - 4.625).abs() < 1e-3);
    }

    /// `duration` is the *trimmed* length while trim segments carry absolute source
    /// times, so an open-ended source (device/mic audio) that ended at `duration`
    /// stopped `trimStart` seconds early — the picture ran on with silence under it.
    #[test]
    fn a_trimmed_start_does_not_cut_the_audio_tail_short() {
        let full = 323.57;
        for trim_start in [0.0_f64, 10.0, 25.0] {
            let duration = full - trim_start;
            let trim_segments = trim(&[(trim_start, full)]);
            let speed_points = speed_curve(&[(trim_start, 2.0), (full, 2.0)]);
            let spans = play_spans(&source_at(0.0), &trim_segments, &speed_points, duration);

            let covered: f64 = spans
                .iter()
                .map(|span| frames_to_time(span.output_frames))
                .sum();
            let timeline = super::super::calculate_mix_output_duration(
                trim_start,
                duration,
                &trim_segments,
                &speed_points,
            );
            assert!(
                (covered - timeline).abs() < 0.01,
                "trimStart={trim_start}: audio covers {covered}s of a {timeline}s timeline"
            );
        }
    }

    #[test]
    fn a_source_cut_away_entirely_produces_no_spans() {
        let mut source = source_at(3.0);
        source.source_out_sec = Some(1.0);
        let spans = play_spans(
            &source,
            &trim(&[(0.0, 2.0), (6.0, 10.0)]),
            &speed_curve(&[(0.0, 1.0), (10.0, 1.0)]),
            10.0,
        );
        assert!(spans.is_empty());
    }
}
