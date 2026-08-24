//! Audio decoding for the drag-and-drop / paste input path: decode any
//! symphonia-supported container into 16-bit PCM WAV bytes. Kept out of the
//! drag-and-drop routing module (`input_handler`) so that file is pure dispatch.

use std::io::Cursor;
use std::path::Path;

use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::codecs::registry::CodecRegistry;
use symphonia_adapter_libopus::OpusDecoder;

pub(crate) const SUPPORTED_AUDIO_EXTENSIONS: &[&str] = &[
    "wav", "mp3", "flac", "ogg", "m4a", "m4b", "aac", "alac", "aiff", "aif", "opus",
];

pub(crate) fn is_supported_audio_extension(extension: &str) -> bool {
    SUPPORTED_AUDIO_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
}

fn decoder_registry() -> CodecRegistry {
    let mut registry = CodecRegistry::new();
    symphonia::default::register_enabled_codecs(&mut registry);
    registry.register_audio_decoder::<OpusDecoder>();
    registry
}

/// Load an audio file and convert to WAV format using symphonia.
/// Supports: WAV, MP3, FLAC, OGG, AAC, ALAC, AIFF, etc.
pub(crate) fn load_audio_file(path: &Path) -> Option<Vec<u8>> {
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::formats::probe::Hint;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;

    // Open the file
    let file = std::fs::File::open(path).ok()?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    // Create a hint using the file extension
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    // Probe the media source
    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .ok()?;

    // Find the first audio track
    let (track_id, codec_params) = format.tracks().iter().find_map(|track| {
        let params = track.codec_params.as_ref()?.audio()?;
        (params.codec != symphonia::core::codecs::audio::CODEC_ID_NULL_AUDIO)
            .then_some((track.id, params.clone()))
    })?;

    // Get sample rate and channels
    let sample_rate = codec_params.sample_rate.unwrap_or(44100);
    let channels = codec_params
        .channels
        .as_ref()
        .map(|channels| channels.count())
        .unwrap_or(2) as u16;

    // Create decoder
    let mut decoder = decoder_registry()
        .make_audio_decoder(&codec_params, &AudioDecoderOptions::default())
        .ok()?;

    // Decode all samples
    let mut all_samples: Vec<i16> = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            Err(symphonia::core::errors::Error::IoError(ref e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(_) => break,
        };

        if packet.track_id != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(_) => continue,
        };

        let old_len = all_samples.len();
        all_samples.resize(old_len + decoded.samples_interleaved(), 0);
        decoded.copy_to_slice_interleaved(&mut all_samples[old_len..]);
    }

    if all_samples.is_empty() {
        return None;
    }

    // Write to WAV format in memory
    let spec = hound::WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut wav_cursor = Cursor::new(Vec::new());
    {
        let mut writer = hound::WavWriter::new(&mut wav_cursor, spec).ok()?;
        for sample in &all_samples {
            writer.write_sample(*sample).ok()?;
        }
        writer.finalize().ok()?;
    }

    Some(wav_cursor.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use symphonia::core::codecs::audio::AudioCodecParameters;
    use symphonia::core::codecs::audio::well_known::CODEC_ID_OPUS;

    #[test]
    fn advertised_extensions_are_the_exact_core_audio_contract() {
        assert_eq!(
            SUPPORTED_AUDIO_EXTENSIONS,
            [
                "wav", "mp3", "flac", "ogg", "m4a", "m4b", "aac", "alac", "aiff", "aif", "opus",
            ]
        );
        assert!(!is_supported_audio_extension("wma"));
        assert!(is_supported_audio_extension("OPUS"));
    }

    #[test]
    fn advertised_opus_has_a_registered_decoder() {
        let mut params = AudioCodecParameters::new();
        params
            .for_codec(CODEC_ID_OPUS)
            .with_sample_rate(48_000)
            .with_channels(symphonia::core::audio::layouts::CHANNEL_LAYOUT_MONO);
        assert!(
            decoder_registry()
                .make_audio_decoder(&params, &AudioDecoderOptions::default())
                .is_ok()
        );
    }
}
