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
        Err(_) => return true,
    };

    let track = match format.tracks().iter().find_map(|track| {
        let params = track.codec_params.as_ref()?.audio()?;
        (params.codec != CODEC_ID_NULL_AUDIO).then_some((track.id, params.clone()))
    }) {
        Some(track) => track,
        None => return true,
    };
    let (track_id, codec_params) = track;

    let mut decoder = match symphonia::default::get_codecs()
        .make_audio_decoder(&codec_params, &AudioDecoderOptions::default())
    {
        Ok(d) => d,
        Err(_) => return true,
    };

    loop {
        if is_interrupted() {
            return false;
        }

        let packet = match format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            // EOF (and any other read error) ends decoding, matching the previous
            // `minimp3::Error::Eof => break` / `Err(_) => break` behavior.
            Err(_) => break,
        };

        if packet.track_id != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
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

    true
}

/// Linear-interpolation resampler for mono PCM16 by source/target rate. Shared by
/// the TTS workers and the TTS playground; delegates to the canonical
/// [`crate::api::audio::resample_linear_i16`] (also used by realtime capture).
pub(crate) fn resample_audio(samples: &[i16], from_rate: u32, to_rate: u32) -> Vec<i16> {
    crate::api::audio::resample_linear_i16(samples, to_rate as f64 / from_rate as f64)
}

/// Read a WAV sidecar into i16 samples, returning `(samples, sample_rate)`.
/// `label` names the producer in error messages. With `require_mono`, a non-mono
/// file is rejected. Int >16-bit is down-shifted to 16-bit; Float is
/// clamped/scaled/rounded. Previously copy-pasted across the Magpie, Step-Audio
/// and VieNeu workers (with a hardcoded-shift / missing-wide-int divergence).
pub(crate) fn read_wav_i16(
    path: &std::path::Path,
    label: &str,
    require_mono: bool,
) -> anyhow::Result<(Vec<i16>, u32)> {
    let mut reader = hound::WavReader::open(path)
        .with_context(|| format!("Failed to read {label} WAV '{}'", path.display()))?;
    let spec = reader.spec();
    if require_mono && spec.channels != 1 {
        bail!("{label} WAV must be mono, got {} channels", spec.channels);
    }
    let samples = match spec.sample_format {
        hound::SampleFormat::Int => {
            if spec.bits_per_sample <= 16 {
                reader
                    .samples::<i16>()
                    .collect::<std::result::Result<Vec<_>, _>>()?
            } else {
                reader
                    .samples::<i32>()
                    .map(|sample| {
                        sample.map(|value| {
                            (value >> (spec.bits_per_sample.saturating_sub(16) as u32)) as i16
                        })
                    })
                    .collect::<std::result::Result<Vec<_>, _>>()?
            }
        }
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .map(|sample| {
                sample.map(|value| (value.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16)
            })
            .collect::<std::result::Result<Vec<_>, _>>()?,
    };
    Ok((samples, spec.sample_rate))
}
