use hound::WavReader;
use std::io::Cursor;

/// Decode a prepared 16 kHz mono WAV (as produced by the narration prep paths)
/// into i16 samples. `label` names the producer for error messages, e.g.
/// "Gemini Translate" or "S2S".
pub(crate) fn decode_wav_mono_i16(bytes: &[u8], label: &str) -> Result<Vec<i16>, String> {
    let mut reader = WavReader::new(Cursor::new(bytes))
        .map_err(|error| format!("Read prepared {label} WAV: {error}"))?;
    let spec = reader.spec();
    if spec.channels != 1 || spec.sample_rate != 16_000 {
        return Err(format!(
            "Prepared {label} WAV must be 16 kHz mono, got {} Hz {} channel(s)",
            spec.sample_rate, spec.channels
        ));
    }
    reader
        .samples::<i16>()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Decode prepared {label} WAV samples: {error}"))
}
