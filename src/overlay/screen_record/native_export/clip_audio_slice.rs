//! Slices project-wide audio tracks into one clip of a multi-clip composition.
//!
//! The sequence timeline lays each clip's kept (trimmed) length end to end, so
//! project audio and its track volume curve live in that compact time. A clip
//! renders in its own source time, where its kept trim segments sit at their
//! absolute times with cuts between them. Every slice is therefore mapped back
//! per kept segment, like `sequenceTimeToClipSourceTime` in
//! `screen-record/src/lib/sequenceTimeline.ts`.

use super::config::{DeviceAudioPoint, ImportedAudioSegmentConfig, TrimSegment};

const MIN_SLICE_SEC: f64 = 0.0001;

/// One kept trim segment: its start on the sequence timeline and in source time.
#[derive(Clone, Copy, Debug)]
struct KeptSpan {
    sequence_start: f64,
    source_start: f64,
    len: f64,
}

/// Where one clip sits on the sequence timeline, segment by kept segment.
pub(crate) struct ClipSequenceWindow {
    spans: Vec<KeptSpan>,
}

impl ClipSequenceWindow {
    /// `trim_segments` empty means the clip keeps `trim_start..trim_start + duration`.
    pub(crate) fn new(
        sequence_start: f64,
        trim_start: f64,
        duration: f64,
        trim_segments: &[TrimSegment],
    ) -> Self {
        let mut kept: Vec<(f64, f64)> = if trim_segments.is_empty() {
            vec![(trim_start, trim_start + duration.max(0.0))]
        } else {
            trim_segments
                .iter()
                .map(|segment| (segment.start_time, segment.end_time))
                .collect()
        };
        kept.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut cursor = sequence_start;
        let spans = kept
            .into_iter()
            .filter(|(start, end)| end - start > MIN_SLICE_SEC)
            .map(|(start, end)| {
                let span = KeptSpan {
                    sequence_start: cursor,
                    source_start: start,
                    len: end - start,
                };
                cursor += span.len;
                span
            })
            .collect();
        Self { spans }
    }
}

/// The parts of each project audio segment that play during this clip, one
/// piece per kept trim segment, placed at the clip source time they play over.
pub(crate) fn slice_audio_for_clip(
    project_segments: &[ImportedAudioSegmentConfig],
    window: &ClipSequenceWindow,
) -> Vec<ImportedAudioSegmentConfig> {
    let mut out = Vec::new();
    for seg in project_segments {
        let trimmed_len = (seg.out_point - seg.in_point).max(0.0);
        if trimmed_len <= 0.0 || seg.raw_audio_path.trim().is_empty() {
            continue;
        }
        let rate = if seg.playback_rate > 0.0001 {
            seg.playback_rate
        } else {
            1.0
        };
        let seg_start = seg.start_time;
        let seg_end = seg_start + trimmed_len / rate;
        for span in &window.spans {
            let overlap_start = seg_start.max(span.sequence_start);
            let overlap_end = seg_end.min(span.sequence_start + span.len);
            if overlap_end - overlap_start <= MIN_SLICE_SEC {
                continue;
            }
            let in_point = seg.in_point + (overlap_start - seg_start).max(0.0) * rate;
            let out_point = in_point + (overlap_end - overlap_start) * rate;
            out.push(ImportedAudioSegmentConfig {
                raw_audio_path: seg.raw_audio_path.clone(),
                duration: seg.duration,
                start_time: span.source_start + (overlap_start - span.sequence_start),
                in_point,
                out_point: out_point.min(seg.out_point),
                volume_points: seg.volume_points.clone(),
                playback_rate: seg.playback_rate,
            });
        }
    }
    out
}

/// The track volume curve over this clip, in clip source time. Each kept
/// segment gets its edge values plus the control points inside it.
pub(crate) fn slice_track_volume_points(
    points: &[DeviceAudioPoint],
    window: &ClipSequenceWindow,
) -> Vec<DeviceAudioPoint> {
    if points.is_empty() {
        return Vec::new();
    }
    let mut sorted = points.to_vec();
    sorted.sort_by(|a, b| a.time.total_cmp(&b.time));
    let mut sliced = Vec::new();
    for span in &window.spans {
        let start = span.sequence_start;
        let end = start + span.len;
        let at_source = |time: f64| span.source_start + (time - start);
        sliced.push(point(span.source_start, volume_at(&sorted, start)));
        sliced.extend(
            sorted
                .iter()
                .filter(|p| p.time > start + MIN_SLICE_SEC && p.time < end - MIN_SLICE_SEC)
                .map(|p| point(at_source(p.time), p.volume)),
        );
        sliced.push(point(at_source(end), volume_at(&sorted, end)));
    }
    sliced
}

fn point(time: f64, volume: f64) -> DeviceAudioPoint {
    DeviceAudioPoint { time, volume }
}

/// Cosine interpolation between control points, as the editor draws the curve.
fn volume_at(points: &[DeviceAudioPoint], time: f64) -> f64 {
    let idx = points.partition_point(|point| point.time < time);
    if idx == 0 {
        return points[0].volume;
    }
    if idx >= points.len() {
        return points[points.len() - 1].volume;
    }
    let left = &points[idx - 1];
    let right = &points[idx];
    let ratio = ((time - left.time) / (right.time - left.time).max(0.0001)).clamp(0.0, 1.0);
    let cos_t = (1.0 - (ratio * std::f64::consts::PI).cos()) / 2.0;
    left.volume + (right.volume - left.volume) * cos_t
}
