use super::*;

struct TempWav(std::path::PathBuf);

impl Drop for TempWav {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn write_wav(channels: u16, sample_rate: u32, frames: usize) -> (TempWav, Vec<[f32; 2]>) {
    let mut random = [0u8; 8];
    getrandom::fill(&mut random).unwrap();
    let path = std::env::temp_dir().join(format!(
        "sgt-wav-fast-{}-{}.wav",
        std::process::id(),
        u64::from_ne_bytes(random)
    ));
    let spec = hound::WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&path, spec).unwrap();
    let mut expected = Vec::with_capacity(frames);
    for index in 0..frames {
        let left = ((index * 37) % 2000) as i16 - 1000;
        let right = ((index * 11) % 3000) as i16 - 1500;
        writer.write_sample(left).unwrap();
        if channels == 2 {
            writer.write_sample(right).unwrap();
            expected.push([left as f32 / 32768.0, right as f32 / 32768.0]);
        } else {
            expected.push([left as f32 / 32768.0; 2]);
        }
    }
    writer.finalize().unwrap();
    (TempWav(path), expected)
}

/// The previous whole-file implementation, kept as the reference.
fn reference_chunks(source: &[[f32; 2]], rate: u32) -> Vec<(f64, Vec<f32>)> {
    let output_frames = output_frame_count(source.len(), rate);
    let mut chunks = Vec::new();
    let mut samples = Vec::new();
    let mut chunk_start = 0;
    for output_frame in 0..output_frames {
        let pos = output_frame as f64 * rate as f64 / MIX_OUTPUT_SAMPLE_RATE as f64;
        let left_idx = pos.floor() as usize;
        let right_idx = (left_idx + 1).min(source.len() - 1);
        let t = (pos - left_idx as f64) as f32;
        let (left, right) = (source[left_idx], source[right_idx]);
        samples.push(left[0] + (right[0] - left[0]) * t);
        samples.push(left[1] + (right[1] - left[1]) * t);
        if (output_frame + 1).is_multiple_of(WAV_FAST_CHUNK_FRAMES) {
            chunks.push((
                chunk_start as f64 / MIX_OUTPUT_SAMPLE_RATE as f64,
                std::mem::take(&mut samples),
            ));
            chunk_start = output_frame + 1;
        }
    }
    if !samples.is_empty() {
        chunks.push((chunk_start as f64 / MIX_OUTPUT_SAMPLE_RATE as f64, samples));
    }
    chunks
}

fn drain(stream: &mut WavChunkStream) -> Vec<(f64, Vec<f32>)> {
    std::iter::from_fn(|| stream.next_chunk().unwrap())
        .map(|chunk| (chunk.decoded_time, chunk.samples))
        .collect()
}

#[test]
fn streamed_chunks_match_the_whole_file_resampler() {
    for (channels, rate) in [(2, 44_100), (1, 16_000), (2, 48_000)] {
        let (wav, source) = write_wav(channels, rate, rate as usize * 3 / 2 + 17);
        let mut stream = read_wav_fast_chunks(&wav.0.to_string_lossy())
            .unwrap()
            .unwrap();
        assert_eq!(drain(&mut stream), reference_chunks(&source, rate));
    }
}

#[test]
fn forward_seek_skips_whole_chunks_and_matches_the_reference() {
    let rate = 44_100;
    let (wav, source) = write_wav(2, rate, rate as usize * 4);
    let reference = reference_chunks(&source, rate);
    let mut stream = read_wav_fast_chunks(&wav.0.to_string_lossy())
        .unwrap()
        .unwrap();
    stream.next_chunk().unwrap();
    stream.seek(2.5);
    let remaining = drain(&mut stream);
    let first = reference
        .iter()
        .position(|(time, samples)| {
            time + (samples.len() / 2) as f64 / MIX_OUTPUT_SAMPLE_RATE as f64 > 2.5
        })
        .unwrap();
    assert_eq!(remaining, reference[first..].to_vec());
}

#[test]
fn non_wav_paths_and_unsupported_layouts_use_the_decoder_path() {
    assert!(read_wav_fast_chunks("clip.mp3").unwrap().is_none());
}
