use std::io::BufWriter;
use std::path::Path;

use anyhow::{Context, Result, bail};
use sgt_recorder_protocol::MAX_DECODED_AUDIO_BYTES;

pub(super) fn decode(input_path: &str, output_path: &str) -> Result<()> {
    let output = Path::new(output_path);
    validate_output(output)?;
    let result = (|| {
        let decoder = super::mf_audio::MfAudioDecoder::new(input_path)
            .map_err(anyhow::Error::msg)
            .context("open optional audio decoder")?;
        let spec = hound::WavSpec {
            channels: decoder.channels().max(1) as u16,
            sample_rate: decoder.sample_rate(),
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output)
            .context("create decoded audio output")?;
        let mut writer = hound::WavWriter::new(BufWriter::new(file), spec)?;
        let mut written_bytes = 0_u64;
        while let Some((bytes, _)) = decoder.read_samples().map_err(anyhow::Error::msg)? {
            for sample in bytes.as_chunks::<4>().0 {
                written_bytes = written_bytes.saturating_add(2);
                if written_bytes > MAX_DECODED_AUDIO_BYTES {
                    bail!("decoded audio exceeds its bounded output size");
                }
                let value = f32::from_le_bytes(*sample).clamp(-1.0, 1.0);
                writer.write_sample((value * i16::MAX as f32) as i16)?;
            }
        }
        if written_bytes == 0 {
            bail!("optional audio decoder returned no samples");
        }
        writer.finalize()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(output);
    }
    result
}

fn validate_output(output: &Path) -> Result<()> {
    if !output.is_absolute() || output.exists() {
        bail!("decoded audio output path is invalid");
    }
    let name = output
        .file_name()
        .and_then(|name| name.to_str())
        .context("decoded audio output name is invalid")?;
    let token = name
        .strip_prefix("audio-decode-")
        .and_then(|value| value.strip_suffix(".wav"))
        .context("decoded audio output name is not owned")?;
    if token.len() != 32 || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("decoded audio output token is invalid");
    }
    let parent = output
        .parent()
        .context("decoded audio output has no parent")?;
    if std::fs::canonicalize(parent)? != std::fs::canonicalize(std::env::current_dir()?)? {
        bail!("decoded audio output escaped the worker workspace");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_name_requires_the_owned_random_shape() {
        let current = std::env::current_dir().unwrap();
        assert!(
            validate_output(&current.join("audio-decode-00112233445566778899aabbccddeeff.wav"))
                .is_ok()
        );
        assert!(validate_output(&current.join("audio-decode-short.wav")).is_err());
        assert!(validate_output(&current.join("other-00112233445566778899aabbccddeeff.wav")).is_err());
    }
}
