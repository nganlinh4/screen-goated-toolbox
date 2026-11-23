use super::loopback_format::Converter;
use anyhow::{Context, Result, bail};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use parking_lot::Mutex;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::thread::{self, JoinHandle};
use std::time::Duration;

type Sink = Arc<Mutex<Box<dyn FnMut(&[f32], &cpal::InputCallbackInfo, u64) + Send>>>;
type ErrorSink = Arc<dyn Fn(cpal::Error) + Send + Sync>;

pub(super) struct DefaultLoopback {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

fn needs_reopen(kind: cpal::ErrorKind) -> bool {
    matches!(
        kind,
        cpal::ErrorKind::StreamInvalidated
            | cpal::ErrorKind::DeviceChanged
            | cpal::ErrorKind::DeviceNotAvailable
    )
}

impl DefaultLoopback {
    pub(super) fn start(
        rate: u32,
        channels: usize,
        samples: impl FnMut(&[f32], &cpal::InputCallbackInfo, u64) + Send + 'static,
        error: impl Fn(cpal::Error) + Send + Sync + 'static,
    ) -> Result<Self> {
        if rate == 0 || channels == 0 {
            bail!("invalid loopback output format");
        }
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let sink: Sink = Arc::new(Mutex::new(Box::new(samples)));
        let errors: ErrorSink = Arc::new(error);
        let (ready, startup) = mpsc::sync_channel(1);
        let thread = thread::spawn(move || {
            let host = cpal::default_host();
            let reopen = Arc::new(AtomicBool::new(false));
            let mut generation = 0;
            let mut stream = None;
            while !stopping.load(Ordering::Acquire) {
                if stream.is_none() || reopen.swap(false, Ordering::AcqRel) {
                    drop(stream.take());
                    generation += 1;
                    match open(
                        &host,
                        rate,
                        channels,
                        generation,
                        sink.clone(),
                        errors.clone(),
                        reopen.clone(),
                    ) {
                        Ok(next) => {
                            if stopping.load(Ordering::Acquire) {
                                break;
                            }
                            if let Err(error) = next.play() {
                                if generation == 1 {
                                    let _ = ready.send(Err(error.to_string()));
                                    break;
                                }
                                eprintln!("[Loopback] default endpoint not ready: {error}");
                            } else {
                                stream = Some(next);
                                if generation == 1 {
                                    let _ = ready.send(Ok(()));
                                }
                                crate::log_info!(
                                    "[Loopback] following default endpoint generation={generation}"
                                );
                            }
                        }
                        Err(error) => {
                            if generation == 1 {
                                let _ = ready.send(Err(error.to_string()));
                                break;
                            }
                            // A disconnected endpoint can leave no default until the replacement arrives.
                        }
                    }
                }
                thread::sleep(Duration::from_millis(100));
            }
            drop(stream);
        });
        let capture = Self {
            stop,
            thread: Some(thread),
        };
        match startup.recv_timeout(Duration::from_secs(10)) {
            Ok(Ok(())) => Ok(capture),
            result => {
                capture.stop.store(true, Ordering::Release);
                bail!("start default loopback capture: {result:?}")
            }
        }
    }
}

impl Drop for DefaultLoopback {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn open(
    host: &cpal::Host,
    rate: u32,
    channels: usize,
    generation: u64,
    sink: Sink,
    errors: ErrorSink,
    reopen: Arc<AtomicBool>,
) -> Result<cpal::Stream> {
    let device = host
        .default_output_device()
        .context("no default output endpoint")?;
    let format = device.default_output_config()?;
    if format.sample_rate() == 0
        || format.channels() == 0
        || rate as u64 > format.sample_rate() as u64 * 8
        || channels > 32
    {
        bail!("invalid default endpoint conversion format");
    }
    let mut converter = Converter::new(
        format.sample_rate(),
        format.channels() as usize,
        rate,
        channels,
    );
    let input_rate = u64::from(format.sample_rate());
    let input_channels = usize::from(format.channels());
    let packet_errors = errors.clone();
    let callback = move |data: &[f32], info: &cpal::InputCallbackInfo| {
        let output_bound =
            ((data.len() / input_channels) as u64 + 2).saturating_mul(u64::from(rate)) / input_rate;
        if data.len() > 2 * 1024 * 1024
            || output_bound
                .saturating_add(2)
                .saturating_mul(channels as u64)
                > 128 * 1024
        {
            packet_errors(cpal::Error::new(cpal::ErrorKind::InvalidInput));
            return;
        }
        let converted = converter.convert(data);
        if !converted.is_empty() {
            (sink.lock())(&converted, info, generation);
        }
    };
    let error = move |error: cpal::Error| {
        if needs_reopen(error.kind()) {
            reopen.store(true, Ordering::Release);
        } else {
            errors(error);
        }
    };
    let config = format.config();
    let stream = match format.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(config, callback, error, None)?,
        cpal::SampleFormat::I16 => {
            let mut callback = callback;
            device.build_input_stream(
                config,
                move |data: &[i16], info| {
                    callback(
                        &data.iter().map(|x| *x as f32 / 32768.0).collect::<Vec<_>>(),
                        info,
                    )
                },
                error,
                None,
            )?
        }
        cpal::SampleFormat::U16 => {
            let mut callback = callback;
            device.build_input_stream(
                config,
                move |data: &[u16], info| {
                    callback(
                        &data
                            .iter()
                            .map(|x| (*x as f32 - 32768.0) / 32768.0)
                            .collect::<Vec<_>>(),
                        info,
                    )
                },
                error,
                None,
            )?
        }
        cpal::SampleFormat::I32 => {
            let mut callback = callback;
            device.build_input_stream(
                config,
                move |data: &[i32], info| {
                    callback(
                        &data
                            .iter()
                            .map(|x| *x as f32 / 2147483648.0)
                            .collect::<Vec<_>>(),
                        info,
                    )
                },
                error,
                None,
            )?
        }
        other => bail!("unsupported default endpoint format: {other:?}"),
    };
    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn endpoint_invalidation_reopens_but_an_xrun_does_not() {
        assert!(needs_reopen(cpal::ErrorKind::StreamInvalidated));
        assert!(needs_reopen(cpal::ErrorKind::DeviceNotAvailable));
        assert!(!needs_reopen(cpal::ErrorKind::Xrun));
    }
}
