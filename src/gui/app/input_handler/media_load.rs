//! Audio decoding for the drag-and-drop / paste input path: decode any
//! symphonia-supported container into 16-bit PCM WAV bytes. Kept out of the
//! drag-and-drop routing module (`input_handler`) so that file is pure dispatch.

use std::io::Cursor;
use std::path::Path;

/// Load an audio file and convert to WAV format using symphonia.
/// Supports: WAV, MP3, FLAC, OGG, AAC, ALAC, AIFF, etc.
pub(crate) fn load_audio_file(path: &Path) -> Option<Vec<u8>> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    // Open the file
    let file = std::fs::File::open(path).ok()?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    // Create a hint using the file extension
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    // Probe the media source
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .ok()?;

    let mut format = probed.format;

    // Find the first audio track
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != symphonia::core::codecs::CODEC_TYPE_NULL)?;
    let track_id = track.id;
    let codec_params = track.codec_params.clone();

    // Get sample rate and channels
    let sample_rate = codec_params.sample_rate.unwrap_or(44100);
    let channels = codec_params.channels.map(|c| c.count()).unwrap_or(2) as u16;

    // Create decoder
    let mut decoder = symphonia::default::get_codecs()
        .make(&codec_params, &DecoderOptions::default())
        .ok()?;

    // Decode all samples
    let mut all_samples: Vec<i16> = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(ref e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(_) => break,
        };

        if packet.track_id() != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(_) => continue,
        };

        // Convert to interleaved i16 samples
        let spec = *decoded.spec();
        let duration = decoded.capacity() as u64;
        let mut sample_buf = SampleBuffer::<i16>::new(duration, spec);
        sample_buf.copy_interleaved_ref(decoded);
        all_samples.extend(sample_buf.samples());
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
