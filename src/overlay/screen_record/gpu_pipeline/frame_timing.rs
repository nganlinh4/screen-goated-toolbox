use super::super::native_export::config::{SpeedPoint, TrimSegment};
use super::types::PipelineConfig;

// Single canonical speed sampler, re-exported so existing `frame_timing::get_speed`
// importers (decode_thread, decode_thread_cpu) keep working.
pub(super) use super::super::native_export::config::get_speed;

pub fn build_frame_times(config: &PipelineConfig) -> Vec<f64> {
    frame_times(
        config.framerate,
        config.trim_start,
        config.duration,
        &config.trim_segments,
        &config.speed_points,
    )
}

/// Source time of every output frame: walks the kept segments, advancing by
/// one output frame scaled by the speed curve.
///
/// The audio places every kept segment back to back on a continuous output
/// timeline, so a frame step that runs past a segment's end carries its
/// overshoot into the next segment instead of restarting there. Otherwise
/// each cut would add up to a frame of video and the sound would drift ahead.
pub(in crate::overlay::screen_record) fn frame_times(
    framerate: u32,
    trim_start: f64,
    duration: f64,
    trim_segments: &[TrimSegment],
    speed_points: &[SpeedPoint],
) -> Vec<f64> {
    let mut times = Vec::new();
    let out_dt = 1.0 / framerate as f64;

    let trim_segments = if trim_segments.is_empty() {
        vec![TrimSegment {
            start_time: trim_start,
            end_time: trim_start + duration,
        }]
    } else {
        trim_segments.to_vec()
    };

    if trim_segments.is_empty() {
        return times;
    }

    let speed_at = |time: f64| get_speed(time, speed_points).clamp(0.1, 16.0);
    let mut seg_idx = 0usize;
    let mut current_source_time = trim_segments[0].start_time;
    let mut step_speed = speed_at(current_source_time);

    loop {
        while seg_idx < trim_segments.len()
            && current_source_time >= trim_segments[seg_idx].end_time - 1e-9
        {
            let overshoot_output_sec =
                (current_source_time - trim_segments[seg_idx].end_time).max(0.0) / step_speed;
            seg_idx += 1;
            if let Some(next) = trim_segments.get(seg_idx) {
                step_speed = speed_at(next.start_time);
                current_source_time = next.start_time + overshoot_output_sec * step_speed;
            }
        }
        if seg_idx >= trim_segments.len() {
            break;
        }

        times.push(current_source_time);
        step_speed = speed_at(current_source_time);
        current_source_time += step_speed * out_dt;
    }

    times
}
