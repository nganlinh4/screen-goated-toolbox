//! Channel layout and sample-rate conversion for the Symphonia decode path.
//!
//! Downmixing follows the speaker positions of the decoded layout, so a 5.1
//! source keeps its center and surround channels in stereo instead of being
//! truncated to its first two channels. Resampling keeps its phase and the
//! last input frame across packets, so a long file does not drift from the
//! per-packet rounding of independent resamples.

use std::f32::consts::FRAC_1_SQRT_2;

use symphonia::core::audio::{Channels, Position};

const LEFT_SURROUNDS: Position = Position::REAR_LEFT
    .union(Position::FRONT_LEFT_CENTER)
    .union(Position::SIDE_LEFT)
    .union(Position::TOP_FRONT_LEFT)
    .union(Position::TOP_REAR_LEFT)
    .union(Position::TOP_SIDE_LEFT)
    .union(Position::BOTTOM_FRONT_LEFT)
    .union(Position::FRONT_LEFT_WIDE);
const RIGHT_SURROUNDS: Position = Position::REAR_RIGHT
    .union(Position::FRONT_RIGHT_CENTER)
    .union(Position::SIDE_RIGHT)
    .union(Position::TOP_FRONT_RIGHT)
    .union(Position::TOP_REAR_RIGHT)
    .union(Position::TOP_SIDE_RIGHT)
    .union(Position::BOTTOM_FRONT_RIGHT)
    .union(Position::FRONT_RIGHT_WIDE);
const LOW_FREQUENCY: Position = Position::LFE1.union(Position::LFE2);

/// Speaker position of each interleaved channel, in buffer order.
///
/// Positioned layouts use Symphonia's canonical order (ascending position
/// bits). A discrete layout is read in the standard WAVE order, which is the
/// same bit order; anything else has no known positions.
pub(super) fn channel_positions(channels: &Channels) -> Option<Vec<Position>> {
    let bits = match channels {
        Channels::Positioned(positions) => positions.bits(),
        Channels::Discrete(1) => Position::FRONT_CENTER.bits(),
        Channels::Discrete(count) if (2..=18).contains(count) => (1_u64 << count) - 1,
        _ => return None,
    };
    Some(
        (0..u64::BITS)
            .map(|bit| 1_u64 << bit)
            .filter(|bit| bits & bit != 0)
            .map(Position::from_bits_retain)
            .collect(),
    )
}

/// Left/right gains for one source speaker in an ITU-style stereo downmix.
fn stereo_gains(position: Position) -> (f32, f32) {
    if position == Position::FRONT_LEFT {
        (1.0, 0.0)
    } else if position == Position::FRONT_RIGHT {
        (0.0, 1.0)
    } else if LOW_FREQUENCY.contains(position) {
        (0.0, 0.0)
    } else if LEFT_SURROUNDS.contains(position) {
        (FRAC_1_SQRT_2, 0.0)
    } else if RIGHT_SURROUNDS.contains(position) {
        (0.0, FRAC_1_SQRT_2)
    } else {
        // Front, rear, top and bottom centers feed both sides equally.
        (FRAC_1_SQRT_2, FRAC_1_SQRT_2)
    }
}

/// Stereo downmix matrix, each output row normalized so a full-scale signal
/// on every source speaker cannot clip.
fn stereo_matrix(positions: &[Position]) -> Vec<(f32, f32)> {
    let gains: Vec<(f32, f32)> = positions.iter().copied().map(stereo_gains).collect();
    let left_sum: f32 = gains.iter().map(|gain| gain.0).sum();
    let right_sum: f32 = gains.iter().map(|gain| gain.1).sum();
    let left_scale = if left_sum > 1.0 {
        left_sum.recip()
    } else {
        1.0
    };
    let right_scale = if right_sum > 1.0 {
        right_sum.recip()
    } else {
        1.0
    };
    gains
        .into_iter()
        .map(|(left, right)| (left * left_scale, right * right_scale))
        .collect()
}

