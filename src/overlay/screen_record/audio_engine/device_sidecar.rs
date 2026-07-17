use anyhow::{Context, Result, bail};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::traits::*;
use ringbuf::{HeapProd, HeapRb};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};
use windows::Win32::System::Threading::{
    GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_HIGHEST,
};

use super::AUDIO_POLL_SLEEP_MS;

const DEVICE_AUDIO_RING_SAMPLES: usize = 4 * 1024 * 1024;
const MAX_INITIAL_PADDING_SECONDS: u64 = 2;

fn push_initial_padding(
    producer: &mut HeapProd<i16>,
    recording_start: Instant,
    sample_rate: u32,
    channels: usize,
    padded: &mut bool,
) {
    if *padded {
        return;
    }
    *padded = true;

    let elapsed = recording_start
        .elapsed()
        .min(Duration::from_secs(MAX_INITIAL_PADDING_SECONDS));
    let frames = ((elapsed.as_nanos() * sample_rate as u128) / 1_000_000_000) as usize;
    let sample_count = frames.saturating_mul(channels);
    let silence = vec![0i16; sample_count];
    let _ = producer.push_slice(&silence);
}

fn push_f32_samples(producer: &mut HeapProd<i16>, data: &[f32]) {
    let converted = data
        .iter()
        .map(|sample| (sample.clamp(-1.0, 1.0) * 32767.0) as i16)
        .collect::<Vec<_>>();
    let _ = producer.push_slice(&converted);
}

fn push_u16_samples(producer: &mut HeapProd<i16>, data: &[u16]) {
    let converted = data
        .iter()
        .map(|sample| (*sample as i32 - 32768) as i16)
        .collect::<Vec<_>>();
    let _ = producer.push_slice(&converted);
}

fn build_loopback_stream(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    sample_format: cpal::SampleFormat,
    recording_start: Instant,
    sample_rate: u32,
    channels: usize,
    producer: HeapProd<i16>,
) -> Result<cpal::Stream> {
    let err_fn = |error| eprintln!("[CompatibilityAudio] stream error: {error}");

    match sample_format {
        cpal::SampleFormat::F32 => {
            let mut producer = producer;
            let mut padded = false;
            device
                .build_input_stream(
                    *config,
                    move |data: &[f32], _: &_| {
                        if data.is_empty() {
                            return;
                        }
                        push_initial_padding(
                            &mut producer,
                            recording_start,
                            sample_rate,
                            channels,
                            &mut padded,
                        );
                        push_f32_samples(&mut producer, data);
                    },
                    err_fn,
                    None,
                )
                .context("build system-output loopback stream")
        }
        cpal::SampleFormat::I16 => {
            let mut producer = producer;
            let mut padded = false;
            device
                .build_input_stream(
                    *config,
                    move |data: &[i16], _: &_| {
                        if data.is_empty() {
                            return;
                        }
                        push_initial_padding(
                            &mut producer,
                            recording_start,
                            sample_rate,
                            channels,
                            &mut padded,
                        );
                        let _ = producer.push_slice(data);
                    },
                    err_fn,
                    None,
                )
                .context("build system-output loopback stream")
        }
        cpal::SampleFormat::U16 => {
            let mut producer = producer;
            let mut padded = false;
            device
                .build_input_stream(
                    *config,
                    move |data: &[u16], _: &_| {
                        if data.is_empty() {
                            return;
                        }
                        push_initial_padding(
                            &mut producer,
                            recording_start,
                            sample_rate,
                            channels,
                            &mut padded,
                        );
                        push_u16_samples(&mut producer, data);
                    },
                    err_fn,
                    None,
                )
                .context("build system-output loopback stream")
        }
        other => bail!("unsupported system-output sample format: {other:?}"),
    }
}

pub(crate) fn record_device_audio_sidecar(
    output_path: String,
    recording_start: Instant,
    stop_signal: Arc<AtomicBool>,
    finished_signal: Arc<AtomicBool>,
) -> Result<()> {
    finished_signal.store(false, Ordering::SeqCst);

    let host = cpal::host_from_id(cpal::HostId::Wasapi).unwrap_or_else(|_| cpal::default_host());
    let device = host
        .default_output_device()
        .context("no default system output device found")?;
    let config = device
        .default_output_config()
        .context("query default system output format")?;
    let sample_rate = config.sample_rate();
    let channels = config.channels() as usize;
    if sample_rate == 0 || channels == 0 {
        bail!("default system output reported an invalid audio format");
    }

    let rb = HeapRb::<i16>::new(DEVICE_AUDIO_RING_SAMPLES);
    let (producer, mut consumer) = rb.split();
    let stream_config: cpal::StreamConfig = config.into();
    let stream = build_loopback_stream(
        &device,
        &stream_config,
        config.sample_format(),
        recording_start,
        sample_rate,
        channels,
        producer,
    )?;
    stream.play().context("start system-output loopback")?;

    thread::spawn(move || {
        unsafe {
            let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST);
        }

        let spec = hound::WavSpec {
            channels: channels as u16,
            sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = match hound::WavWriter::create(&output_path, spec) {
            Ok(writer) => writer,
            Err(error) => {
                eprintln!("[CompatibilityAudio] create WAV failed: {error}");
                finished_signal.store(true, Ordering::SeqCst);
                return;
            }
        };

        let _stream = stream;
        let mut chunk = vec![0i16; 16_384];
        let mut write_failed = false;

        while !stop_signal.load(Ordering::SeqCst) {
            let count = consumer.pop_slice(&mut chunk);
            if count == 0 {
                thread::sleep(Duration::from_millis(AUDIO_POLL_SLEEP_MS));
                continue;
            }
            for sample in &chunk[..count] {
                if let Err(error) = writer.write_sample(*sample) {
                    eprintln!("[CompatibilityAudio] WAV write failed: {error}");
                    write_failed = true;
                    break;
                }
            }
            if write_failed {
                break;
            }
        }

        if !write_failed {
            loop {
                let count = consumer.pop_slice(&mut chunk);
                if count == 0 {
                    break;
                }
                for sample in &chunk[..count] {
                    if let Err(error) = writer.write_sample(*sample) {
                        eprintln!("[CompatibilityAudio] WAV flush failed: {error}");
                        write_failed = true;
                        break;
                    }
                }
                if write_failed {
                    break;
                }
            }
        }

        if let Err(error) = writer.finalize() {
            eprintln!("[CompatibilityAudio] WAV finalize failed: {error}");
            write_failed = true;
        }
        if write_failed {
            let _ = std::fs::remove_file(&output_path);
        }
        finished_signal.store(true, Ordering::SeqCst);
    });

    Ok(())
}
