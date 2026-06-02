use std::collections::VecDeque;

use super::{MIX_OUTPUT_CHANNELS, MIX_OUTPUT_SAMPLE_RATE};

const WAV_FAST_CHUNK_FRAMES: usize = 8192;

pub(super) struct DecodedAudioChunk {
    pub(super) pcm: Vec<u8>,
    pub(super) decoded_time: f64,
    pub(super) channels: usize,
}

fn f32_samples_to_le_bytes(samples: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(samples.len() * 4);
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

fn push_wav_frame(
    frames: &mut Vec<[f32; 2]>,
    pending: &mut Option<f32>,
    sample: f32,
    channels: u16,
) {
    match channels {
        1 => frames.push([sample, sample]),
        2 => {
            if let Some(left) = pending.take() {
                frames.push([left, sample]);
            } else {
                *pending = Some(sample);
            }
        }
        _ => {}
    }
}

fn wav_frames_to_output_chunks(
    source_frames: &[[f32; 2]],
    source_sample_rate: u32,
) -> VecDeque<DecodedAudioChunk> {
    let mut chunks = VecDeque::new();
    if source_frames.is_empty() || source_sample_rate == 0 {
        return chunks;
    }

    let output_frames = ((source_frames.len() as u64 * MIX_OUTPUT_SAMPLE_RATE as u64)
        .saturating_add(source_sample_rate as u64 - 1)
        / source_sample_rate as u64) as usize;
    let mut chunk_samples =
        Vec::with_capacity(WAV_FAST_CHUNK_FRAMES * MIX_OUTPUT_CHANNELS as usize);
    let mut chunk_start_frame = 0usize;

    let flush_chunk =
        |samples: &mut Vec<f32>, start_frame: usize, chunks: &mut VecDeque<DecodedAudioChunk>| {
            if samples.is_empty() {
                return;
            }
            chunks.push_back(DecodedAudioChunk {
                pcm: f32_samples_to_le_bytes(samples),
                decoded_time: start_frame as f64 / MIX_OUTPUT_SAMPLE_RATE as f64,
                channels: MIX_OUTPUT_CHANNELS as usize,
            });
            samples.clear();
        };

    for output_frame_idx in 0..output_frames {
        let source_pos =
            output_frame_idx as f64 * source_sample_rate as f64 / MIX_OUTPUT_SAMPLE_RATE as f64;
        let left_idx = source_pos.floor() as usize;
        let right_idx = (left_idx + 1).min(source_frames.len() - 1);
        let t = (source_pos - left_idx as f64) as f32;
        let left = source_frames[left_idx];
        let right = source_frames[right_idx];
        chunk_samples.push(left[0] + (right[0] - left[0]) * t);
        chunk_samples.push(left[1] + (right[1] - left[1]) * t);

        if (output_frame_idx + 1).is_multiple_of(WAV_FAST_CHUNK_FRAMES) {
            flush_chunk(&mut chunk_samples, chunk_start_frame, &mut chunks);
            chunk_start_frame = output_frame_idx + 1;
        }
    }
    flush_chunk(&mut chunk_samples, chunk_start_frame, &mut chunks);
    chunks
}

pub(super) fn read_wav_fast_chunks(
    path: &str,
) -> Result<Option<VecDeque<DecodedAudioChunk>>, String> {
    if !path.to_ascii_lowercase().ends_with(".wav") {
        return Ok(None);
    }
    let mut reader =
        hound::WavReader::open(path).map_err(|e| format!("Open WAV fast path: {e}"))?;
    let spec = reader.spec();
    if spec.sample_rate == 0 || !(spec.channels == 1 || spec.channels == 2) {
        return Ok(None);
    }

    let estimated_frames = reader.duration() as usize / spec.channels as usize;
    let mut source_frames = Vec::with_capacity(estimated_frames);
    let mut pending_stereo_sample = None;

    match (spec.sample_format, spec.bits_per_sample) {
        (hound::SampleFormat::Float, 32) => {
            for sample in reader.samples::<f32>() {
                let sample = sample.map_err(|e| format!("Read WAV float sample: {e}"))?;
                push_wav_frame(
                    &mut source_frames,
                    &mut pending_stereo_sample,
                    sample.clamp(-1.0, 1.0),
                    spec.channels,
                );
            }
        }
        (hound::SampleFormat::Int, 16) => {
            for sample in reader.samples::<i16>() {
                let sample = sample.map_err(|e| format!("Read WAV i16 sample: {e}"))?;
                push_wav_frame(
                    &mut source_frames,
                    &mut pending_stereo_sample,
                    sample as f32 / 32768.0,
                    spec.channels,
                );
            }
        }
        (hound::SampleFormat::Int, 24 | 32) => {
            let denom = if spec.bits_per_sample == 24 {
                8_388_608.0
            } else {
                2_147_483_648.0
            };
            for sample in reader.samples::<i32>() {
                let sample = sample.map_err(|e| format!("Read WAV i32 sample: {e}"))?;
                push_wav_frame(
                    &mut source_frames,
                    &mut pending_stereo_sample,
                    (sample as f32 / denom).clamp(-1.0, 1.0),
                    spec.channels,
                );
            }
        }
        _ => return Ok(None),
    }
    Ok(Some(wav_frames_to_output_chunks(
        &source_frames,
        spec.sample_rate,
    )))
}

pub(super) fn fast_retime_f32le(pcm: &[u8], channels: usize, speed: f64) -> Vec<u8> {
    if pcm.is_empty() || channels == 0 {
        return Vec::new();
    }
    let input_frames = pcm.len() / (channels * 4);
    if input_frames == 0 {
        return Vec::new();
    }
    let speed = speed.clamp(0.05, 64.0);
    if (speed - 1.0).abs() <= 0.0001 {
        return pcm.to_vec();
    }

    let output_frames = ((input_frames as f64) / speed).ceil().max(1.0) as usize;
    let mut out = Vec::with_capacity(output_frames * channels * 4);
    for output_frame_idx in 0..output_frames {
        let source_pos = output_frame_idx as f64 * speed;
        let left_frame = source_pos.floor().min((input_frames - 1) as f64) as usize;
        let right_frame = (left_frame + 1).min(input_frames - 1);
        let t = (source_pos - left_frame as f64) as f32;
        for channel_idx in 0..channels {
            let left_sample_idx = ((left_frame * channels) + channel_idx) * 4;
            let right_sample_idx = ((right_frame * channels) + channel_idx) * 4;
            let left = f32::from_le_bytes(
                pcm[left_sample_idx..left_sample_idx + 4]
                    .try_into()
                    .unwrap(),
            );
            let right = f32::from_le_bytes(
                pcm[right_sample_idx..right_sample_idx + 4]
                    .try_into()
                    .unwrap(),
            );
            out.extend_from_slice(&(left + (right - left) * t).to_le_bytes());
        }
    }
    out
}
