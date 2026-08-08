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

    /// Mixes at an exact frame index. The stretcher emits hops that must butt up
    /// against each other perfectly; rounding each hop's start from a float time
    /// would leave a sample-sized gap or overlap at every seam.
    pub(super) fn mix_f32_at_frame(
        &mut self,
        start_frame: usize,
        samples: &[f32],
        channels: usize,
    ) -> Result<(), String> {
        if samples.is_empty() {
            return Ok(());
        }
        if channels != self.channels {
            return Err(format!(
                "Audio mix channel mismatch: source={channels}, output={}",
                self.channels
            ));
        }
        let start_sample = start_frame.saturating_mul(self.channels);
        let required = start_sample.saturating_add(samples.len());
        if required > self.samples.len() {
            self.samples.resize(required, 0.0);
        }
        for (index, sample) in samples.iter().enumerate() {
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
