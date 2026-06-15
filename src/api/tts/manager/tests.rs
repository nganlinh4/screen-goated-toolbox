use super::TtsManager;
use crate::api::tts::types::AudioEvent;
use crate::api::tts::types::TtsRequestProfile;
use crate::config::{StepAudioSettings, TtsMethod};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

fn step_audio_e2e_profile() -> TtsRequestProfile {
    TtsRequestProfile {
        method: TtsMethod::StepAudioEditX,
        gemini_model: String::new(),
        gemini_voice: String::new(),
        gemini_speed: String::new(),
        gemini_instruction: String::new(),
        gemini_language_conditions: Vec::new(),
        gemini_parallel_requests: 2,
        google_speed: String::new(),
        edge_voice: String::new(),
        edge_settings: Default::default(),
        step_audio_settings: StepAudioSettings::default(),
        magpie_settings: Default::default(),
        kokoro_settings: Default::default(),
        supertonic_settings: Default::default(),
        vieneu_settings: Default::default(),
        language_code_override: Some("eng".to_string()),
    }
}

#[test]
fn synthesize_to_wav_with_step_audio_profile_e2e_when_enabled() {
    if std::env::var("SGT_STEP_AUDIO_MANAGER_E2E").as_deref() != Ok("1") {
        eprintln!(
            "skipping Step Audio manager e2e test; set SGT_STEP_AUDIO_MANAGER_E2E=1 to run it"
        );
        return;
    }

    let manager = Arc::new(TtsManager::new());
    let worker_manager = manager.clone();
    let worker = std::thread::spawn(move || {
        crate::api::tts::worker::run_socket_worker(worker_manager);
    });

    let audio = manager
        .synthesize_to_wav_with_profile(
            "Step Audio manager artifact path test.",
            step_audio_e2e_profile(),
        )
        .expect("synthesize Step Audio artifact");

    assert_eq!(audio.sample_rate, 24_000);
    assert!(
        audio.duration_ms >= 500,
        "Step Audio artifact is too short: {}ms",
        audio.duration_ms
    );
    assert!(
        audio.wav_data.len() > 44,
        "Step Audio artifact WAV data should include samples"
    );

    let start_sample = (audio.pcm_samples.len() / 4).min(24_000);
    let playback_id = manager.play_pcm_interrupt(audio.pcm_samples.clone(), start_sample);
    let playback_job = manager
        .playback_queue
        .lock()
        .expect("lock playback queue")
        .pop_front()
        .expect("playback job queued");
    let (rx, hwnd, req_id, _generation, is_realtime) = playback_job;
    assert_eq!(hwnd, 0);
    assert_eq!(req_id, playback_id);
    assert!(!is_realtime);

    let mut playback_bytes = 0usize;
    loop {
        match rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("playback PCM event")
        {
            AudioEvent::Data(data) => playback_bytes += data.len(),
            AudioEvent::Error(error) => panic!("playback error: {error}"),
            AudioEvent::End => break,
        }
    }
    assert!(
        playback_bytes > 0,
        "Step Audio generated PCM should enqueue playable audio bytes"
    );

    manager._shutdown();
    worker.join().expect("join Step Audio worker");
}

#[test]
fn step_audio_playback_loopback_e2e_when_enabled() {
    if std::env::var("SGT_STEP_AUDIO_LOOPBACK_E2E").as_deref() != Ok("1") {
        eprintln!(
            "skipping Step Audio loopback e2e test; set SGT_STEP_AUDIO_LOOPBACK_E2E=1 to run it"
        );
        return;
    }

    let manager = Arc::new(TtsManager::new());
    let worker_manager = manager.clone();
    let worker = std::thread::spawn(move || {
        crate::api::tts::worker::run_socket_worker(worker_manager);
    });

    let player_manager = manager.clone();
    let player = std::thread::spawn(move || {
        crate::api::tts::player::run_player_thread(player_manager);
    });

    let host = cpal::host_from_id(cpal::HostId::Wasapi).unwrap_or_else(|_| cpal::default_host());
    let device = host
        .default_output_device()
        .expect("default output device for loopback capture");
    let config = device
        .default_output_config()
        .expect("default output config for loopback capture");
    let stream_config: cpal::StreamConfig = config.clone().into();
    let captured_samples = Arc::new(AtomicU64::new(0));
    let captured_energy = Arc::new(AtomicU64::new(0));
    let capture_active = Arc::new(AtomicBool::new(true));
    let samples_for_callback = captured_samples.clone();
    let energy_for_callback = captured_energy.clone();
    let active_for_callback = capture_active.clone();
    let err_fn = |err| eprintln!("[StepAudioLoopbackE2E] loopback stream error: {err}");

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(
            stream_config.clone(),
            move |data: &[f32], _: &_| {
                if !active_for_callback.load(Ordering::Relaxed) {
                    return;
                }
                samples_for_callback.fetch_add(data.len() as u64, Ordering::Relaxed);
                let energy: u64 = data
                    .iter()
                    .map(|sample| (sample.abs() * 1_000_000.0) as u64)
                    .sum();
                energy_for_callback.fetch_add(energy, Ordering::Relaxed);
            },
            err_fn,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_input_stream(
            stream_config.clone(),
            move |data: &[i16], _: &_| {
                if !active_for_callback.load(Ordering::Relaxed) {
                    return;
                }
                samples_for_callback.fetch_add(data.len() as u64, Ordering::Relaxed);
                let energy: u64 = data.iter().map(|sample| sample.unsigned_abs() as u64).sum();
                energy_for_callback.fetch_add(energy, Ordering::Relaxed);
            },
            err_fn,
            None,
        ),
        cpal::SampleFormat::U16 => device.build_input_stream(
            stream_config.clone(),
            move |data: &[u16], _: &_| {
                if !active_for_callback.load(Ordering::Relaxed) {
                    return;
                }
                samples_for_callback.fetch_add(data.len() as u64, Ordering::Relaxed);
                let energy: u64 = data
                    .iter()
                    .map(|sample| sample.abs_diff(32768) as u64)
                    .sum();
                energy_for_callback.fetch_add(energy, Ordering::Relaxed);
            },
            err_fn,
            None,
        ),
        other => panic!("unsupported loopback sample format: {other:?}"),
    }
    .expect("build default output loopback stream");
    stream.play().expect("start default output loopback stream");

    let audio = manager
        .synthesize_to_wav_with_profile(
            "Step Audio loopback playback test.",
            step_audio_e2e_profile(),
        )
        .expect("synthesize Step Audio for loopback playback");
    assert!(
        audio.duration_ms >= 500,
        "Step Audio loopback source is too short: {}ms",
        audio.duration_ms
    );

    manager.play_pcm_interrupt(audio.pcm_samples, 0);
    let started = Instant::now();
    while manager.has_pending_audio() && started.elapsed() < Duration::from_secs(45) {
        std::thread::sleep(Duration::from_millis(100));
    }
    std::thread::sleep(Duration::from_millis(500));
    capture_active.store(false, Ordering::Relaxed);
    drop(stream);

    let samples = captured_samples.load(Ordering::Relaxed);
    let energy = captured_energy.load(Ordering::Relaxed);
    manager._shutdown();
    worker.join().expect("join Step Audio worker");
    player.join().expect("join TTS player");

    assert!(
        samples > 0,
        "loopback capture should receive render-device samples"
    );
    assert!(
        energy > 0,
        "Step Audio playback should produce non-silent loopback output"
    );
}
