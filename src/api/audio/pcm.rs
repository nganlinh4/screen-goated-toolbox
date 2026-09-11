//! Stateful, callback-partition-independent PCM conversion. No gain is applied.
use anyhow::{Result, ensure};

pub(crate) struct PcmConverter {
    channels: usize,
    channel_count: usize,
    channel_sum: f64,
    input_rate: u64,
    output_rate: u64,
    filled: u64,
    weighted: f64,
}

impl PcmConverter {
    pub(crate) fn new(channels: usize, input_rate: u32, output_rate: u32) -> Result<Self> {
        ensure!(
            channels > 0 && input_rate > 0 && output_rate > 0,
            "invalid PCM configuration"
        );
        Ok(Self {
            channels,
            channel_count: 0,
            channel_sum: 0.0,
            input_rate: input_rate.into(),
            output_rate: output_rate.into(),
            filled: 0,
            weighted: 0.0,
        })
    }

    pub(crate) fn reset(&mut self) {
        self.channel_count = 0;
        self.channel_sum = 0.0;
        self.filled = 0;
        self.weighted = 0.0;
    }

    pub(crate) fn push(&mut self, samples: impl Iterator<Item = f64>) -> Vec<i16> {
        let mut output = Vec::new();
        for sample in samples {
            self.channel_sum += if sample.is_finite() {
                sample.clamp(-1.0, 1.0)
            } else {
                0.0
            };
            self.channel_count += 1;
            if self.channel_count != self.channels {
                continue;
            }
            let mono = self.channel_sum / self.channels as f64;
            self.channel_count = 0;
            self.channel_sum = 0.0;
            // Integrate source sample coverage over destination sample intervals.
            // Integer phase retains fractional time across arbitrary callbacks.
            let mut remaining = self.output_rate;
            while remaining > 0 {
                let take = remaining.min(self.input_rate - self.filled);
                self.weighted += mono * take as f64;
                self.filled += take;
                remaining -= take;
                if self.filled == self.input_rate {
                    output.push(
                        (self.weighted / self.input_rate as f64 * 32768.0)
                            .round()
                            .clamp(-32768.0, 32767.0) as i16,
                    );
                    self.filled = 0;
                    self.weighted = 0.0;
                }
            }
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn partitions_preserve_samples_rates_channels_and_levels() {
        for rate in [8000, 16000, 22050, 44100, 48000, 96000] {
            for channels in [1, 2, 6] {
                let input = vec![0.125; rate as usize * channels];
                let expected = PcmConverter::new(channels, rate, 16000)
                    .unwrap()
                    .push(input.iter().copied());
                assert_eq!(expected.len(), 16000);
                assert!(expected.iter().all(|s| *s == 4096));
                for block in [1, 127, 256, 441, 1024] {
                    let mut converter = PcmConverter::new(channels, rate, 16000).unwrap();
                    let actual: Vec<_> = input
                        .chunks(block)
                        .flat_map(|s| converter.push(s.iter().copied()))
                        .collect();
                    assert_eq!(
                        actual, expected,
                        "rate={rate} channels={channels} block={block}"
                    );
                }
            }
        }
    }
    #[test]
    fn signal_partitioning_and_invalid_samples_are_deterministic() {
        let input: Vec<_> = (0..44100).map(|n| (n as f64 * 0.13).sin() * 0.02).collect();
        let expected = PcmConverter::new(1, 44100, 16000)
            .unwrap()
            .push(input.iter().copied());
        let mut converter = PcmConverter::new(1, 44100, 16000).unwrap();
        let actual: Vec<_> = input
            .chunks(256)
            .flat_map(|s| converter.push(s.iter().copied()))
            .collect();
        assert_eq!(actual, expected);
        let mut converter = PcmConverter::new(1, 16000, 16000).unwrap();
        assert_eq!(
            converter.push([f64::NAN, f64::INFINITY, -2.0, 2.0].into_iter()),
            [0, 0, -32768, 32767]
        );
        converter.reset();
        assert_eq!(converter.push([0.0].into_iter()), [0]);
    }
}
