// Keep the recording's format stable when the default endpoint changes format.
pub(super) struct Converter {
    input_channels: usize,
    output_channels: usize,
    step: f64,
    position: f64,
    frames: Vec<f32>,
}

impl Converter {
    pub(super) fn new(
        input_rate: u32,
        input_channels: usize,
        output_rate: u32,
        output_channels: usize,
    ) -> Self {
        Self {
            input_channels,
            output_channels,
            step: input_rate as f64 / output_rate as f64,
            position: 0.0,
            frames: Vec::new(),
        }
    }

    pub(super) fn convert(&mut self, input: &[f32]) -> Vec<f32> {
        for frame in input.chunks_exact(self.input_channels) {
            for channel in 0..self.output_channels {
                let sample = if self.output_channels == 1 {
                    frame.iter().sum::<f32>() / self.input_channels as f32
                } else if self.input_channels == 1 {
                    frame[0]
                } else {
                    frame.get(channel).copied().unwrap_or(0.0)
                };
                self.frames.push(sample);
            }
        }
        let count = self.frames.len() / self.output_channels;
        let mut output = Vec::new();
        while self.position + 1.0 < count as f64 {
            let left = self.position as usize;
            let fraction = (self.position - left as f64) as f32;
            for channel in 0..self.output_channels {
                let a = self.frames[left * self.output_channels + channel];
                let b = self.frames[(left + 1) * self.output_channels + channel];
                output.push(a + (b - a) * fraction);
            }
            self.position += self.step;
        }
        let consumed = (self.position as usize).min(count.saturating_sub(1));
        self.frames.drain(..consumed * self.output_channels);
        self.position -= consumed as f64;
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packet_boundaries_do_not_change_resampling_or_accumulate_drift() {
        let input: Vec<_> = (0..44_100).map(|i| (i % 100) as f32 / 100.0).collect();
        let whole = Converter::new(44_100, 1, 48_000, 2).convert(&input);
        let mut converter = Converter::new(44_100, 1, 48_000, 2);
        let split: Vec<_> = input
            .chunks(137)
            .flat_map(|packet| converter.convert(packet))
            .collect();
        assert_eq!(whole.len(), split.len());
        assert!(
            whole
                .iter()
                .zip(&split)
                .all(|(a, b)| (a - b).abs() < 0.00001)
        );
        assert!((whole.len() as isize - 96_000).abs() <= 4);
        assert!(
            split
                .as_chunks::<2>()
                .0
                .iter()
                .all(|frame| frame[0] == frame[1])
        );
    }

    #[test]
    fn downmix_averages_channels_and_retains_the_next_packet_boundary() {
        let mut converter = Converter::new(48_000, 2, 48_000, 1);
        assert_eq!(converter.convert(&[0.2, 0.6, 0.4, 0.8]), vec![0.4]);
        assert_eq!(converter.convert(&[0.6, 1.0]), vec![0.6]);
    }
}
