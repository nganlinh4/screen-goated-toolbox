use std::io::Cursor;

pub(crate) fn encode_wav(samples: &[i16], sample_rate: u32, channels: u16) -> Vec<u8> {
    let spec = hound::WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut output = Cursor::new(Vec::new());
    {
        let mut writer =
            hound::WavWriter::new(&mut output, spec).expect("in-memory WAV writer must initialize");
        for sample in samples {
            writer
                .write_sample(*sample)
                .expect("in-memory WAV sample must write");
        }
        writer.finalize().expect("in-memory WAV must finalize");
    }
    output.into_inner()
}

pub(crate) fn extract_pcm_from_wav(wav_data: &[u8]) -> anyhow::Result<Vec<i16>> {
    let reader = hound::WavReader::new(Cursor::new(wav_data))?;
    let spec = reader.spec();
    let samples: Vec<i16> = match spec.sample_format {
        hound::SampleFormat::Int => reader.into_samples::<i16>().collect::<Result<_, _>>()?,
        hound::SampleFormat::Float => reader
            .into_samples::<f32>()
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(|sample| (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
            .collect(),
    };
    let mono = if spec.channels > 1 {
        samples
            .chunks(spec.channels as usize)
            .map(|chunk| {
                let sum: i32 = chunk.iter().map(|sample| i32::from(*sample)).sum();
                (sum / chunk.len() as i32) as i16
            })
            .collect()
    } else {
        samples
    };
    Ok(resample_linear_i16(
        &mono,
        16_000.0 / f64::from(spec.sample_rate),
    ))
}

pub(crate) fn resample_linear_i16(samples: &[i16], ratio: f64) -> Vec<i16> {
    if (ratio - 1.0).abs() < f64::EPSILON || samples.is_empty() {
        return samples.to_vec();
    }
    let output_len = (samples.len() as f64 * ratio) as usize;
    (0..output_len)
        .map(|index| {
            let source = index as f64 / ratio;
            let lower = source as usize;
            let upper = (lower + 1).min(samples.len() - 1);
            let fraction = source - lower as f64;
            (samples[lower] as f64 + (samples[upper] as f64 - samples[lower] as f64) * fraction)
                as i16
        })
        .collect()
}
