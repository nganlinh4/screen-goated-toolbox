use std::collections::VecDeque;

const OUTPUT_FRAME_SAMPLES: usize = 2400;
const OUTPUT_PREROLL_SAMPLES: usize = 4800;
const OUTPUT_MIN_SAMPLES: usize = 7200;
const OUTPUT_END_SILENCE_FRAMES: usize = 3;

#[derive(Clone)]
pub(super) struct OutputRegion {
    pub(super) index: usize,
    pub(super) start_sample: usize,
    pub(super) end_sample: usize,
    pub(super) samples: Vec<i16>,
}

pub(super) struct OutputVad {
    pending: Vec<i16>,
    preroll: VecDeque<(usize, i16)>,
    active: Vec<i16>,
    active_start_sample: usize,
    silence_frames: usize,
    cursor_sample: usize,
    noise_floor: f32,
    next_index: usize,
}
impl OutputVad {
    pub(super) fn new() -> Self {
        Self {
            pending: Vec::new(),
            preroll: VecDeque::new(),
            active: Vec::new(),
            active_start_sample: 0,
            silence_frames: 0,
            cursor_sample: 0,
            noise_floor: 0.003,
            next_index: 0,
        }
    }

    pub(super) fn push(&mut self, samples: &[i16]) -> Vec<OutputRegion> {
        self.pending.extend_from_slice(samples);
        let mut output = Vec::new();
        while self.pending.len() >= OUTPUT_FRAME_SAMPLES {
            let frame_start = self.cursor_sample;
            let frame = self
                .pending
                .drain(..OUTPUT_FRAME_SAMPLES)
                .collect::<Vec<_>>();
            self.cursor_sample += frame.len();
            if let Some(region) = self.process_frame(frame_start, frame) {
                output.push(region);
            }
        }
        output
    }

    pub(super) fn finish(mut self) -> Option<OutputRegion> {
        if !self.pending.is_empty() {
            let frame_start = self.cursor_sample;
            let frame = std::mem::take(&mut self.pending);
            self.cursor_sample += frame.len();
            if self.active.is_empty() {
                self.active_start_sample = frame_start;
            }
            self.active.extend(frame);
        }
        self.close_active()
    }

    fn process_frame(&mut self, frame_start: usize, frame: Vec<i16>) -> Option<OutputRegion> {
        let rms = calculate_rms(&frame);
        let threshold = (self.noise_floor * 3.0).max(0.004);
        let is_speech = rms >= threshold || rms >= 0.012;
        if !is_speech {
            self.noise_floor = self.noise_floor.mul_add(0.96, rms * 0.04).min(0.02);
        }
        if self.active.is_empty() {
            self.preroll.extend(
                frame
                    .iter()
                    .copied()
                    .enumerate()
                    .map(|(idx, sample)| (frame_start + idx, sample)),
            );
            while self.preroll.len() > OUTPUT_PREROLL_SAMPLES {
                self.preroll.pop_front();
            }
            if is_speech {
                self.active_start_sample = self
                    .preroll
                    .front()
                    .map(|(idx, _)| *idx)
                    .unwrap_or(frame_start);
                self.active
                    .extend(self.preroll.drain(..).map(|(_, sample)| sample));
                self.active.extend(frame);
                self.silence_frames = 0;
            }
            return None;
        }
        self.active.extend(frame);
        self.silence_frames = if is_speech {
            0
        } else {
            self.silence_frames + 1
        };
        if self.active.len() >= OUTPUT_MIN_SAMPLES
            && self.silence_frames >= OUTPUT_END_SILENCE_FRAMES
        {
            self.close_active()
        } else {
            None
        }
    }

    fn close_active(&mut self) -> Option<OutputRegion> {
        if self.active.len() < OUTPUT_MIN_SAMPLES {
            self.active.clear();
            self.preroll.clear();
            self.silence_frames = 0;
            return None;
        }
        let samples = std::mem::take(&mut self.active);
        let start_sample = self.active_start_sample;
        let end_sample = start_sample + samples.len();
        let region = OutputRegion {
            index: self.next_index,
            start_sample,
            end_sample,
            samples,
        };
        self.next_index += 1;
        self.preroll.clear();
        self.silence_frames = 0;
        Some(region)
    }
}
fn calculate_rms(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum = samples
        .iter()
        .map(|sample| {
            let value = *sample as f32 / i16::MAX as f32;
            value * value
        })
        .sum::<f32>();
    (sum / samples.len() as f32).sqrt()
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_vad_closes_speech_regions() {
        let mut vad = OutputVad::new();
        let silence = vec![0i16; OUTPUT_FRAME_SAMPLES * 2];
        let speech = vec![4000i16; OUTPUT_FRAME_SAMPLES * 4];
        assert!(vad.push(&silence).is_empty());
        assert!(vad.push(&speech).is_empty());
        let regions = vad.push(&silence);
        assert_eq!(regions.len(), 1);
        assert!(regions[0].samples.len() >= OUTPUT_MIN_SAMPLES);
    }
}
