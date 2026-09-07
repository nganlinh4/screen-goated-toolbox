use super::*;

/// Start per-app audio capture using WASAPI process loopback (Windows 10 1903+)
///
/// This function spawns a thread that captures audio from a specific process
/// and pushes samples to the provided buffer.
#[cfg(target_os = "windows")]
pub fn start_per_app_capture(
    process_id: u32,
    audio_buffer: Arc<Mutex<Vec<i16>>>,
    stop_signal: Arc<AtomicBool>,
    pause_signal: Arc<AtomicBool>,
) -> Result<CaptureWorker> {
    use std::collections::VecDeque;
    use wasapi::{AudioClient, Direction, SampleType, StreamMode, WaveFormat};

    CaptureWorker::spawn("sgt-per-app-capture", move |capture_stop| {
        // Initialize COM for this thread (required for WASAPI)
        if wasapi::initialize_mta().is_err() {
            eprintln!("Per-app capture: Failed to initialize MTA");
            return;
        }

        // Create loopback capture client for the specified process
        // include_tree=true to include child processes (browsers often use separate audio processes)
        let audio_client = match AudioClient::new_application_loopback_client(process_id, true) {
            Ok(client) => client,
            Err(e) => {
                eprintln!(
                    "Per-app capture: Failed to create loopback client for PID {}: {:?}",
                    process_id, e
                );
                return;
            }
        };

        // Configure desired format: 16kHz mono 16-bit (what Gemini expects)
        // With autoconvert=true, Windows will handle resampling from the app's native format
        let desired_format = WaveFormat::new(
            16, // bits per sample
            16, // valid bits
            &SampleType::Int,
            16000, // 16kHz sample rate
            1,     // mono
            None,
        );

        // Buffer duration: 100ms in 100-nanosecond units
        let buffer_duration_hns = 1_000_000i64; // 100ms

        // Configure stream mode with auto-conversion
        let mode = StreamMode::EventsShared {
            autoconvert: true,
            buffer_duration_hns,
        };

        let mut audio_client = audio_client;
        if let Err(e) = audio_client.initialize_client(&desired_format, &Direction::Capture, &mode)
        {
            eprintln!(
                "Per-app capture: Failed to initialize audio client: {:?}",
                e
            );
            eprintln!("Hint: Per-app capture requires Windows 10 version 1903 or later");
            return;
        }

        // Get the capture client interface
        let capture_client = match audio_client.get_audiocaptureclient() {
            Ok(client) => client,
            Err(e) => {
                eprintln!("Per-app capture: Failed to get capture client: {:?}", e);
                return;
            }
        };

        // Get event handle for efficient waiting
        let event_handle = match audio_client.set_get_eventhandle() {
            Ok(handle) => handle,
            Err(e) => {
                eprintln!("Per-app capture: Failed to get event handle: {:?}", e);
                return;
            }
        };

        // Start the audio stream
        if let Err(e) = audio_client.start_stream() {
            eprintln!("Per-app capture: Failed to start stream: {:?}", e);
            return;
        }

        // Per-app capture started for process_id

        // Buffer for reading audio data
        let mut capture_buffer: VecDeque<u8> = VecDeque::new();

        // Capture loop
        while !stop_signal.load(Ordering::Relaxed) && !capture_stop.load(Ordering::Acquire) {
            if pause_signal.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
            // Wait for buffer to be ready (up to 100ms timeout)
            if event_handle.wait_for_event(100).is_err() {
                continue; // Timeout, check stop signal and try again
            }

            if capture_stop.load(Ordering::Acquire) || stop_signal.load(Ordering::Relaxed) {
                break;
            }

            // Read captured data
            match capture_client.read_from_device_to_deque(&mut capture_buffer) {
                Ok(_buffer_info) => {
                    // Check if we received any data
                    if !capture_buffer.is_empty() {
                        // Convert bytes to i16 samples (16-bit = 2 bytes per sample)
                        // Format is 16-bit mono at 16kHz
                        let bytes_per_sample = 2;
                        let sample_count = capture_buffer.len() / bytes_per_sample;

                        if sample_count > 0 {
                            // Drain buffer and convert to i16
                            let mut samples: Vec<i16> = Vec::with_capacity(sample_count);

                            while capture_buffer.len() >= bytes_per_sample {
                                let low = capture_buffer.pop_front().unwrap_or(0);
                                let high = capture_buffer.pop_front().unwrap_or(0);
                                let sample = i16::from_le_bytes([low, high]);
                                samples.push(sample);
                            }

                            // Audio received from per-app capture - add to buffer

                            // Push to shared audio buffer
                            if let Ok(mut buf) = audio_buffer.lock() {
                                buf.extend(&samples);
                            }

                            // Calculate RMS for volume visualization
                            if !samples.is_empty() {
                                let sum_sq: f64 =
                                    samples.iter().map(|&s| (s as f64 / 32768.0).powi(2)).sum();
                                let rms = (sum_sq / samples.len() as f64).sqrt() as f32;
                                REALTIME_RMS.store(rms.to_bits(), Ordering::Relaxed);
                            }
                        }
                    }
                }
                Err(e) => {
                    // Check for specific errors that indicate process ended or connection lost
                    eprintln!("Per-app capture: Read error: {:?}", e);
                    // Small delay before retrying
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        }

        // Cleanup
        let _ = audio_client.stop_stream();
        // Per-app capture stopped
    })
}
