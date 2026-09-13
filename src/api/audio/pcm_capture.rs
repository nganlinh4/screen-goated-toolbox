//! One diagnosed sample-format adapter for mono speech capture.
use super::{capture_diagnostics::CaptureDiagnostics, pcm::PcmConverter};
use anyhow::Result;
use cpal::traits::DeviceTrait;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub(crate) struct CaptureOptions {
    pub interleaved: bool,
    pub stop: Arc<AtomicBool>,
    pub pause: Arc<AtomicBool>,
    pub diagnostics: CaptureDiagnostics,
}

pub(crate) fn build(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    options: CaptureOptions,
    deliver: impl FnMut(Vec<i16>) + Send + 'static,
    error: impl FnMut(cpal::Error) + Send + 'static,
) -> Result<cpal::Stream> {
    options.diagnostics.configure(device, config);
    options
        .diagnostics
        .controls(options.stop.clone(), options.pause.clone());
    macro_rules! format { ($($variant:ident => $ty:ty),* $(,)?) => {
        match config.sample_format() {
            $(cpal::SampleFormat::$variant => typed::<$ty>(device, config, options, deliver, error),)*
            other => anyhow::bail!("unsupported microphone sample format: {other:?}"),
        }
    }; }
    format!(I8 => i8, I16 => i16, I24 => cpal::I24, I32 => i32, I64 => i64,
        U8 => u8, U16 => u16, U24 => cpal::U24, U32 => u32, U64 => u64, F32 => f32, F64 => f64)
}

fn typed<T: cpal::SizedSample>(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    options: CaptureOptions,
    mut deliver: impl FnMut(Vec<i16>) + Send + 'static,
    mut error: impl FnMut(cpal::Error) + Send + 'static,
) -> Result<cpal::Stream>
where
    f64: cpal::FromSample<T>,
{
    let mut converter = PcmConverter::new(
        if options.interleaved {
            1
        } else {
            config.channels() as usize
        },
        config.sample_rate(),
        if options.interleaved {
            config.sample_rate()
        } else {
            16000
        },
    )?;
    let error_diagnostics = options.diagnostics.clone();
    let mut last_summary = std::time::Instant::now();
    let mut frames = 0u64;
    let mut active_frames = 0u64;
    let mut maximum_rms = 0.0f32;
    let mut activity = super::activity::SpeechActivity::default();
    Ok(device.build_input_stream((*config).into(), move |data: &[T], _| {
        let callback_started = std::time::Instant::now();
        let suppressed = options.stop.load(Ordering::Relaxed) || options.pause.load(Ordering::Relaxed);
        options.diagnostics.raw(data.iter().map(|s| s.to_sample::<f64>()), suppressed);
        if suppressed { converter.reset(); options.diagnostics.callback_finished(callback_started); return; }
        let pcm = converter.push(data.iter().map(|s| s.to_sample::<f64>()));
        options.diagnostics.delivered(pcm.iter().map(|s| *s as f64 / 32768.0));
        if !pcm.is_empty() {
            let rms = crate::api::gemini_transcribe::compute_i16_rms(&pcm);
            frames += 1;
            maximum_rms = maximum_rms.max(rms);
            active_frames += u64::from(activity.observe(rms, std::time::Instant::now()));
            if last_summary.elapsed() >= crate::debug_log::diagnostics::health_interval() {
                options.diagnostics.event("activity", &format!("callback_frames={frames} active_frames={active_frames} max_frame_rms={maximum_rms:.6}"));
                frames = 0; active_frames = 0; maximum_rms = 0.0;
                last_summary = std::time::Instant::now();
            }
        }
        if !pcm.is_empty() { deliver(pcm); }
        options.diagnostics.callback_finished(callback_started);
    }, move |err| {
        error_diagnostics.event("stream_error", &format!("{err:?}"));
        error(err);
    }, None)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cpal::Sample;
    #[test]
    fn signed_unsigned_and_float_formats_share_normalized_pcm_contract() {
        macro_rules! check {
            ($values:expr) => {{
                let mut converter = PcmConverter::new(1, 16000, 16000).unwrap();
                assert_eq!(
                    converter.push($values.into_iter().map(|v| v.to_sample::<f64>())),
                    [-32768, 0, 16384]
                );
            }};
        }
        check!([-128i8, 0, 64]);
        check!([-32768i16, 0, 16384]);
        check!([-8388608, 0, 4194304].map(|v| cpal::I24::new(v).unwrap()));
        check!([i32::MIN, 0, 1i32 << 30]);
        check!([i64::MIN, 0, 1i64 << 62]);
        check!([0u8, 128, 192]);
        check!([0u16, 32768, 49152]);
        check!([0, 8388608, 12582912].map(|v| cpal::U24::new(v).unwrap()));
        check!([0u32, 1u32 << 31, 3u32 << 30]);
        check!([0u64, 1u64 << 63, 3u64 << 62]);
        check!([-1.0f32, 0.0, 0.5]);
        check!([-1.0f64, 0.0, 0.5]);
    }
    #[test]
    #[ignore = "opens the selected microphone without retaining or transmitting audio"]
    fn native_capture_pause_resume_and_reopen() -> Result<()> {
        use cpal::traits::StreamTrait;
        use std::sync::atomic::AtomicUsize;
        for _ in 0..3 {
            let device = crate::audio_input::microphone_device()
                .ok_or_else(|| anyhow::anyhow!("no microphone"))?;
            let config = device.default_input_config()?;
            let stop = Arc::new(AtomicBool::new(false));
            let pause = Arc::new(AtomicBool::new(false));
            let count = Arc::new(AtomicUsize::new(0));
            let count_callback = count.clone();
            let errors = Arc::new(AtomicUsize::new(0));
            let errors_callback = errors.clone();
            let stream = build(
                &device,
                &config,
                CaptureOptions {
                    interleaved: false,
                    stop: stop.clone(),
                    pause: pause.clone(),
                    diagnostics: CaptureDiagnostics::new("capture-acceptance", "mic"),
                },
                move |pcm| {
                    count_callback.fetch_add(pcm.len(), Ordering::SeqCst);
                },
                move |_| {
                    errors_callback.fetch_add(1, Ordering::SeqCst);
                },
            )?;
            stream.play()?;
            std::thread::sleep(std::time::Duration::from_millis(500));
            assert!(count.load(Ordering::SeqCst) > 0);
            pause.store(true, Ordering::SeqCst);
            std::thread::sleep(std::time::Duration::from_millis(100));
            let paused_count = count.load(Ordering::SeqCst);
            std::thread::sleep(std::time::Duration::from_millis(100));
            assert_eq!(count.load(Ordering::SeqCst), paused_count);
            pause.store(false, Ordering::SeqCst);
            std::thread::sleep(std::time::Duration::from_millis(300));
            assert!(count.load(Ordering::SeqCst) > paused_count);
            stop.store(true, Ordering::SeqCst);
            drop(stream);
            assert_eq!(errors.load(Ordering::SeqCst), 0);
        }
        Ok(())
    }
}
