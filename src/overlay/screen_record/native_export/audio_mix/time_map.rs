use super::super::config::{DeviceAudioPoint, SpeedPoint, TrimSegment};
use super::ExportAudioSource;

const MIXER_INTEGRATION_STEP_SEC: f64 = 0.005;

pub(super) fn normalized_trim_segments(
    trim_start: f64,
    duration: f64,
    trim_segments: &[TrimSegment],
) -> Vec<TrimSegment> {
    if trim_segments.is_empty() {
        return vec![TrimSegment {
            start_time: trim_start,
            end_time: trim_start + duration.max(0.0),
        }];
    }
    trim_segments.to_vec()
}

// Single canonical speed sampler, re-exported so `time_map::get_speed` importers
// (audio_mix) keep working.
pub(super) use super::super::config::get_speed;

pub(super) fn get_audio_volume(time: f64, points: &[DeviceAudioPoint]) -> f64 {
    if points.is_empty() {
        return 1.0;
    }

    let idx = points.partition_point(|point| point.time < time);
    if idx == 0 {
        return points[0].volume.clamp(0.0, 1.0);
    }
    if idx >= points.len() {
        return points.last().unwrap().volume.clamp(0.0, 1.0);
    }

    let left = &points[idx - 1];
    let right = &points[idx];
    let t = (time - left.time) / (right.time - left.time).max(1e-9);
    let cos_t = (1.0 - (t * std::f64::consts::PI).cos()) / 2.0;
    (left.volume + (right.volume - left.volume) * cos_t).clamp(0.0, 1.0)
}

pub(super) fn implicit_edge_fade_multiplier(
    time: f64,
    start_time: f64,
    end_time: f64,
    fade_sec: f64,
) -> f64 {
    if fade_sec <= 0.0 || end_time <= start_time {
        return 1.0;
    }
    let duration = end_time - start_time;
    let fade = fade_sec.min(duration / 2.0).max(0.0);
    if fade <= 0.0 {
        return 1.0;
    }
    if time <= start_time || time >= end_time {
        return 0.0;
    }
    let fade_in = if time - start_time < fade {
        (1.0 - (((time - start_time) / fade) * std::f64::consts::PI).cos()) / 2.0
    } else {
        1.0
    };
    let fade_out = if end_time - time < fade {
        (1.0 - (((end_time - time) / fade) * std::f64::consts::PI).cos()) / 2.0
    } else {
        1.0
    };
    (fade_in * fade_out).clamp(0.0, 1.0)
}

pub(super) fn curve_has_audible_points(points: &[DeviceAudioPoint]) -> bool {
    if points.is_empty() {
        return true;
    }
    points.iter().any(|point| point.volume > 0.0001)
}

pub(super) struct OutputTimeMapper {
    trim_segments: Vec<TrimSegment>,
    speed_points: Vec<SpeedPoint>,
    segment_idx: usize,
    cursor_source_time: f64,
    cursor_output_time: f64,
}

impl OutputTimeMapper {
    pub(super) fn new(trim_segments: Vec<TrimSegment>, speed_points: Vec<SpeedPoint>) -> Self {
        let cursor_source_time = trim_segments
            .first()
            .map(|segment| segment.start_time)
            .unwrap_or(0.0);
        Self {
            trim_segments,
            speed_points,
            segment_idx: 0,
            cursor_source_time,
            cursor_output_time: 0.0,
        }
    }

    pub(super) fn map_source_time(&mut self, target_time: f64) -> Option<f64> {
        if self.trim_segments.is_empty() {
            return Some(0.0);
        }

        while self.segment_idx < self.trim_segments.len() {
            let segment = &self.trim_segments[self.segment_idx];
            if target_time < segment.start_time {
                return Some(self.cursor_output_time);
            }
            if self.cursor_source_time < segment.start_time {
                self.cursor_source_time = segment.start_time;
            }
            if target_time <= self.cursor_source_time {
                return Some(self.cursor_output_time);
            }
            if target_time <= segment.end_time {
                self.integrate_to(target_time);
                return Some(self.cursor_output_time);
            }
            self.integrate_to(segment.end_time);
            self.segment_idx += 1;
            if self.segment_idx < self.trim_segments.len() {
                self.cursor_source_time = self.trim_segments[self.segment_idx].start_time;
            }
        }

        None
    }

    fn integrate_to(&mut self, target_time: f64) {
        while self.cursor_source_time < target_time - 1e-9 {
            let step_end = (self.cursor_source_time + MIXER_INTEGRATION_STEP_SEC).min(target_time);
            let mid_time = (self.cursor_source_time + step_end) * 0.5;
            let speed = get_speed(mid_time, &self.speed_points).clamp(0.1, 16.0);
            self.cursor_output_time += (step_end - self.cursor_source_time) / speed;
            self.cursor_source_time = step_end;
        }
    }
}

pub(super) fn source_project_start_time(source: &ExportAudioSource) -> f64 {
    source.start_offset_sec
        + source
            .source_in_sec
            .filter(|value| value.is_finite())
            .unwrap_or(0.0)
            / source.playback_rate.max(0.0001)
}

pub(super) fn source_project_end_time(source: &ExportAudioSource, fallback_duration: f64) -> f64 {
    let source_out = source
        .source_out_sec
        .filter(|value| value.is_finite())
        .unwrap_or(fallback_duration.max(0.0));
    source.start_offset_sec + source_out / source.playback_rate.max(0.0001)
}

fn output_time_for_project_time(
    project_time: f64,
    trim_segments: &[TrimSegment],
    speed_points: &[SpeedPoint],
) -> Option<f64> {
    OutputTimeMapper::new(trim_segments.to_vec(), speed_points.to_vec())
        .map_source_time(project_time)
}

pub fn calculate_mix_output_duration(
    trim_start: f64,
    duration: f64,
    trim_segments: &[TrimSegment],
    speed_points: &[SpeedPoint],
) -> f64 {
    let normalized = normalized_trim_segments(trim_start, duration, trim_segments);
    let Some(last_end) = normalized.last().map(|segment| segment.end_time) else {
        return 0.0;
    };
    output_time_for_project_time(last_end, &normalized, speed_points).unwrap_or(duration.max(0.0))
}

pub(super) fn average_output_tempo(
    source: &ExportAudioSource,
    trim_segments: &[TrimSegment],
    speed_points: &[SpeedPoint],
    fallback_duration: f64,
) -> Option<(f64, f64)> {
    let project_start = source_project_start_time(source);
    let project_end = source_project_end_time(source, fallback_duration);
    if project_end <= project_start {
        return None;
    }
    let output_start = output_time_for_project_time(project_start, trim_segments, speed_points)?;
    let output_end = output_time_for_project_time(project_end, trim_segments, speed_points)?;
    if output_end <= output_start {
        return None;
    }
    Some((
        (project_end - project_start) / (output_end - output_start),
        output_start,
    ))
}
