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

/// Walks one kept region the other way round: the caller advances output time by a
/// fixed step and reads back where that lands in project time.
///
/// The stretcher needs this because it emits a fixed number of output frames per hop
/// and has to know how far along the source to draw the next grain from. Integrating
/// with the same step and midpoint rule as [`OutputTimeMapper`] keeps the stretched
/// audio and the mixer's placement from drifting apart.
pub(super) struct SegmentWalker {
    project_time: f64,
    output_time: f64,
    end_time: f64,
    speed_points: Vec<SpeedPoint>,
}

impl SegmentWalker {
    pub(super) fn new(
        start_time: f64,
        end_time: f64,
        output_time: f64,
        speed_points: Vec<SpeedPoint>,
    ) -> Self {
        Self {
            project_time: start_time,
            output_time,
            end_time,
            speed_points,
        }
    }

    pub(super) fn project_time(&self) -> f64 {
        self.project_time
    }

    /// Advances until output time has moved by `output_delta`. Returns `false` once
    /// the region is used up.
    pub(super) fn advance(&mut self, output_delta: f64) -> bool {
        let target = self.output_time + output_delta;
        while self.output_time < target - 1e-12 {
            if self.project_time >= self.end_time - 1e-12 {
                return false;
            }
            let step_end = (self.project_time + MIXER_INTEGRATION_STEP_SEC).min(self.end_time);
            let mid_time = (self.project_time + step_end) * 0.5;
            let speed = get_speed(mid_time, &self.speed_points).clamp(0.1, 16.0);
            let step_output = (step_end - self.project_time) / speed;
            if self.output_time + step_output >= target {
                self.project_time += (target - self.output_time) * speed;
                self.output_time = target;
                return true;
            }
            self.output_time += step_output;
            self.project_time = step_end;
        }
        true
    }
}

/// Moves a project-time volume envelope onto the output timeline. Points past the
/// last kept region collapse onto `output_end`, which holds the final level.
pub(super) fn volume_points_in_output_time(
    points: &[DeviceAudioPoint],
    trim_segments: &[TrimSegment],
    speed_points: &[SpeedPoint],
    output_end: f64,
) -> Vec<DeviceAudioPoint> {
    if points.is_empty() {
        return Vec::new();
    }
    let mut sorted = points.to_vec();
    sorted.sort_by(|left, right| left.time.total_cmp(&right.time));
    let mut mapper = OutputTimeMapper::new(trim_segments.to_vec(), speed_points.to_vec());
    sorted
        .into_iter()
        .map(|point| DeviceAudioPoint {
            time: mapper.map_source_time(point.time).unwrap_or(output_end),
            volume: point.volume,
        })
        .collect()
}

pub(super) fn source_project_start_time(source: &ExportAudioSource) -> f64 {
    source.start_offset_sec
        + source
            .source_in_sec
            .filter(|value| value.is_finite())
            .unwrap_or(0.0)
            / source.playback_rate.max(0.0001)
}

/// Where a source stops on the project timeline.
///
/// A source with an explicit out-point stops there. One without — device and mic
/// audio, which simply run alongside the recording — plays until the timeline does.
/// It must NOT fall back to the segment duration: that is the *trimmed* length,
/// while trim segments carry absolute source times, so `trimStart + duration` is the
/// real end and `duration` alone comes up short by exactly `trimStart`. The video
/// pipeline walks the segments directly and never had this gap, so getting it wrong
/// here ends the audio early while the picture keeps going.
pub(super) fn source_timeline_end_time(
    source: &ExportAudioSource,
    trim_segments: &[TrimSegment],
    fallback_duration: f64,
) -> f64 {
    match source.source_out_sec.filter(|value| value.is_finite()) {
        Some(out) => source.start_offset_sec + out / source.playback_rate.max(0.0001),
        None => trim_segments
            .last()
            .map(|segment| segment.end_time)
            .unwrap_or(fallback_duration.max(0.0)),
    }
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
