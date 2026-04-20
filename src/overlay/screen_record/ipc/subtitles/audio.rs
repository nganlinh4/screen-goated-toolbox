use std::io::Cursor;

use crate::overlay::screen_record::mf_audio::MfAudioDecoder;

use super::types::SubtitleTrimSegment;

pub const MIN_SUBTITLE_DURATION_SEC: f64 = 0.1;

const MF_100NS_PER_SEC: f64 = 10_000_000.0;

pub fn build_trimmed_wav(
    source_path: &str,
    trim_segments: &[SubtitleTrimSegment],
    source_offset_sec: f64,
    apply_offset: bool,
) -> Result<Vec<u8>, String> {
    let decoder = MfAudioDecoder::new_with_output_format(source_path, Some(16_000), Some(1))?;
    let sample_rate = decoder.sample_rate() as f64;
    let channels = decoder.channels().max(1) as usize;
    let mut pcm_samples: Vec<i16> = Vec::new();

    for trim_segment in trim_segments {
        let adjusted_start =
            (trim_segment.start_time + if apply_offset { source_offset_sec } else { 0.0 }).max(0.0);
        let adjusted_end = (trim_segment.end_time
            + if apply_offset { source_offset_sec } else { 0.0 })
        .max(adjusted_start);
        decoder.seek((adjusted_start * MF_100NS_PER_SEC) as i64)?;

        while let Some((bytes, timestamp_100ns)) = decoder.read_samples()? {
            let timestamp_sec = timestamp_100ns as f64 / MF_100NS_PER_SEC;
            let total_float_samples = bytes.len() / 4;
            if total_float_samples == 0 {
                continue;
            }
            let frame_count = total_float_samples / channels;
            let chunk_duration_sec = frame_count as f64 / sample_rate;
            let chunk_end_sec = timestamp_sec + chunk_duration_sec;
            if chunk_end_sec <= adjusted_start {
                continue;
            }
            if timestamp_sec >= adjusted_end {
                break;
            }

            let overlap_start = adjusted_start.max(timestamp_sec);
            let overlap_end = adjusted_end.min(chunk_end_sec);
            if overlap_end <= overlap_start {
                continue;
            }

            let start_frame = ((overlap_start - timestamp_sec) * sample_rate)
                .floor()
                .max(0.0) as usize;
            let end_frame = ((overlap_end - timestamp_sec) * sample_rate)
                .ceil()
                .max(start_frame as f64) as usize;
            let floats = bytes
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect::<Vec<_>>();

            for frame_index in start_frame..end_frame.min(frame_count) {
                let sample = floats[frame_index * channels];
                let clamped = sample.clamp(-1.0, 1.0);
                pcm_samples.push((clamped * i16::MAX as f32) as i16);
            }
        }
    }

    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut cursor = Cursor::new(Vec::new());
    let mut writer =
        hound::WavWriter::new(&mut cursor, spec).map_err(|e| format!("Create WAV writer: {e}"))?;
    for sample in pcm_samples {
        writer
            .write_sample(sample)
            .map_err(|e| format!("Write WAV sample: {e}"))?;
    }
    writer
        .finalize()
        .map_err(|e| format!("Finalize WAV: {e}"))?;
    Ok(cursor.into_inner())
}

pub fn compact_to_source_time(
    compact_time: f64,
    trim_segments: &[SubtitleTrimSegment],
    source_duration: f64,
) -> f64 {
    let mut remaining = compact_time.max(0.0);
    for segment in trim_segments {
        let len = (segment.end_time - segment.start_time).max(0.0);
        if remaining <= len {
            return (segment.start_time + remaining).clamp(0.0, source_duration);
        }
        remaining -= len;
    }
    trim_segments
        .last()
        .map(|segment| segment.end_time)
        .unwrap_or(source_duration)
        .clamp(0.0, source_duration)
}
