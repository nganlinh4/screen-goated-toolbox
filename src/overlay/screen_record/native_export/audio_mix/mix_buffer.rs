use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use super::{MIX_OUTPUT_CHANNELS, MIX_OUTPUT_SAMPLE_RATE};

/// Frames per resident block: about 22 s at 48 kHz, 8 MiB of stereo f32.
const BLOCK_FRAMES: usize = 1 << 20;

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

/// Float mix of the whole output timeline with bounded memory.
///
/// The timeline is split into fixed blocks. One block stays in memory; the
/// others live in a scratch file of raw f32 samples, so a long export holds
/// one block instead of the whole timeline. Sources are mixed one after
/// another in mostly increasing time, so block swaps are rare.
pub(super) struct FloatMixBuffer {
    channels: usize,
    block_samples: usize,
    /// Logical length in samples; it grows when a source runs past the end.
    len: usize,
    block: Vec<f32>,
    block_index: usize,
    block_dirty: bool,
    /// Samples written to the scratch file so far (a multiple of a block).
    spilled_samples: usize,
    spill_path: PathBuf,
    spill: Option<File>,
    audible: bool,
}

impl FloatMixBuffer {
    pub(super) fn new(channels: usize, duration_sec: f64, spill_path: PathBuf) -> Self {
        Self::with_block_frames(channels, duration_sec, spill_path, BLOCK_FRAMES)
    }

