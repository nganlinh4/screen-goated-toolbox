use super::super::native_export::config::DeviceAudioPoint;
use super::super::audio_time_stretch;

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
            for chunk in pcm.chunks_exact_mut(4) {
                let sample = f32::from_le_bytes(chunk.try_into().unwrap());
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

/// Pitch-preserving time stretch for native audio speed changes.
pub(super) fn time_stretch_pcm_bytes(
    input: &[u8],
    speed: f64,
    sample_rate: u32,
    channels: usize,
) -> Vec<u8> {
    audio_time_stretch::time_stretch_pcm_bytes(input, speed, sample_rate, channels)
}
