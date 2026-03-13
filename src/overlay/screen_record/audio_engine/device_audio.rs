use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use windows_capture::encoder::AudioEncoderHandle;

use super::{
    AUDIO_POLL_SLEEP_MS, AUDIO_SILENCE_CATCHUP_THRESHOLD_100NS, AUDIO_SILENCE_CHUNK_DIVISOR,
    encode_pcm_chunk_i16, encode_pcm_i16_chunk, record_silence_only_audio,
};

pub(super) fn record_per_app_audio(
    audio_handle: AudioEncoderHandle,
    start_time: Instant,
    stop_signal: Arc<AtomicBool>,
    finished_signal: Arc<AtomicBool>,
    sample_rate: u32,
    channels: usize,
    process_id: u32,
) {
    #[cfg(not(target_os = "windows"))]
    {
        let _ = process_id;
        eprintln!("[AudioMux] per-app capture is only available on Windows");
        record_silence_only_audio(
            audio_handle,
            start_time,
            stop_signal,
            finished_signal,
            sample_rate,
            channels,
        );
    }

    #[cfg(target_os = "windows")]
    {
        use wasapi::{AudioClient, Direction, SampleType, StreamMode, WaveFormat};

        if channels == 0 || sample_rate == 0 {
            finished_signal.store(true, Ordering::SeqCst);
            return;
        }
        if wasapi::initialize_mta().is_err() {
            eprintln!("[AudioMux] failed to initialize MTA for per-app capture");
            record_silence_only_audio(
                audio_handle,
                start_time,
                stop_signal,
                finished_signal,
                sample_rate,
                channels,
            );
            return;
        }

        let mut audio_client = match AudioClient::new_application_loopback_client(process_id, true)
        {
            Ok(client) => client,
            Err(error) => {
                eprintln!(
                    "[AudioMux] failed to create per-app loopback client for PID {}: {:?}",
                    process_id, error
                );
                record_silence_only_audio(
                    audio_handle,
                    start_time,
                    stop_signal,
                    finished_signal,
                    sample_rate,
                    channels,
                );
                return;
            }
        };

        let desired_format = WaveFormat::new(
            16,
            16,
            &SampleType::Int,
            sample_rate as usize,
            channels,
            None,
        );
        let mode = StreamMode::EventsShared {
            autoconvert: true,
            buffer_duration_hns: 1_000_000,
        };
        if let Err(error) =
            audio_client.initialize_client(&desired_format, &Direction::Capture, &mode)
        {
            eprintln!(
                "[AudioMux] failed to initialize per-app audio client for PID {}: {:?}",
                process_id, error
            );
            record_silence_only_audio(
                audio_handle,
                start_time,
                stop_signal,
                finished_signal,
                sample_rate,
                channels,
            );
            return;
        }

        let capture_client = match audio_client.get_audiocaptureclient() {
            Ok(client) => client,
            Err(error) => {
                eprintln!("[AudioMux] failed to get per-app capture client: {:?}", error);
                record_silence_only_audio(
                    audio_handle,
                    start_time,
                    stop_signal,
                    finished_signal,
                    sample_rate,
                    channels,
                );
                return;
            }
        };

        let event_handle = match audio_client.set_get_eventhandle() {
            Ok(handle) => handle,
            Err(error) => {
                eprintln!("[AudioMux] failed to get per-app event handle: {:?}", error);
                record_silence_only_audio(
                    audio_handle,
                    start_time,
                    stop_signal,
                    finished_signal,
                    sample_rate,
                    channels,
                );
                return;
            }
        };

        if let Err(error) = audio_client.start_stream() {
            eprintln!(
                "[AudioMux] failed to start per-app capture for PID {}: {:?}",
                process_id, error
            );
            record_silence_only_audio(
                audio_handle,
                start_time,
                stop_signal,
                finished_signal,
                sample_rate,
                channels,
            );
            return;
        }

        println!(
            "Audio recording started (Per-app loopback): pid={}, {}Hz, {} channels",
            process_id, sample_rate, channels
        );

        let mut audio_output_100ns = (start_time.elapsed().as_nanos() / 100) as i64;
        let silence_frames = (sample_rate / AUDIO_SILENCE_CHUNK_DIVISOR).max(1) as usize;
        let silence_samples = silence_frames.saturating_mul(channels);
        let silence_chunk = vec![0.0f32; silence_samples];
        let mut silence_logged = false;
        let mut capture_buffer = VecDeque::<u8>::new();

        while !stop_signal.load(Ordering::SeqCst) {
            let mut sent_audio = false;
            let _ = event_handle.wait_for_event(100);
            match capture_client.read_from_device_to_deque(&mut capture_buffer) {
                Ok(_) => {
                    if capture_buffer.len() >= 2 {
                        silence_logged = false;
                        let sample_count = capture_buffer.len() / 2;
                        let mut samples = Vec::with_capacity(sample_count);
                        while capture_buffer.len() >= 2 {
                            let low = capture_buffer.pop_front().unwrap_or(0);
                            let high = capture_buffer.pop_front().unwrap_or(0);
                            samples.push(i16::from_le_bytes([low, high]));
                        }
                        if let Some((bytes, duration_100ns)) =
                            encode_pcm_i16_chunk(&samples, channels, sample_rate)
                        {
                            if let Err(error) =
                                audio_handle.send_audio_buffer(bytes, audio_output_100ns)
                            {
                                eprintln!("Per-app audio mux send error: {}", error);
                                break;
                            }
                            audio_output_100ns =
                                audio_output_100ns.saturating_add(duration_100ns);
                            sent_audio = true;
                        }
                    }
                }
                Err(error) => {
                    eprintln!("[AudioMux] per-app read error: {:?}", error);
                }
            }

            if sent_audio {
                continue;
            }

            let wall_clock_100ns = (start_time.elapsed().as_nanos() / 100) as i64;
            let lag_100ns = wall_clock_100ns.saturating_sub(audio_output_100ns);
            if lag_100ns >= AUDIO_SILENCE_CATCHUP_THRESHOLD_100NS {
                if !silence_logged {
                    eprintln!("[AudioMux] per-app capture starved; injecting silence");
                    silence_logged = true;
                }
                let lag_frames =
                    ((lag_100ns as i128) * (sample_rate as i128) / 10_000_000i128) as usize;
                let frames_to_send = lag_frames.clamp(1, silence_frames);
                let samples_to_send = frames_to_send.saturating_mul(channels);
                if let Some((bytes, duration_100ns)) =
                    encode_pcm_chunk_i16(&silence_chunk[..samples_to_send], channels, sample_rate)
                {
                    if let Err(error) = audio_handle.send_audio_buffer(bytes, audio_output_100ns) {
                        eprintln!("Per-app audio silence send error: {}", error);
                        break;
                    }
                    audio_output_100ns = audio_output_100ns.saturating_add(duration_100ns);
                    continue;
                }
            }

            thread::sleep(Duration::from_millis(AUDIO_POLL_SLEEP_MS));
        }

        let _ = audio_client.stop_stream();
        finished_signal.store(true, Ordering::SeqCst);
    }
}
