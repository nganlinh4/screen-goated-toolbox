use anyhow::{Context, Result};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// Load an audio file, converting to mono f32 at the target sample rate.
///
/// Uses symphonia to decode common audio formats (MP3, FLAC, AAC, WAV, OGG).
/// Falls back to hound for WAV files if symphonia fails.
pub fn load_audio(path: &str, target_sample_rate: u32) -> Result<Vec<f32>> {
    match load_audio_symphonia(path, target_sample_rate) {
        Ok(samples) => Ok(samples),
        Err(e) => {
            tracing::warn!("Symphonia loading failed ({}), trying WAV fallback", e);
            load_audio_wav(path, target_sample_rate)
        }
    }
}

/// Load audio using symphonia (MP3, FLAC, AAC, WAV, OGG).
fn load_audio_symphonia(path: &str, target_sample_rate: u32) -> Result<Vec<f32>> {
    let file = std::fs::File::open(path).context("Failed to open audio file")?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = std::path::Path::new(path).extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .context("Failed to probe audio format")?;

    let mut format = probed.format;

    let track = format
        .default_track()
        .ok_or_else(|| anyhow::anyhow!("No audio track found in {}", path))?;

    let track_id = track.id;
    let source_sample_rate = track.codec_params.sample_rate
        .ok_or_else(|| anyhow::anyhow!("Unknown sample rate"))?;
    let channels = track.codec_params.channels
        .map(|c| c.count())
        .unwrap_or(1);

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .context("Failed to create audio decoder")?;

    let mut all_samples: Vec<f32> = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(ref e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(e) => return Err(e.into()),
        };

        if packet.track_id() != track_id {
            continue;
        }

        let decoded = decoder.decode(&packet)?;
        let spec = *decoded.spec();
        let num_frames = decoded.frames();

        if num_frames == 0 {
            continue;
        }

        let mut sample_buf = SampleBuffer::<f32>::new(num_frames as u64, spec);
        sample_buf.copy_interleaved_ref(decoded);
        let interleaved = sample_buf.samples();

        // Downmix to mono
        if channels > 1 {
            for frame in interleaved.chunks(channels) {
                let mono = frame.iter().sum::<f32>() / channels as f32;
                all_samples.push(mono);
            }
        } else {
            all_samples.extend_from_slice(interleaved);
        }
    }

    if all_samples.is_empty() {
        anyhow::bail!("No audio samples decoded");
    }

    // Resample if needed
    let samples = if source_sample_rate != target_sample_rate {
        resample(&all_samples, source_sample_rate, target_sample_rate)?
    } else {
        all_samples
    };

    tracing::info!(
        "Loaded audio via Symphonia: {} samples ({:.2}s at {}Hz)",
        samples.len(),
        samples.len() as f64 / target_sample_rate as f64,
        target_sample_rate
    );

    Ok(samples)
}

/// Load a WAV file and resample to target rate using rubato.
fn load_audio_wav(path: &str, target_sample_rate: u32) -> Result<Vec<f32>> {
    let reader = hound::WavReader::open(path).context("Failed to open WAV file")?;
    let spec = reader.spec();

    tracing::info!(
        "WAV: {}ch, {}Hz, {:?}, {}bit",
        spec.channels,
        spec.sample_rate,
        spec.sample_format,
        spec.bits_per_sample
    );

    // Read all samples as f32
    let raw_samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => {
            let max_val = (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .into_samples::<i32>()
                .map(|s| s.unwrap() as f32 / max_val)
                .collect()
        }
        hound::SampleFormat::Float => {
            reader
                .into_samples::<f32>()
                .map(|s| s.unwrap())
                .collect()
        }
    };

    // Convert to mono if stereo
    let mono_samples = if spec.channels > 1 {
        let channels = spec.channels as usize;
        raw_samples
            .chunks(channels)
            .map(|frame| frame.iter().sum::<f32>() / channels as f32)
            .collect()
    } else {
        raw_samples
    };

    // Resample if needed
    let samples = if spec.sample_rate != target_sample_rate {
        resample(&mono_samples, spec.sample_rate, target_sample_rate)?
    } else {
        mono_samples
    };

    tracing::info!(
        "Loaded WAV: {} samples ({:.2}s at {}Hz)",
        samples.len(),
        samples.len() as f64 / target_sample_rate as f64,
        target_sample_rate
    );

    Ok(samples)
}

/// Resample audio using rubato.
fn resample(samples: &[f32], from_rate: u32, to_rate: u32) -> Result<Vec<f32>> {
    use rubato::{SincFixedIn, SincInterpolationParameters, SincInterpolationType, Resampler, WindowFunction};

    let params = SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 256,
        window: WindowFunction::BlackmanHarris2,
    };

    let mut resampler = SincFixedIn::<f32>::new(
        to_rate as f64 / from_rate as f64,
        2.0,
        params,
        samples.len(),
        1, // mono
    )
    .map_err(|e| anyhow::anyhow!("Failed to create resampler: {}", e))?;

    let result = resampler
        .process(&[samples.to_vec()], None)
        .map_err(|e| anyhow::anyhow!("Resampling failed: {}", e))?;

    Ok(result.into_iter().next().unwrap_or_default())
}
