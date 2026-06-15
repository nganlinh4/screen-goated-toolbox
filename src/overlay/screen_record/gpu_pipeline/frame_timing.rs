use super::super::native_export::config::TrimSegment;
use super::types::PipelineConfig;

// Single canonical speed sampler, re-exported so existing `frame_timing::get_speed`
// importers (decode_thread, decode_thread_cpu) keep working.
pub(super) use super::super::native_export::config::get_speed;

pub fn build_frame_times(config: &PipelineConfig) -> Vec<f64> {
    let mut times = Vec::new();
    let out_dt = 1.0 / config.framerate as f64;

    let trim_segments = if config.trim_segments.is_empty() {
        vec![TrimSegment {
            start_time: config.trim_start,
            end_time: config.trim_start + config.duration,
        }]
    } else {
        config.trim_segments.clone()
    };

    if trim_segments.is_empty() {
        return times;
    }

    let mut seg_idx = 0usize;
    let mut current_source_time = trim_segments[0].start_time;
    let end_time = trim_segments.last().unwrap().end_time;

    while current_source_time < end_time - 1e-9 {
        while seg_idx < trim_segments.len()
            && current_source_time >= trim_segments[seg_idx].end_time
        {
            seg_idx += 1;
            if seg_idx < trim_segments.len() {
                current_source_time = trim_segments[seg_idx].start_time;
            }
        }
        if seg_idx >= trim_segments.len() {
            break;
        }

        times.push(current_source_time);
        let speed = get_speed(current_source_time, &config.speed_points).clamp(0.1, 16.0);
        current_source_time += speed * out_dt;
    }

    times
}
