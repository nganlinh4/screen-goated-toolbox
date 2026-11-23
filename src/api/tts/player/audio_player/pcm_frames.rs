//! Pure conversion from provider PCM bytes to the player's stereo deck frames.

/// Decodes little-endian 16-bit mono PCM; a trailing odd byte is ignored.
pub(super) fn decode_pcm16_le(audio_data: &[u8]) -> impl Iterator<Item = i16> + '_ {
    audio_data
        .as_chunks::<2>()
        .0
        .iter()
        .map(|chunk| i16::from_le_bytes(*chunk))
}

/// Duplicates each mono sample into a stereo frame at `volume`, saturating
/// instead of wrapping when the gain pushes a sample out of range.
pub(super) fn scaled_stereo_frames(
    samples: impl IntoIterator<Item = i16>,
    volume: f32,
) -> Vec<i16> {
    samples
        .into_iter()
        .flat_map(|sample| {
            let scaled = (sample as f32 * volume).clamp(-32768.0, 32767.0) as i16;
            [scaled, scaled]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mono_pcm_becomes_scaled_stereo_frames() {
        let bytes = [0x00, 0x40, 0x00, 0xC0, 0x7F];
        let samples: Vec<i16> = decode_pcm16_le(&bytes).collect();
        assert_eq!(samples, vec![0x4000, -0x4000]);
        assert_eq!(
            scaled_stereo_frames(samples.iter().copied(), 0.5),
            vec![0x2000, 0x2000, -0x2000, -0x2000]
        );
    }

    #[test]
    fn gain_above_unity_saturates_instead_of_wrapping() {
        assert_eq!(
            scaled_stereo_frames([30_000, -30_000], 2.0),
            vec![i16::MAX, i16::MAX, i16::MIN, i16::MIN]
        );
        assert!(scaled_stereo_frames([], 1.0).is_empty());
    }
}
