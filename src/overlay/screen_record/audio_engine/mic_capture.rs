use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::traits::*;
use ringbuf::{HeapProd, HeapRb};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};
use windows::Win32::System::Threading::{
    GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_HIGHEST,
};

use super::AUDIO_POLL_SLEEP_MS;

fn push_mic_f32_samples(producer: &mut HeapProd<i16>, data: &[f32]) {
    if data.is_empty() {
        return;
    }
    let mut converted = Vec::with_capacity(data.len());
    converted.extend(
        data.iter()
            .map(|sample| (sample.clamp(-1.0, 1.0) * 32767.0) as i16),
    );
    let _ = producer.push_slice(&converted);
}

fn push_mic_u16_samples(producer: &mut HeapProd<i16>, data: &[u16]) {
    if data.is_empty() {
        return;
    }
    let mut converted = Vec::with_capacity(data.len());
    converted.extend(data.iter().map(|sample| (*sample as i32 - 32768) as i16));
    let _ = producer.push_slice(&converted);
}

fn build_mic_input_stream(
    device: &cpal::Device,
    stream_config: &cpal::StreamConfig,
    sample_format: cpal::SampleFormat,
    recording_start: Instant,
    start_offset_ms: &'static AtomicU64,
    producer: HeapProd<i16>,
) -> Result<cpal::Stream, String> {
    let err_fn = |err| eprintln!("[MicCapture] stream error: {}", err);
    match sample_format {
        cpal::SampleFormat::F32 => {
            let mut producer = producer;
            device
                .build_input_stream(
                    stream_config,
                    move |data: &[f32], _: &_| {
                        if !data.is_empty() {
                            let elapsed_ms = recording_start.elapsed().as_millis() as u64;
                            let _ = start_offset_ms.compare_exchange(
                                u64::MAX,
                                elapsed_ms,
                                Ordering::SeqCst,
                                Ordering::SeqCst,
                            );
                        }
                        push_mic_f32_samples(&mut producer, data);
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| format!("Failed to build mic input stream: {e}"))
        }
        cpal::SampleFormat::I16 => {
            let mut producer = producer;
            device
                .build_input_stream(
                    stream_config,
                    move |data: &[i16], _: &_| {
                        if !data.is_empty() {
                            let elapsed_ms = recording_start.elapsed().as_millis() as u64;
                            let _ = start_offset_ms.compare_exchange(
                                u64::MAX,
                                elapsed_ms,
                                Ordering::SeqCst,
                                Ordering::SeqCst,
                            );
                        }
                        let _ = producer.push_slice(data);
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| format!("Failed to build mic input stream: {e}"))
        }
        cpal::SampleFormat::U16 => {
            let mut producer = producer;
            device
                .build_input_stream(
                    stream_config,
                    move |data: &[u16], _: &_| {
                        if !data.is_empty() {
                            let elapsed_ms = recording_start.elapsed().as_millis() as u64;
                            let _ = start_offset_ms.compare_exchange(
                                u64::MAX,
                                elapsed_ms,
                                Ordering::SeqCst,
                                Ordering::SeqCst,
                            );
                        }
                        push_mic_u16_samples(&mut producer, data);
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| format!("Failed to build mic input stream: {e}"))
        }
        other => Err(format!("Unsupported mic sample format: {other:?}")),
    }
}

pub(crate) fn record_mic_audio_sidecar(
    output_path: String,
    recording_start: Instant,
    stop_signal: Arc<AtomicBool>,
    finished_signal: Arc<AtomicBool>,
    start_offset_ms: &'static AtomicU64,
) -> Result<(), String> {
    finished_signal.store(false, Ordering::SeqCst);
    start_offset_ms.store(u64::MAX, Ordering::SeqCst);

    let host = cpal::host_from_id(cpal::HostId::Wasapi).unwrap_or_else(|_| cpal::default_host());
    let device = host
        .default_input_device()
        .ok_or("No default microphone found")?;
    let config = device
        .default_input_config()
        .map_err(|e| format!("Failed to query default microphone config: {e}"))?;
    let sample_rate = config.sample_rate();
    let channels = config.channels() as u16;
    if channels == 0 || sample_rate == 0 {
        return Err("Default microphone reported an invalid audio format".to_string());
    }

    let buffer_len = 2 * 1024 * 1024;
    let rb = HeapRb::<i16>::new(buffer_len);
    let (producer, mut consumer) = rb.split();
    let stream_config: cpal::StreamConfig = config.clone().into();
    let stream = build_mic_input_stream(
        &device,
        &stream_config,
        config.sample_format(),
        recording_start,
        start_offset_ms,
        producer,
    )?;
    stream
        .play()
        .map_err(|e| format!("Failed to start microphone capture: {e}"))?;

    thread::spawn(move || {
        unsafe {
            let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST);
        }

        let spec = hound::WavSpec {
            channels,
            sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = match hound::WavWriter::create(&output_path, spec) {
            Ok(writer) => writer,
            Err(error) => {
                eprintln!("[MicCapture] failed to create WAV writer: {}", error);
                finished_signal.store(true, Ordering::SeqCst);
                return;
            }
        };

        let _stream = stream;
        let mut chunk = vec![0i16; 8192];
        let mut write_failed = false;

        while !stop_signal.load(Ordering::SeqCst) {
            let count = consumer.pop_slice(&mut chunk);
            if count == 0 {
                thread::sleep(Duration::from_millis(AUDIO_POLL_SLEEP_MS));
                continue;
            }
            for &sample in &chunk[..count] {
                if let Err(error) = writer.write_sample(sample) {
                    eprintln!("[MicCapture] WAV write failed: {}", error);
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
                for &sample in &chunk[..count] {
                    if let Err(error) = writer.write_sample(sample) {
                        eprintln!("[MicCapture] WAV flush failed: {}", error);
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
            eprintln!("[MicCapture] finalize failed: {}", error);
            let _ = std::fs::remove_file(&output_path);
        }
        finished_signal.store(true, Ordering::SeqCst);
    });

    Ok(())
}
