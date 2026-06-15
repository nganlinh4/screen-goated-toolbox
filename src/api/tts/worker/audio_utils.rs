use std::io::Cursor;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{CODEC_TYPE_NULL, DecoderOptions};
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// Decode in-memory MP3 bytes to interleaved-then-mono-downmixed PCM16, mirroring
/// the previous `minimp3` decode loop used by the Edge and Google TTS workers.
///
/// - Stereo frames are downmixed to mono via `(L + R) / 2`.
/// - `source_sample_rate` is overwritten with the decoded stream's sample rate
///   (kept as an in-out param to preserve each worker's existing default).
/// - `is_interrupted` is polled before each packet; returning `true` aborts the
///   decode early. The function returns `false` in that case (caller should send
///   `AudioEvent::End` and clear state), and `true` when decoding completes.
pub(crate) fn decode_mp3_to_pcm(
    mp3_data: Vec<u8>,
    all_samples: &mut Vec<i16>,
    source_sample_rate: &mut u32,
    is_interrupted: impl Fn() -> bool,
) -> bool {
    let mss = MediaSourceStream::new(Box::new(Cursor::new(mp3_data)), Default::default());

    let mut hint = Hint::new();
    hint.with_extension("mp3");

    let probed = match symphonia::default::get_probe().format(
        &hint,
        mss,
        &FormatOptions::default(),
        &MetadataOptions::default(),
    ) {
        Ok(p) => p,
        Err(_) => return true,
    };

    let mut format = probed.format;

    let track = match format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
    {
        Some(t) => t,
        None => return true,
    };
    let track_id = track.id;
    let codec_params = track.codec_params.clone();

    let mut decoder =
        match symphonia::default::get_codecs().make(&codec_params, &DecoderOptions::default()) {
            Ok(d) => d,
            Err(_) => return true,
        };

    loop {
        if is_interrupted() {
            return false;
        }

        let packet = match format.next_packet() {
            Ok(p) => p,
            // EOF (and any other read error) ends decoding, matching the previous
            // `minimp3::Error::Eof => break` / `Err(_) => break` behavior.
            Err(_) => break,
        };

        if packet.track_id() != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(_) => continue,
        };

        let spec = *decoded.spec();
        *source_sample_rate = spec.rate;

        let duration = decoded.capacity() as u64;
        let mut sample_buf = SampleBuffer::<i16>::new(duration, spec);
        sample_buf.copy_interleaved_ref(decoded);
        let samples = sample_buf.samples();

        if spec.channels.count() == 2 {
            for chunk in samples.chunks(2) {
                let sample = ((chunk[0] as i32 + chunk[1] as i32) / 2) as i16;
                all_samples.push(sample);
            }
        } else {
            all_samples.extend_from_slice(samples);
        }
    }

    true
}

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
