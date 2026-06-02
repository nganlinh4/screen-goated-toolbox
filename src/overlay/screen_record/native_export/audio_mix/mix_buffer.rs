use std::fs::File;
use std::path::Path;

use super::{MIX_OUTPUT_CHANNELS, MIX_OUTPUT_SAMPLE_RATE};

fn create_wav_writer(
    wav_path: &Path,
) -> Result<hound::WavWriter<std::io::BufWriter<File>>, String> {
    let spec = hound::WavSpec {
        channels: MIX_OUTPUT_CHANNELS as u16,
        sample_rate: MIX_OUTPUT_SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    hound::WavWriter::create(wav_path, spec).map_err(|e| format!("Create mixed WAV: {e}"))
}

pub(super) struct FloatMixBuffer {
    samples: Vec<f32>,
    channels: usize,
}

impl FloatMixBuffer {
    pub(super) fn new(channels: usize, duration_sec: f64) -> Self {
        let frames = (duration_sec.max(0.0) * MIX_OUTPUT_SAMPLE_RATE as f64).ceil() as usize;
        Self {
            samples: vec![0.0; frames.saturating_mul(channels)],
            channels,
        }
    }

    pub(super) fn mix_f32le(
        &mut self,
        output_start_time: f64,
        pcm: &[u8],
        channels: usize,
    ) -> Result<(), String> {
        if pcm.is_empty() || channels == 0 {
            return Ok(());
        }
        if channels != self.channels {
            return Err(format!(
                "Audio mix channel mismatch: source={channels}, output={}",
                self.channels
            ));
        }
        let start_frame = (output_start_time * MIX_OUTPUT_SAMPLE_RATE as f64)
            .round()
            .max(0.0) as usize;
        let start_sample = start_frame.saturating_mul(self.channels);
        let source_samples = pcm.len() / 4;
        let required = start_sample.saturating_add(source_samples);
        if required > self.samples.len() {
            self.samples.resize(required, 0.0);
        }
        for (index, chunk) in pcm.chunks_exact(4).enumerate() {
            let sample = f32::from_le_bytes(chunk.try_into().unwrap());
            self.samples[start_sample + index] += sample;
        }
        Ok(())
    }

    pub(super) fn has_audio(&self) -> bool {
        self.samples.iter().any(|sample| sample.abs() > 0.000_001)
    }

    pub(super) fn write_wav(&self, wav_path: &Path) -> Result<(), String> {
        let mut writer = create_wav_writer(wav_path)?;
        for sample in &self.samples {
            let pcm_i16 = (sample.clamp(-1.0, 1.0) * 32767.0) as i16;
            writer
                .write_sample(pcm_i16)
                .map_err(|e| format!("Write mixed WAV sample: {e}"))?;
        }
        writer
            .finalize()
            .map_err(|e| format!("Finalize mixed WAV: {e}"))?;
        Ok(())
    }
}
