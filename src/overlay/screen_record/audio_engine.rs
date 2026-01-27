use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use hound::{SampleFormat, WavSpec, WavWriter};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub fn record_audio(path: String, stop_signal: Arc<AtomicBool>, finished_signal: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let host = match cpal::host_from_id(cpal::HostId::Wasapi) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("Failed to get WASAPI host: {}", e);
                finished_signal.store(true, Ordering::SeqCst);
                return;
            }
        };

        // For system audio loopback, we use the default output device
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

        let sample_rate = config.sample_rate();

        let spec = WavSpec {
            channels: config.channels() as u16,
            sample_rate,
            bits_per_sample: 32,
            sample_format: SampleFormat::Float,
        };

        let writer = match WavWriter::create(&path, spec) {
            Ok(w) => Arc::new(Mutex::new(Some(w))),
            Err(e) => {
                eprintln!("Failed to create WAV writer: {}", e);
                finished_signal.store(true, Ordering::SeqCst);
                return;
            }
        };

        let writer_clone = writer.clone();

        let stream = match device.build_input_stream(
            &config.into(),
            move |data: &[f32], _: &_| {
                if let Ok(mut guard) = writer_clone.lock() {
                    if let Some(ref mut w) = *guard {
                        for &sample in data {
                            let _ = w.write_sample(sample);
                        }
                    }
                }
            },
            |err| eprintln!("Audio stream error: {}", err),
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

        println!("Audio recording started: {}", path);

        while !stop_signal.load(Ordering::SeqCst) {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        drop(stream);

        if let Ok(mut guard) = writer.lock() {
            if let Some(w) = guard.take() {
                let _ = w.finalize();
            }
        }

        println!("Audio recording finished: {}", path);
        finished_signal.store(true, Ordering::SeqCst);
    });
}
