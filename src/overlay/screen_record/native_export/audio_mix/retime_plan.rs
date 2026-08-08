//! Decides whether one audio source can be retimed by FFmpeg's `atempo`.
//!
//! The video pipeline advances source time through the speed curve frame by frame
//! (`gpu_pipeline::build_frame_times`), so audio has to follow the same curve or it
//! drifts against the picture. `atempo` only ever applies one constant tempo, so it
//! is correct exactly when the curve is flat across the source's footprint and no
//! trim cut falls inside it. Anything else — a ramp, a cut, several speed levels —
//! goes to the WSOLA stretcher in `stretch_mix`, which follows the curve hop by hop
//! and preserves pitch just as `atempo` does.
//!
//! Slicing the source and giving each piece its own `atempo` does not work: `atempo`
//! swallows the first ~21ms of every instance it runs and emits nothing at all for
//! windows under ~50ms, so a sliced-and-concatenated graph loses audio at every
//! boundary. Its runtime `tempo` command does not track a dense command stream
//! either, which is why the stretcher is implemented here instead.

use super::super::config::{DeviceAudioPoint, SpeedPoint, TrimSegment};
use super::ExportAudioSource;
use super::time_map::{
    OutputTimeMapper, get_speed, source_project_start_time, source_timeline_end_time,
    volume_points_in_output_time,
};

/// Speed clamp used by `OutputTimeMapper`; mirrored here so the tempo and the output
/// times it is derived from come from the same curve.
const SPEED_MIN: f64 = 0.1;
const SPEED_MAX: f64 = 16.0;

/// `atempo` produces nothing for very short inputs (~50ms single, ~150ms chained).
/// Well below any real clip, so a source this short just takes the stretcher.
const MIN_ATEMPO_SOURCE_SEC: f64 = 0.5;

const FLAT_SPEED_EPSILON: f64 = 1e-4;
const TIME_EPSILON: f64 = 1e-6;

pub(super) struct ConstantTempoRetime {
    /// Window to read from the decoded source, in source-internal seconds.
    pub(super) source_start: f64,
    pub(super) source_end: f64,
    /// Playback-rate multiplier: >1 plays faster.
    pub(super) tempo: f64,
    /// Where the retimed audio begins on the output timeline.
    pub(super) output_start: f64,
    /// Exact length the retimed audio must occupy on the output timeline.
    pub(super) output_duration: f64,
    /// `source.volume_points` re-expressed in output time. The retimed audio is
    /// mixed on the output timeline, so a project-time envelope would land wrong.
    pub(super) volume_points: Vec<DeviceAudioPoint>,
}

fn sampled_speed(time: f64, speed_points: &[SpeedPoint]) -> f64 {
    get_speed(time, speed_points).clamp(SPEED_MIN, SPEED_MAX)
}

/// The one speed covering `[span_start, span_end]`, or `None` if the curve moves
/// anywhere inside it.
///
/// Checking both ends plus every control point in between is exact: cosine
/// interpolation between two control points is monotone, so a span whose ends and
/// interior control points all share one speed cannot bulge in between.
fn constant_speed_over(span_start: f64, span_end: f64, speed_points: &[SpeedPoint]) -> Option<f64> {
    let base = sampled_speed(span_start, speed_points);
    if (sampled_speed(span_end, speed_points) - base).abs() > FLAT_SPEED_EPSILON {
        return None;
    }
    let moves_inside = speed_points.iter().any(|point| {
        point.time > span_start - TIME_EPSILON
            && point.time < span_end + TIME_EPSILON
            && (point.speed.clamp(SPEED_MIN, SPEED_MAX) - base).abs() > FLAT_SPEED_EPSILON
    });
    (!moves_inside).then_some(base)
}

