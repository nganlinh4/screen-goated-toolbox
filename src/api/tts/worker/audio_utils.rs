use anyhow::{Context, bail};
use std::io::Cursor;
use symphonia::core::codecs::audio::{AudioDecoderOptions, CODEC_ID_NULL_AUDIO};
use symphonia::core::formats::FormatOptions;
use symphonia::core::formats::probe::Hint;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

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
    decode_mp3_inner(
        mp3_data,
        all_samples,
        source_sample_rate,
        is_interrupted,
        false,
    )
    .unwrap_or(true)
}

/// Decode an Edge response without exposing partial samples on failure.
/// Cancellation returns `Ok(None)`; malformed or empty audio is an error.
pub(crate) fn decode_edge_mp3_to_pcm(
    mp3_data: Vec<u8>,
    is_interrupted: impl Fn() -> bool,
) -> anyhow::Result<Option<(Vec<i16>, u32)>> {
    if is_interrupted() {
        return Ok(None);
    }
    let mut samples = Vec::new();
    let mut sample_rate = 24_000;
    if !decode_mp3_inner(
        mp3_data,
        &mut samples,
        &mut sample_rate,
        is_interrupted,
        true,
    )? {
        return Ok(None);
    }
    if samples.is_empty() {
        bail!("Edge TTS returned no decodable audio");
    }
    Ok(Some((samples, sample_rate)))
}

fn decode_mp3_inner(
    mp3_data: Vec<u8>,
    all_samples: &mut Vec<i16>,
    source_sample_rate: &mut u32,
    is_interrupted: impl Fn() -> bool,
    strict: bool,
) -> anyhow::Result<bool> {
    let mss = MediaSourceStream::new(Box::new(Cursor::new(mp3_data)), Default::default());

    let mut hint = Hint::new();
    hint.with_extension("mp3");

    let mut format = match symphonia::default::get_probe().probe(
        &hint,
        mss,
        FormatOptions::default(),
        MetadataOptions::default(),
    ) {
        Ok(p) => p,
        Err(error) if strict => return Err(error).context("probe Edge TTS MP3"),
        Err(_) => return Ok(true),
    };

    let track = match format.tracks().iter().find_map(|track| {
        let params = track.codec_params.as_ref()?.audio()?;
        (params.codec != CODEC_ID_NULL_AUDIO).then_some((track.id, params.clone()))
    }) {
        Some(track) => track,
        None if strict => bail!("Edge TTS MP3 has no audio track"),
        None => return Ok(true),
    };
    let (track_id, codec_params) = track;

    let mut decoder = match symphonia::default::get_codecs()
        .make_audio_decoder(&codec_params, &AudioDecoderOptions::default())
    {
        Ok(d) => d,
        Err(error) if strict => return Err(error).context("initialize Edge TTS MP3 decoder"),
        Err(_) => return Ok(true),
    };

    loop {
        if is_interrupted() {
            return Ok(false);
        }

        let packet = match format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            // EOF (and any other read error) ends decoding, matching the previous
            // `minimp3::Error::Eof => break` / `Err(_) => break` behavior.
            Err(error) if strict => return Err(error).context("read Edge TTS MP3 packet"),
            Err(_) => break,
        };

        if packet.track_id != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(error) if strict => return Err(error).context("decode Edge TTS MP3 packet"),
            Err(_) => continue,
        };

        let spec = decoded.spec();
        *source_sample_rate = spec.rate();
        let mut samples = vec![0_i16; decoded.samples_interleaved()];
        decoded.copy_to_slice_interleaved(&mut samples);

        if spec.channels().count() == 2 {
            for chunk in samples.as_chunks::<2>().0 {
                let sample = ((chunk[0] as i32 + chunk[1] as i32) / 2) as i16;
                all_samples.push(sample);
            }
        } else {
            all_samples.extend_from_slice(&samples);
        }
    }

    Ok(true)
}

/// Linear-interpolation resampler for mono PCM16 by source/target rate. Shared by
/// the TTS workers and the TTS playground; delegates to the canonical
/// [`crate::api::audio::resample_linear_i16`] (also used by realtime capture).
pub(crate) fn resample_audio(samples: &[i16], from_rate: u32, to_rate: u32) -> Vec<i16> {
    crate::api::audio::resample_linear_i16(samples, to_rate as f64 / from_rate as f64)
}

#[cfg(test)]
#[path = "audio_utils_tests.rs"]
mod tests;
