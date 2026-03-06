use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::HeapRb;
use ringbuf::traits::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};
use windows::Win32::System::Threading::{
    GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_HIGHEST,
};
use windows_capture::encoder::AudioEncoderHandle;

pub fn get_default_audio_config() -> (u32, u32) {
    let host = cpal::host_from_id(cpal::HostId::Wasapi).unwrap_or_else(|_| cpal::default_host());
    if let Some(device) = host.default_output_device()
        && let Ok(config) = device.default_output_config()
    {
        return (config.sample_rate(), config.channels() as u32);
    }
    (48_000, 2)
}

pub fn record_audio(
    audio_handle: AudioEncoderHandle,
    start_time: Instant,
    stop_signal: Arc<AtomicBool>,
    finished_signal: Arc<AtomicBool>,
) {
    thread::spawn(move || {
        unsafe {
            let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST);
        }
        let host = match cpal::host_from_id(cpal::HostId::Wasapi) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("Failed to get WASAPI host: {}", e);
                cpal::default_host()
            }
        };

        let device = match host.default_output_device() {
            Some(d) => d,
            None => {
                eprintln!("No default output device found for loopback");
                finished_signal.store(true, Ordering::SeqCst);
                return;
            }
        };

        let config = match device.default_output_config() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Failed to get default output config: {}", e);
                finished_signal.store(true, Ordering::SeqCst);
                return;
            }
        };

        let buffer_len = 4 * 1024 * 1024;
        let rb = HeapRb::<f32>::new(buffer_len);
        let (mut producer, mut consumer) = rb.split();

        let stream_config: cpal::StreamConfig = config.clone().into();
        let channels = stream_config.channels as usize;
        let sample_rate = stream_config.sample_rate;
        if channels == 0 {
            eprintln!("Invalid loopback channel count: 0");
            finished_signal.store(true, Ordering::SeqCst);
            return;
        }

        let err_fn = |err| eprintln!("Audio stream error: {}", err);
        let stream = match device.build_input_stream(
            &stream_config,
            move |data: &[f32], _: &_| {
                let _ = producer.push_slice(data);
            },
            err_fn,
            None,
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to build audio input stream: {}", e);
                finished_signal.store(true, Ordering::SeqCst);
                return;
            }
        };

        if let Err(e) = stream.play() {
            eprintln!("Failed to start audio stream: {}", e);
            finished_signal.store(true, Ordering::SeqCst);
            return;
        }

        println!(
            "Audio recording started (Memory Muxing): {}Hz, {} channels",
            sample_rate, channels
        );

        let mut audio_output_100ns = (start_time.elapsed().as_nanos() / 100) as i64;
        let mut chunk = vec![0.0f32; 16_384];

        while !stop_signal.load(Ordering::SeqCst) {
            if consumer.is_empty() {
                thread::sleep(Duration::from_millis(5));
                continue;
            }

            let count = consumer.pop_slice(&mut chunk);
            if count > 0
                && let Some((bytes, duration_100ns)) =
                    encode_pcm_chunk_i16(&chunk[..count], channels, sample_rate)
            {
                if let Err(e) = audio_handle.send_audio_buffer(bytes, audio_output_100ns) {
                    eprintln!("Audio mux send error: {}", e);
                    break;
                }
                audio_output_100ns = audio_output_100ns.saturating_add(duration_100ns);
            }
        }

        println!("Audio stop signal received. Flushing buffer into muxer...");
        drop(stream);

        loop {
            let count = consumer.pop_slice(&mut chunk);
            if count == 0 {
                break;
            }
            if let Some((bytes, duration_100ns)) =
                encode_pcm_chunk_i16(&chunk[..count], channels, sample_rate)
            {
                if let Err(e) = audio_handle.send_audio_buffer(bytes, audio_output_100ns) {
                    eprintln!("Audio mux flush send error: {}", e);
                    break;
                }
                audio_output_100ns = audio_output_100ns.saturating_add(duration_100ns);
            }
        }

        finished_signal.store(true, Ordering::SeqCst);
    });
}

fn encode_pcm_chunk_i16(
    samples: &[f32],
    channels: usize,
    sample_rate: u32,
) -> Option<(Vec<u8>, i64)> {
    if channels == 0 || sample_rate == 0 || samples.is_empty() {
        return None;
    }

    let frame_count = samples.len() / channels;
    if frame_count == 0 {
        return None;
    }

    let sample_count = frame_count * channels;
    let mut bytes = Vec::with_capacity(sample_count * 2);
    for &sample in &samples[..sample_count] {
        let pcm = (sample.clamp(-1.0, 1.0) * 32767.0) as i16;
        bytes.extend_from_slice(&pcm.to_le_bytes());
    }

    let duration_100ns = ((frame_count as u128) * 10_000_000u128 / (sample_rate as u128)) as i64;
    Some((bytes, duration_100ns))
}
