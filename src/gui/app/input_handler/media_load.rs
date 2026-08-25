//! Audio decoding for drag-and-drop and paste input. The thin host uses the
//! Windows media stack; optional fallback codecs stay in removable workers.

use std::io::Cursor;
use std::path::Path;

pub(crate) const SUPPORTED_AUDIO_EXTENSIONS: &[&str] = &[
    "wav", "mp3", "flac", "ogg", "m4a", "m4b", "aac", "alac", "aiff", "aif", "opus",
];

pub(crate) fn is_supported_audio_extension(extension: &str) -> bool {
    SUPPORTED_AUDIO_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
}

/// Load an audio file and convert it to interleaved 16-bit PCM WAV.
pub(crate) fn load_audio_file(path: &Path) -> Option<Vec<u8>> {
    decode_with_media_foundation(path)
        .or_else(|| crate::overlay::screen_record::decode_audio_with_optional_worker(path))
}

fn decode_with_media_foundation(path: &Path) -> Option<Vec<u8>> {
    let decoder = crate::overlay::screen_record::mf_audio::MfAudioDecoder::new_with_output_format(
        path.to_str()?,
        None,
        None,
    )
    .ok()?;
    let sample_rate = decoder.sample_rate();
    let channels = decoder.channels().max(1) as u16;
    let mut samples = Vec::new();
    while let Some((bytes, _)) = decoder.read_samples().ok()? {
        for sample in bytes.as_chunks::<4>().0 {
            let value = f32::from_le_bytes(*sample).clamp(-1.0, 1.0);
            samples.push((value * i16::MAX as f32) as i16);
        }
    }
    if samples.is_empty() {
        return None;
    }

    let spec = hound::WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut wav = Cursor::new(Vec::new());
    {
        let mut writer = hound::WavWriter::new(&mut wav, spec).ok()?;
        for sample in samples {
            writer.write_sample(sample).ok()?;
        }
        writer.finalize().ok()?;
    }
    Some(wav.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn advertised_extensions_are_unique() {
        let mut extensions = SUPPORTED_AUDIO_EXTENSIONS.to_vec();
        extensions.sort_unstable();
        extensions.dedup();
        assert_eq!(extensions.len(), SUPPORTED_AUDIO_EXTENSIONS.len());
    }

    #[test]
    fn optional_worker_is_only_the_native_decoder_fallback() {
        let source = include_str!("media_load.rs");
        let native = source.find("decode_with_media_foundation(path)").unwrap();
        let optional = source
            .find("decode_audio_with_optional_worker(path)")
            .unwrap();
        assert!(native < optional);
    }
}
