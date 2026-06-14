/// Linear-interpolation resampler for mono PCM16. Shared by the TTS workers and
/// the TTS playground; both previously carried byte-identical copies.
pub(crate) fn resample_audio(samples: &[i16], from_rate: u32, to_rate: u32) -> Vec<i16> {
    if samples.is_empty() || from_rate == to_rate {
        return samples.to_vec();
    }

    let ratio = to_rate as f32 / from_rate as f32;
    let new_len = (samples.len() as f32 * ratio) as usize;
    let mut result = Vec::with_capacity(new_len);

    for i in 0..new_len {
        let src_idx_f = i as f32 / ratio;
        let src_idx = src_idx_f as usize;

        if src_idx >= samples.len() - 1 {
            result.push(samples[src_idx.min(samples.len() - 1)]);
        } else {
            let t = src_idx_f - src_idx as f32;
            let s1 = samples[src_idx] as f32;
            let s2 = samples[src_idx + 1] as f32;
            let val = s1 + t * (s2 - s1);
            result.push(val as i16);
        }
    }

    result
}