    fn with_block_frames(
        channels: usize,
        duration_sec: f64,
        spill_path: PathBuf,
        block_frames: usize,
    ) -> Self {
        let channels = channels.max(1);
        let frames = (duration_sec.max(0.0) * MIX_OUTPUT_SAMPLE_RATE as f64).ceil() as usize;
        let block_samples = block_frames.max(1) * channels;
        Self {
            channels,
            block_samples,
            len: frames.saturating_mul(channels),
            block: vec![0.0; block_samples],
            block_index: 0,
            block_dirty: false,
            spilled_samples: 0,
            spill_path,
            spill: None,
            audible: false,
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
        let mut position = start_frame.saturating_mul(self.channels);
        self.len = self.len.max(position.saturating_add(samples.len()));
        let mut rest = samples;
        while !rest.is_empty() {
            let offset = position % self.block_samples;
            self.load_block(position / self.block_samples)?;
            let take = rest.len().min(self.block_samples - offset);
            for (slot, sample) in self.block[offset..offset + take].iter_mut().zip(rest) {
                *slot += sample;
            }
            self.block_dirty = true;
            position += take;
            rest = &rest[take..];
        }
        self.audible |= samples.iter().any(|sample| sample.abs() > 0.000_001);
        Ok(())
    }

    pub(super) fn has_audio(&self) -> bool {
        self.audible
    }

    pub(super) fn write_wav(&mut self, wav_path: &Path) -> Result<(), String> {
        let mut writer = create_wav_writer(wav_path)?;
        let block_count = self.len.div_ceil(self.block_samples);
        for index in 0..block_count {
            self.load_block(index)?;
            let start = index * self.block_samples;
            let end = self.len.min(start + self.block_samples);
            for sample in &self.block[..end - start] {
                let pcm_i16 = (sample.clamp(-1.0, 1.0) * 32767.0) as i16;
                writer
                    .write_sample(pcm_i16)
                    .map_err(|e| format!("Write mixed WAV sample: {e}"))?;
            }
        }
        writer
            .finalize()
            .map_err(|e| format!("Finalize mixed WAV: {e}"))?;
        Ok(())
    }

    /// Makes `index` the resident block, spilling the current one if it changed.
    fn load_block(&mut self, index: usize) -> Result<(), String> {
        if index == self.block_index {
            return Ok(());
        }
        if self.block_dirty {
            self.store_block()?;
        }
        let start = index * self.block_samples;
        if start < self.spilled_samples {
            let mut bytes = vec![0_u8; self.block_samples * 4];
            let file = self.spill_file()?;
            file.seek(SeekFrom::Start(start as u64 * 4))
                .and_then(|_| file.read_exact(&mut bytes))
                .map_err(|e| format!("Read audio mix scratch: {e}"))?;
            let (words, _) = bytes.as_chunks::<4>();
            for (slot, word) in self.block.iter_mut().zip(words) {
                *slot = f32::from_le_bytes(*word);
            }
        } else {
            self.block.fill(0.0);
        }
        self.block_index = index;
        self.block_dirty = false;
        Ok(())
    }

    fn store_block(&mut self) -> Result<(), String> {
        let start = self.block_index * self.block_samples;
        let bytes: Vec<u8> = self.block.iter().flat_map(|s| s.to_le_bytes()).collect();
        let spilled = self.spilled_samples;
        let file = self.spill_file()?;
        if start > spilled {
            // Blocks never touched between the old end and this one are silence.
            file.set_len(start as u64 * 4)
                .map_err(|e| format!("Extend audio mix scratch: {e}"))?;
        }
        file.seek(SeekFrom::Start(start as u64 * 4))
            .and_then(|_| file.write_all(&bytes))
            .map_err(|e| format!("Write audio mix scratch: {e}"))?;
        self.spilled_samples = spilled.max(start + self.block_samples);
        self.block_dirty = false;
        Ok(())
    }

    fn spill_file(&mut self) -> Result<&mut File, String> {
        if self.spill.is_none() {
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(true)
                .open(&self.spill_path)
                .map_err(|e| format!("Create audio mix scratch: {e}"))?;
            self.spill = Some(file);
        }
        self.spill
            .as_mut()
            .ok_or_else(|| "Audio mix scratch file is unavailable".to_string())
    }
}

impl Drop for FloatMixBuffer {
    fn drop(&mut self) {
        if self.spill.take().is_some() {
            let _ = std::fs::remove_file(&self.spill_path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_wav(path: &Path) -> Vec<i16> {
        hound::WavReader::open(path)
            .unwrap()
            .into_samples::<i16>()
            .map(Result::unwrap)
            .collect()
    }

    #[test]
    fn spilled_blocks_mix_exactly_like_one_in_memory_timeline() {
        let dir = std::env::temp_dir().join(format!("sgt-mix-buffer-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let spill = dir.join("mix.f32");
        let channels = MIX_OUTPUT_CHANNELS as usize;
        let duration = 0.05; // 2400 frames at 48 kHz
        let frames = (duration * MIX_OUTPUT_SAMPLE_RATE as f64).ceil() as usize;

        // Reference: the whole timeline in one Vec.
        let mut reference = vec![0.0_f32; frames * channels];
        let mut add = |start: usize, samples: &[f32]| {
            let end = start * channels + samples.len();
            if end > reference.len() {
                reference.resize(end, 0.0);
            }
            for (slot, sample) in reference[start * channels..].iter_mut().zip(samples) {
                *slot += sample;
            }
        };

        // 100-frame blocks force spills, reloads of spilled blocks, a jump
        // over untouched blocks, and growth past the planned duration.
        let mut mixer = FloatMixBuffer::with_block_frames(channels, duration, spill.clone(), 100);
        let ramp: Vec<f32> = (0..1000 * channels)
            .map(|i| (i as f32 / 4000.0).sin() * 0.4)
            .collect();
        let writes: [(usize, &[f32]); 4] = [
            (0, &ramp),
            (950, &ramp[..300]),
            (2200, &ramp),
            (30, &ramp[..500]),
        ];
        for (start, samples) in writes {
            mixer.mix_f32_at_frame(start, samples, channels).unwrap();
            add(start, samples);
        }
        assert!(spill.exists(), "blocks beyond the resident one must spill");
        assert!(mixer.has_audio());

        let wav = dir.join("mix.wav");
        mixer.write_wav(&wav).unwrap();
        let expected: Vec<i16> = reference
            .iter()
            .map(|sample| (sample.clamp(-1.0, 1.0) * 32767.0) as i16)
            .collect();
        assert_eq!(read_wav(&wav), expected);

        drop(mixer);
        assert!(
            !spill.exists(),
            "the scratch file is removed with the mixer"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn silent_mix_reports_no_audio_and_needs_no_scratch() {
        let spill = std::env::temp_dir().join(format!("sgt-mix-silent-{}.f32", std::process::id()));
        let mut mixer = FloatMixBuffer::new(2, 1.0, spill.clone());
        mixer.mix_f32_at_frame(10, &[0.0; 64], 2).unwrap();
        assert!(!mixer.has_audio());
        assert!(!spill.exists());
    }
}