/// Convert interleaved frames from the source layout to `target_channels`.
pub(super) fn remap_channels(
    samples: &[f32],
    source_positions: Option<&[Position]>,
    source_channels: usize,
    target_channels: usize,
) -> Vec<f32> {
    if source_channels == target_channels {
        return samples.to_vec();
    }
    let matrix = source_positions
        .filter(|positions| positions.len() == source_channels)
        .filter(|_| target_channels <= 2 && source_channels > target_channels)
        .map(stereo_matrix);

    let frame_count = samples.len() / source_channels;
    let mut remapped = Vec::with_capacity(frame_count * target_channels);
    for frame in samples.chunks_exact(source_channels) {
        if let Some(matrix) = &matrix {
            let (left, right) = frame
                .iter()
                .zip(matrix)
                .fold((0.0, 0.0), |(left, right), (sample, gain)| {
                    (left + sample * gain.0, right + sample * gain.1)
                });
            if target_channels == 1 {
                remapped.push((left + right) * 0.5);
            } else {
                remapped.extend_from_slice(&[left, right]);
            }
            continue;
        }
        match (source_channels, target_channels) {
            (_, 1) => remapped.push(frame.iter().sum::<f32>() / frame.len() as f32),
            (1, dst) => remapped.extend(std::iter::repeat_n(frame[0], dst)),
            (src, dst) if src >= dst => remapped.extend_from_slice(&frame[..dst]),
            _ => {
                remapped.extend_from_slice(frame);
                let fill = *frame.last().unwrap_or(&0.0);
                remapped.extend(std::iter::repeat_n(fill, target_channels - source_channels));
            }
        }
    }
    remapped
}

/// Linear resampler that carries its phase and last input frame between
/// packets. Positions are exact rationals in units of 1/target_rate source
/// frames, so no rounding accumulates over a long stream.
pub(super) struct LinearResampler {
    channels: usize,
    source_rate: u64,
    target_rate: u64,
    /// Next output position, in 1/target_rate source frames, relative to the
    /// first frame of `carry ++ next input`.
    position: u64,
    carry: Vec<f32>,
}

impl LinearResampler {
    pub(super) fn new(channels: usize, source_rate: u32, target_rate: u32) -> Self {
        Self {
            channels: channels.max(1),
            source_rate: u64::from(source_rate.max(1)),
            target_rate: u64::from(target_rate.max(1)),
            position: 0,
            carry: Vec::new(),
        }
    }

    pub(super) fn matches(&self, channels: usize, source_rate: u32, target_rate: u32) -> bool {
        self.channels == channels.max(1)
            && self.source_rate == u64::from(source_rate.max(1))
            && self.target_rate == u64::from(target_rate.max(1))
    }

    /// Forget the carried frame and phase, as after a seek.
    pub(super) fn reset(&mut self) {
        self.position = 0;
        self.carry.clear();
    }

