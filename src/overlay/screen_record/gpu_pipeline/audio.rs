use super::super::native_export::config::DeviceAudioPoint;

pub(super) fn get_device_audio_volume(time: f64, points: &[DeviceAudioPoint]) -> f64 {
    if points.is_empty() {
        return 1.0;
    }

    let idx = points.partition_point(|p| p.time < time);
    if idx == 0 {
        return points[0].volume.clamp(0.0, 1.0);
    }
    if idx >= points.len() {
        return points.last().unwrap().volume.clamp(0.0, 1.0);
    }

    let p1 = &points[idx - 1];
    let p2 = &points[idx];
    let t = (time - p1.time) / (p2.time - p1.time).max(1e-9);
    let cos_t = (1.0 - (t * std::f64::consts::PI).cos()) / 2.0;
    (p1.volume + (p2.volume - p1.volume) * cos_t).clamp(0.0, 1.0)
}

pub(super) fn apply_audio_volume_envelope(
    pcm: &mut [u8],
    source_start_time: f64,
    source_duration_sec: f64,
    channels: usize,
    points: &[DeviceAudioPoint],
) {
    if pcm.is_empty() || channels == 0 {
        return;
    }

    let frames = pcm.len() / (channels * 4);
    if frames == 0 {
        return;
    }

    if points
        .iter()
        .all(|point| (point.volume.clamp(0.0, 1.0) - 1.0).abs() < 0.0001)
    {
        return;
    }

    if let Some(first_point) = points.first() {
        let constant_volume = first_point.volume.clamp(0.0, 1.0) as f32;
        if points
            .iter()
            .all(|point| (point.volume.clamp(0.0, 1.0) - constant_volume as f64).abs() < 0.0001)
        {
            for chunk in pcm.as_chunks_mut::<4>().0 {
                let sample = f32::from_le_bytes(*chunk);
                chunk.copy_from_slice(&(sample * constant_volume).clamp(-1.0, 1.0).to_le_bytes());
            }
            return;
        }
    }

    let frame_time_step = if source_duration_sec <= 0.0 {
        0.0
    } else {
        source_duration_sec / frames as f64
    };

    for frame_idx in 0..frames {
        let sample_time = source_start_time + ((frame_idx as f64) + 0.5) * frame_time_step;
        let volume = get_device_audio_volume(sample_time, points) as f32;
        if (volume - 1.0).abs() < 0.0001 {
            continue;
        }
        for channel_idx in 0..channels {
            let sample_idx = ((frame_idx * channels) + channel_idx) * 4;
            let sample = f32::from_le_bytes(pcm[sample_idx..sample_idx + 4].try_into().unwrap());
            pcm[sample_idx..sample_idx + 4]
                .copy_from_slice(&(sample * volume).clamp(-1.0, 1.0).to_le_bytes());
        }
    }
}

/// Sample frames of a decoded chunk that fall inside the kept segment
/// `[seg_start, seg_end)`. Decoder chunks do not start on trim boundaries, so
/// the chunk is cut on whole sample frames rather than kept or dropped whole.
pub(super) fn frames_inside_segment(
    frames: usize,
    sample_rate: u32,
    chunk_time: f64,
    seg_start: f64,
    seg_end: f64,
) -> std::ops::Range<usize> {
    let rate = f64::from(sample_rate);
    let frame_at = |time: f64| {
        ((time - chunk_time) * rate)
            .round()
            .clamp(0.0, frames as f64)
    };
    let first = frame_at(seg_start) as usize;
    let end = frame_at(seg_end) as usize;
    if end > first { first..end } else { 0..0 }
}

#[cfg(test)]
mod tests {
    use super::frames_inside_segment;

    #[test]
    fn chunks_are_cut_on_sample_frames_at_trim_boundaries() {
        // 1024 frames at 48 kHz from t=0.99 s against a segment ending at 1 s:
        // only the 480 frames before the cut belong to the export.
        assert_eq!(frames_inside_segment(1024, 48_000, 0.99, 0.0, 1.0), 0..480);
        // After a seek the decoder resumes 5 ms before the next segment; the
        // frames from the segment start on are kept, not the whole chunk dropped.
        assert_eq!(
            frames_inside_segment(1024, 48_000, 1.995, 2.0, 3.0),
            240..1024
        );
        assert_eq!(frames_inside_segment(1024, 48_000, 2.5, 2.0, 3.0), 0..1024);
        assert_eq!(frames_inside_segment(1024, 48_000, 3.0, 2.0, 3.0), 0..0);
        assert_eq!(frames_inside_segment(1024, 48_000, 1.0, 2.0, 3.0), 0..0);
    }
}
