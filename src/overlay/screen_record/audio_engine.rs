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

const AUDIO_POLL_SLEEP_MS: u64 = 5;
const AUDIO_SILENCE_CATCHUP_THRESHOLD_100NS: i64 = 200_000; // 20 ms
const AUDIO_SILENCE_CHUNK_DIVISOR: u32 = 50; // 20 ms chunks

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
        let (fallback_sample_rate, fallback_channels) = get_default_audio_config();
        let host = match cpal::host_from_id(cpal::HostId::Wasapi) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("Failed to get WASAPI host: {}", e);
                cpal::default_host()
            }
        };

        let buffer_len = 4 * 1024 * 1024;
        let rb = HeapRb::<f32>::new(buffer_len);
        let (mut producer, mut consumer) = rb.split();

        let mut loopback_stream = None;
        let mut loopback_available = false;
        let mut sample_rate = fallback_sample_rate;
        let mut channels = fallback_channels as usize;

        if let Some(device) = host.default_output_device() {
            match device.default_output_config() {
                Ok(config) => {
                    let stream_config: cpal::StreamConfig = config.clone().into();
                    sample_rate = stream_config.sample_rate;
                    channels = stream_config.channels as usize;

                    let err_fn = |err| eprintln!("Audio stream error: {}", err);
                    match device.build_input_stream(
                        &stream_config,
                        move |data: &[f32], _: &_| {
                            let _ = producer.push_slice(data);
                        },
                        err_fn,
                        None,
                    ) {
                        Ok(stream) => {
                            if let Err(e) = stream.play() {
                                eprintln!(
                                    "Failed to start audio stream; falling back to silence: {}",
                                    e
                                );
                            } else {
                                loopback_available = true;
                                loopback_stream = Some(stream);
                            }
                        }
                        Err(e) => {
                            eprintln!(
                                "Failed to build audio input stream; falling back to silence: {}",
                                e
                            );
                        }
                    }
                }
                Err(e) => {
                    eprintln!(
                        "Failed to get default output config; falling back to silence: {}",
                        e
                    );
                }
            }
        } else {
            eprintln!("No default output device found for loopback; falling back to silence");
        }

        if channels == 0 {
            eprintln!("Invalid loopback channel count: 0");
            finished_signal.store(true, Ordering::SeqCst);
            return;
        }

        println!(
            "Audio recording started (Memory Muxing): {}Hz, {} channels{}",
            sample_rate,
            channels,
            if loopback_available {
                ""
            } else {
                " [silence fallback]"
            }
        );

        let mut audio_output_100ns = (start_time.elapsed().as_nanos() / 100) as i64;
        let mut chunk = vec![0.0f32; 16_384];
        let silence_frames = (sample_rate / AUDIO_SILENCE_CHUNK_DIVISOR).max(1) as usize;
        let silence_samples = silence_frames.saturating_mul(channels);
        let silence_chunk = vec![0.0f32; silence_samples];
        let mut silence_logged = false;

        while !stop_signal.load(Ordering::SeqCst) {
            let count = consumer.pop_slice(&mut chunk);
            if count > 0 {
                silence_logged = false;
                if let Some((bytes, duration_100ns)) =
                    encode_pcm_chunk_i16(&chunk[..count], channels, sample_rate)
                {
                    if let Err(e) = audio_handle.send_audio_buffer(bytes, audio_output_100ns) {
                        eprintln!("Audio mux send error: {}", e);
                        break;
                    }
                    audio_output_100ns = audio_output_100ns.saturating_add(duration_100ns);
                    continue;
                }
            }

            let wall_clock_100ns = (start_time.elapsed().as_nanos() / 100) as i64;
            let lag_100ns = wall_clock_100ns.saturating_sub(audio_output_100ns);
            if lag_100ns >= AUDIO_SILENCE_CATCHUP_THRESHOLD_100NS {
                if !silence_logged {
                    eprintln!(
                        "[AudioMux] loopback starved; injecting silence to keep encoder alive"
                    );
                    silence_logged = true;
                }

                let lag_frames = ((lag_100ns as i128) * (sample_rate as i128) / 10_000_000i128)
                    as usize;
                let frames_to_send = lag_frames.clamp(1, silence_frames);
                let samples_to_send = frames_to_send.saturating_mul(channels);
                if let Some((bytes, duration_100ns)) =
                    encode_pcm_chunk_i16(&silence_chunk[..samples_to_send], channels, sample_rate)
                {
                    if let Err(e) = audio_handle.send_audio_buffer(bytes, audio_output_100ns) {
                        eprintln!("Audio silence send error: {}", e);
                        break;
                    }
                    audio_output_100ns = audio_output_100ns.saturating_add(duration_100ns);
                    continue;
                }
            }

            thread::sleep(Duration::from_millis(AUDIO_POLL_SLEEP_MS));
        }

        println!("Audio stop signal received. Flushing buffer into muxer...");
        drop(loopback_stream);

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
