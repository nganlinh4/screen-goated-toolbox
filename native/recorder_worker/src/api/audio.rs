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