    pub(super) fn process(&mut self, input: &[f32]) -> Vec<f32> {
        if self.source_rate == self.target_rate {
            return input.to_vec();
        }
        let channels = self.channels;
        let mut buffer = std::mem::take(&mut self.carry);
        buffer.extend_from_slice(&input[..input.len() - input.len() % channels]);
        let frames = (buffer.len() / channels) as u64;
        if frames == 0 {
            return Vec::new();
        }

        let mut output = Vec::new();
        loop {
            let index = self.position / self.target_rate;
            if index + 1 >= frames {
                break;
            }
            let frac = (self.position % self.target_rate) as f32 / self.target_rate as f32;
            let start = index as usize * channels;
            for channel in 0..channels {
                let from = buffer[start + channel];
                let to = buffer[start + channels + channel];
                output.push(from + (to - from) * frac);
            }
            self.position += self.source_rate;
        }

        // The last frame becomes frame 0 of the next packet's buffer.
        self.position -= (frames - 1) * self.target_rate;
        let last = (frames as usize - 1) * channels;
        self.carry = buffer[last..last + channels].to_vec();
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn five_one() -> Vec<Position> {
        channel_positions(&Channels::Discrete(6)).expect("WAVE 5.1 layout")
    }

    #[test]
    fn surround_downmix_keeps_center_and_surround_channels() {
        // FL FR FC LFE RL RR with only the center speaker sounding.
        let center_only = [0.0, 0.0, 0.8, 0.0, 0.0, 0.0];
        let stereo = remap_channels(&center_only, Some(&five_one()), 6, 2);
        assert_eq!(stereo.len(), 2);
        assert!(
            stereo[0] > 0.2 && (stereo[0] - stereo[1]).abs() < 1e-6,
            "{stereo:?}"
        );

        // A rear-left-only signal lands on the left side only.
        let rear_left = [0.0, 0.0, 0.0, 0.0, 0.8, 0.0];
        let stereo = remap_channels(&rear_left, Some(&five_one()), 6, 2);
        assert!(stereo[0] > 0.2 && stereo[1] == 0.0, "{stereo:?}");

        // LFE is not folded into the main channels, and nothing can clip.
        let lfe = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0];
        assert_eq!(
            remap_channels(&lfe, Some(&five_one()), 6, 2),
            vec![0.0, 0.0]
        );
        let full = [1.0; 6];
        let stereo = remap_channels(&full, Some(&five_one()), 6, 2);
        assert!(
            stereo.iter().all(|sample| *sample <= 1.0 + 1e-6),
            "{stereo:?}"
        );

        let mono = remap_channels(&center_only, Some(&five_one()), 6, 1);
        assert!((mono[0] - stereo_matrix(&five_one())[2].0 * 0.8).abs() < 1e-6);
    }

    #[test]
    fn positioned_layouts_follow_symphonia_canonical_order() {
        let layout = Channels::Positioned(
            Position::FRONT_LEFT | Position::FRONT_RIGHT | Position::SIDE_LEFT,
        );
        assert_eq!(
            channel_positions(&layout),
            Some(vec![
                Position::FRONT_LEFT,
                Position::FRONT_RIGHT,
                Position::SIDE_LEFT
            ])
        );
        assert_eq!(channel_positions(&Channels::Ambisonic(1)), None);
    }

    #[test]
    fn resampling_across_packets_matches_one_continuous_pass() {
        // 100 AAC-sized packets of a 44.1 kHz ramp resampled to 48 kHz.
        let packets = 100;
        let packet_frames = 1024;
        let source: Vec<f32> = (0..packets * packet_frames)
            .flat_map(|frame| {
                let value = frame as f32 / 1000.0;
                [value, -value]
            })
            .collect();

        let mut whole = LinearResampler::new(2, 44_100, 48_000);
        let continuous = whole.process(&source);

        let mut streamed = LinearResampler::new(2, 44_100, 48_000);
        let chunked: Vec<f32> = source
            .chunks(packet_frames * 2)
            .flat_map(|packet| streamed.process(packet))
            .collect();

        let expected_frames = (packets * packet_frames - 1) * 48_000 / 44_100 + 1;
        assert_eq!(chunked.len() / 2, expected_frames);
        assert_eq!(chunked.len(), continuous.len());
        for (a, b) in chunked.iter().zip(&continuous) {
            assert!((a - b).abs() < 1e-4, "{a} vs {b}");
        }
        // A ramp stays a ramp across packet boundaries.
        let step = 1.0 / 1000.0 * 44_100.0 / 48_000.0;
        let (frames, _) = chunked.as_chunks::<2>();
        for pair in frames.windows(2) {
            assert!((pair[1][0] - pair[0][0] - step).abs() < 1e-3);
        }

        streamed.reset();
        assert!(streamed.process(&source[..2]).is_empty());
    }
}