/// The stretch of project time this source is actually heard over: its footprint
/// clipped to the one kept region it plays inside.
///
/// Clipping is what lets a track delay work — a delayed source hangs off the end of
/// the timeline (or, for a negative delay, starts before it), and only the part
/// inside the kept region is heard. `None` when a cut falls inside the footprint, or
/// when it is cut away entirely: one `atempo` cannot drop a cut from the middle.
fn uncut_play_span(
    project_start: f64,
    project_end: f64,
    trim_segments: &[TrimSegment],
) -> Option<(f64, f64)> {
    let mut overlapping = trim_segments.iter().filter(|segment| {
        segment.end_time > project_start + TIME_EPSILON
            && segment.start_time < project_end - TIME_EPSILON
    });
    let segment = overlapping.next()?;
    if overlapping.next().is_some() {
        return None;
    }
    let play_start = project_start.max(segment.start_time);
    let play_end = project_end.min(segment.end_time);
    (play_end > play_start + TIME_EPSILON).then_some((play_start, play_end))
}

/// Returns `None` whenever `atempo` cannot reproduce the timeline exactly. Every
/// `None` falls through to the WSOLA stretcher, which follows the speed curve and
/// the trim cuts while preserving pitch.
pub(super) fn build_constant_tempo_retime(
    source: &ExportAudioSource,
    trim_segments: &[TrimSegment],
    speed_points: &[SpeedPoint],
    fallback_duration: f64,
) -> Option<ConstantTempoRetime> {
    let project_start = source_project_start_time(source);
    let project_end = source_timeline_end_time(source, trim_segments, fallback_duration);
    if project_end <= project_start + TIME_EPSILON {
        return None;
    }
    let (play_start, play_end) = uncut_play_span(project_start, project_end, trim_segments)?;
    constant_speed_over(play_start, play_end, speed_points)?;

    let playback_rate = source.playback_rate.max(0.0001);
    let source_start = ((play_start - source.start_offset_sec) * playback_rate).max(0.0);
    let source_end = ((play_end - source.start_offset_sec) * playback_rate).max(0.0);
    if source_end - source_start < MIN_ATEMPO_SOURCE_SEC {
        return None;
    }

    let mut mapper = OutputTimeMapper::new(trim_segments.to_vec(), speed_points.to_vec());
    let output_start = mapper.map_source_time(play_start)?;
    let output_end = mapper.map_source_time(play_end)?;
    let output_duration = output_end - output_start;
    if output_duration <= TIME_EPSILON {
        return None;
    }

    let tempo = (source_end - source_start) / output_duration;
    if (tempo - 1.0).abs() <= FLAT_SPEED_EPSILON {
        return None;
    }

    Some(ConstantTempoRetime {
        source_start,
        source_end,
        tempo,
        output_start,
        output_duration,
        volume_points: volume_points_in_output_time(
            &source.volume_points,
            trim_segments,
            speed_points,
            output_end,
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_source() -> ExportAudioSource {
        ExportAudioSource {
            path: "device.wav".to_string(),
            volume_points: Vec::new(),
            start_offset_sec: 0.0,
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
    fn flat_unity_speed_needs_no_retime() {
        assert!(
            build_constant_tempo_retime(
                &test_source(),
                &trim(&[(0.0, 10.0)]),
                &speed_curve(&[(0.0, 1.0), (10.0, 1.0)]),
                10.0,
            )
            .is_none()
        );
    }

    #[test]
    fn constant_speed_becomes_one_tempo() {
        let retime = build_constant_tempo_retime(
            &test_source(),
            &trim(&[(0.0, 10.0)]),
            &speed_curve(&[(0.0, 2.0), (10.0, 2.0)]),
            10.0,
        )
        .expect("constant 2x is exactly what atempo does");

        assert!((retime.source_start - 0.0).abs() < 1e-6);
        assert!((retime.source_end - 10.0).abs() < 1e-6);
        assert!((retime.tempo - 2.0).abs() < 1e-3);
        assert!((retime.output_start - 0.0).abs() < 1e-6);
        assert!((retime.output_duration - 5.0).abs() < 1e-3);
    }

    /// The regression this module exists to prevent: a ramp used to be flattened
    /// into one average tempo, which matched the total duration and nothing else.
    #[test]
    fn speed_ramp_is_refused_so_the_stretcher_follows_the_curve() {
        assert!(
            build_constant_tempo_retime(
                &test_source(),
                &trim(&[(0.0, 4.0)]),
                &speed_curve(&[(0.0, 1.0), (4.0, 4.0)]),
                4.0,
            )
            .is_none()
        );
    }

    #[test]
    fn stepped_speed_levels_are_refused() {
        assert!(
            build_constant_tempo_retime(
                &test_source(),
                &trim(&[(0.0, 12.0)]),
                &speed_curve(&[(0.0, 1.0), (4.0, 1.0), (4.5, 3.0), (12.0, 3.0)]),
                12.0,
            )
            .is_none()
        );
    }

    #[test]
    fn a_ramp_that_returns_to_its_starting_speed_is_still_refused() {
        assert!(
            build_constant_tempo_retime(
                &test_source(),
                &trim(&[(0.0, 8.0)]),
                &speed_curve(&[(0.0, 2.0), (3.0, 2.0), (4.0, 6.0), (5.0, 2.0), (8.0, 2.0)]),
                8.0,
            )
            .is_none(),
            "the 6x peak sits inside the span and must be detected"
        );
    }

    #[test]
    fn a_cut_inside_the_source_is_refused() {
        assert!(
            build_constant_tempo_retime(
                &test_source(),
                &trim(&[(0.0, 2.0), (6.0, 10.0)]),
                &speed_curve(&[(0.0, 2.0), (10.0, 2.0)]),
                10.0,
            )
            .is_none(),
            "one atempo cannot drop the 2s..6s cut"
        );
    }

    #[test]
    fn a_clip_inside_one_kept_region_is_accepted() {
        let mut source = test_source();
        source.start_offset_sec = 7.0;
        source.source_out_sec = Some(2.0);

        let retime = build_constant_tempo_retime(
            &source,
            &trim(&[(0.0, 2.0), (6.0, 10.0)]),
            &speed_curve(&[(0.0, 2.0), (10.0, 2.0)]),
            10.0,
        )
        .expect("the clip sits wholly inside the second kept region");

        // Project 7s..9s at 2x, after 2s of source (1s of output) was kept.
        assert!((retime.output_start - 1.5).abs() < 1e-3);
        assert!((retime.output_duration - 1.0).abs() < 1e-3);
        assert!((retime.tempo - 2.0).abs() < 1e-3);
    }

    #[test]
    fn clip_offset_and_playback_rate_fold_into_the_tempo() {
        let mut source = test_source();
        source.start_offset_sec = 4.0;
        source.playback_rate = 2.0;
        source.source_out_sec = Some(8.0);

        let retime = build_constant_tempo_retime(
            &source,
            &trim(&[(0.0, 12.0)]),
            &speed_curve(&[(0.0, 2.0), (12.0, 2.0)]),
            12.0,
        )
        .expect("offset clip at constant speed");

        // Clip occupies project 4s..8s, i.e. source-internal 0s..8s at rate 2.
        assert!((retime.source_start - 0.0).abs() < 1e-6);
        assert!((retime.source_end - 8.0).abs() < 1e-6);
        assert!((retime.output_start - 2.0).abs() < 1e-3);
        assert!((retime.output_duration - 2.0).abs() < 1e-3);
        assert!((retime.tempo - 4.0).abs() < 1e-3, "tempo {}", retime.tempo);
    }

    /// A positive track delay pushes the source past the end of the timeline. Only
    /// the part still inside the kept region is heard, and the tail is clipped.
    #[test]
    fn a_delayed_device_track_keeps_the_pitch_preserved_path() {
        let mut source = test_source();
        source.start_offset_sec = 0.75;

        let retime = build_constant_tempo_retime(
            &source,
            &trim(&[(0.0, 10.0)]),
            &speed_curve(&[(0.0, 2.0), (10.0, 2.0)]),
            10.0,
        )
        .expect("a delayed track at constant speed is still one tempo");

        // Source-internal 0s..9.25s: the 0.75s hanging off the end is clipped.
        assert!((retime.source_start - 0.0).abs() < 1e-6);
        assert!((retime.source_end - 9.25).abs() < 1e-6);
        // The delay itself is on the timeline, so it compresses with everything
        // else: 0.75s of project time at 2x lands the audio at output 0.375s.
        assert!((retime.output_start - 0.375).abs() < 1e-3);
        assert!((retime.output_duration - 4.625).abs() < 1e-3);
        assert!((retime.tempo - 2.0).abs() < 1e-3);
    }

    /// A negative delay pulls the source in front of the timeline; its head is the
    /// part that gets clipped.
    #[test]
    fn a_negative_track_delay_clips_the_head_instead() {
        let mut source = test_source();
        source.start_offset_sec = -0.5;

        let retime = build_constant_tempo_retime(
            &source,
            &trim(&[(0.0, 10.0)]),
            &speed_curve(&[(0.0, 2.0), (10.0, 2.0)]),
            10.0,
        )
        .expect("a negatively delayed track is still one tempo");

        // Pulled 0.5s earlier, so the timeline's last instant wants source 10.5s.
        // The track plays to the end of the timeline, not to `duration` — reading
        // past the file just drains the decoder and pads, whereas stopping at
        // `start_offset + duration` would end the audio a quarter-second early.
        assert!((retime.source_start - 0.5).abs() < 1e-6);
        assert!((retime.source_end - 10.5).abs() < 1e-6);
        assert!((retime.output_start - 0.0).abs() < 1e-6);
        assert!((retime.output_duration - 5.0).abs() < 1e-3);
        assert!((retime.tempo - 2.0).abs() < 1e-3);
    }

    #[test]
    fn a_delayed_track_over_a_ramp_still_falls_back_to_the_stretcher() {
        let mut source = test_source();
        source.start_offset_sec = 0.75;
        assert!(
            build_constant_tempo_retime(
                &source,
                &trim(&[(0.0, 4.0)]),
                &speed_curve(&[(0.0, 1.0), (4.0, 4.0)]),
                4.0,
            )
            .is_none()
        );
    }

    #[test]
    fn a_source_too_short_for_atempo_is_refused() {
        let mut source = test_source();
        source.source_out_sec = Some(0.2);
        assert!(
            build_constant_tempo_retime(
                &source,
                &trim(&[(0.0, 10.0)]),
                &speed_curve(&[(0.0, 2.0), (10.0, 2.0)]),
                10.0,
            )
            .is_none()
        );
    }

    #[test]
    fn volume_envelope_moves_onto_the_output_timeline() {
        let mut source = test_source();
        source.volume_points = vec![
            DeviceAudioPoint {
                time: 0.0,
                volume: 1.0,
            },
            DeviceAudioPoint {
                time: 5.0,
                volume: 0.5,
            },
            DeviceAudioPoint {
                time: 10.0,
                volume: 1.0,
            },
        ];
        let retime = build_constant_tempo_retime(
            &source,
            &trim(&[(0.0, 10.0)]),
            &speed_curve(&[(0.0, 2.0), (10.0, 2.0)]),
            10.0,
        )
        .unwrap();

        let times: Vec<f64> = retime
            .volume_points
            .iter()
            .map(|point| point.time)
            .collect();
        assert!((times[0] - 0.0).abs() < 1e-3);
        assert!((times[1] - 2.5).abs() < 1e-3, "mid point at {}", times[1]);
        assert!((times[2] - 5.0).abs() < 1e-3, "end point at {}", times[2]);
    }
}
