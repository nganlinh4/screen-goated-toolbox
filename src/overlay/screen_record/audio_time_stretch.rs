//! Stateful pitch-preserving tempo change for export audio.
//!
//! This wraps the streaming `timestretch` processor so screen-record export can
//! change playback speed without the chipmunk artifact from naive resampling.
//! The app's speed convention is `> 1.0` = faster, so we invert it before
//! passing the duration ratio into `timestretch`.

use timestretch::{EdmPreset, StreamProcessor, StretchParams};

fn pcm_le_bytes_to_f32(input: &[u8]) -> Option<Vec<f32>> {
    if input.is_empty() || !input.len().is_multiple_of(4) {
        return None;
    }

    let mut output = vec![0.0f32; input.len() / 4];
    unsafe {
        std::ptr::copy_nonoverlapping(input.as_ptr(), output.as_mut_ptr() as *mut u8, input.len());
    }
    Some(output)
}

fn f32_to_pcm_le_bytes(input: &[f32]) -> Vec<u8> {
    let mut output = vec![0u8; input.len() * 4];
    unsafe {
        std::ptr::copy_nonoverlapping(
            input.as_ptr() as *const u8,
            output.as_mut_ptr(),
            output.len(),
        );
    }
    output
}

/// Streaming tempo stretcher used by export audio.
pub(crate) struct ExportAudioStretcher {
    processor: StreamProcessor,
    last_ratio: f64,
}

impl ExportAudioStretcher {
    pub(crate) fn new(sample_rate: u32, channels: usize) -> Self {
        let params = StretchParams::new(1.0)
            .with_sample_rate(sample_rate)
            .with_channels(channels.min(u32::MAX as usize) as u32)
            .with_preset(EdmPreset::DjBeatmatch);
        Self {
            processor: StreamProcessor::new(params),
            last_ratio: 1.0,
        }
    }

    fn speed_to_stretch_ratio(speed: f64) -> Option<f64> {
        if speed <= 0.0 || !speed.is_finite() {
            return None;
        }
        Some((1.0 / speed).clamp(0.0625, 10.0))
    }

    pub(crate) fn stretch(&mut self, input: &[u8], speed: f64) -> Vec<u8> {
        if input.is_empty() {
            return Vec::new();
        }
        let Some(stretch_ratio) = Self::speed_to_stretch_ratio(speed) else {
            return Vec::new();
        };

        let Some(pcm_f32) = pcm_le_bytes_to_f32(input) else {
            return input.to_vec();
        };

        if (stretch_ratio - self.last_ratio).abs() > f64::EPSILON {
            if self.processor.set_stretch_ratio(stretch_ratio).is_err() {
                return input.to_vec();
            }
            self.last_ratio = stretch_ratio;
        }

        let mut output_f32 = Vec::with_capacity(pcm_f32.len() * 2);
        if self
            .processor
            .process_into(&pcm_f32, &mut output_f32)
            .is_err()
        {
            return input.to_vec();
        }
        f32_to_pcm_le_bytes(&output_f32)
    }

    pub(crate) fn finish(&mut self) -> Vec<u8> {
        let mut output_f32 = Vec::new();
        if self.processor.flush_into(&mut output_f32).is_err() {
            return Vec::new();
        }
        self.last_ratio = 1.0;
        f32_to_pcm_le_bytes(&output_f32)
    }
}

#[cfg(test)]
mod tests {
    use super::ExportAudioStretcher;

    #[test]
    fn app_speed_maps_to_inverse_stretch_ratio() {
        assert_eq!(ExportAudioStretcher::speed_to_stretch_ratio(1.0), Some(1.0));
        assert_eq!(ExportAudioStretcher::speed_to_stretch_ratio(2.0), Some(0.5));
        assert_eq!(ExportAudioStretcher::speed_to_stretch_ratio(0.5), Some(2.0));
    }
}
