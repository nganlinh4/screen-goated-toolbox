use super::*;
use cpal::traits::StreamTrait;

#[test]
fn recoverable_stream_notifications_do_not_revoke_capture() {
    let failed = AtomicBool::new(false);
    for kind in [
        cpal::ErrorKind::Xrun,
        cpal::ErrorKind::DeviceChanged,
        cpal::ErrorKind::RealtimeDenied,
    ] {
        handle_stream_error(&failed, kind.into());
        assert!(!failed.load(Ordering::Acquire));
    }
    handle_stream_error(&failed, cpal::ErrorKind::DeviceNotAvailable.into());
    assert!(failed.load(Ordering::Acquire));
}

#[test]
#[ignore = "requires real Windows loopback and a public audio stimulus"]
fn live_system_output_sidecar_finalizes() {
    let output = std::env::var("SGT_LIVE_AUDIO_OUTPUT").unwrap();
    let started = Instant::now();
    let stop = Arc::new(AtomicBool::new(false));
    let finished = Arc::new(AtomicBool::new(false));
    let state = Arc::new(DeviceSidecarState::default());
    record_device_audio_sidecar(
        output.clone(),
        started,
        stop.clone(),
        finished.clone(),
        state.clone(),
    )
    .unwrap();
    eprintln!("LIVE_SIDECAR_READY");
    let seconds = std::env::var("SGT_LIVE_AUDIO_SECONDS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(10)
        .clamp(1, 900);
    thread::sleep(Duration::from_secs(seconds));
    state.stop_now(started);
    stop.store(true, Ordering::SeqCst);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !finished.load(Ordering::SeqCst) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(finished.load(Ordering::SeqCst));
    assert_eq!(state.error(), None);
    assert!(std::fs::metadata(output).unwrap().len() > 44);
}

#[test]
fn initial_future_device_epoch_is_aligned_once_with_a_stale_callback_clock() {
    let mut clock = CaptureClock {
        recording_start: Instant::now() - Duration::from_secs(2),
        sample_rate: 48_000,
        channels: 1,
        qpc_offset_ns: -8_000_000_000,
        previous_packet: None,
    };
    let (sender, receiver) = sync_channel(2);
    let failed = AtomicBool::new(false);
    for (capture_ns, callback_ns) in [(300_000_000, 0), (310_000_000, 10_000_000)] {
        let info = cpal::InputCallbackInfo::new(cpal::InputStreamTimestamp {
            capture: cpal::StreamInstant::new(10, capture_ns),
            callback: cpal::StreamInstant::new(10, callback_ns),
        });
        // The second packet is already captured before its delayed callback.
        if capture_ns > 300_000_000 {
            thread::sleep(Duration::from_millis(15));
        }
        submit_packet(
            &sender,
            &failed,
            &mut clock,
            &info,
            &[1i16; 480],
            |sample| *sample,
        );
    }
    assert!(!failed.load(Ordering::Acquire));
    let first = receiver.try_recv().unwrap();
    let second = receiver.try_recv().unwrap();
    assert!((95_520..=96_000).contains(&first.start_frame));
    assert_eq!(second.start_frame, first.start_frame + 480);
}

#[test]
#[ignore = "requires a real Windows output endpoint and a public audio stimulus"]
fn live_loopback_packet_timestamps() {
    let host = cpal::host_from_id(cpal::HostId::Wasapi).unwrap();
    let device = host.default_output_device().unwrap();
    let config = device.default_output_config().unwrap();
    let started = Instant::now();
    let clock =
        CaptureClock::calibrate(started, config.sample_rate(), config.channels() as usize).unwrap();
    let (sender, receiver) = sync_channel(256);
    let channels = config.channels() as usize;
    macro_rules! stream {
        ($sample:ty) => {
            device.build_input_stream(
                config.config(),
                move |data: &[$sample], info: &cpal::InputCallbackInfo| {
                    let timestamp = info.timestamp();
                    let _ = sender.try_send(serde_json::json!({
                        "capture_ns": timestamp.capture.as_nanos().to_string(),
                        "callback_ns": timestamp.callback.as_nanos().to_string(),
                        "mapped_ns": (clock.qpc_offset_ns + timestamp.capture.as_nanos() as i128).to_string(),
                        "observed_ns": started.elapsed().as_nanos().to_string(),
                        "frames": data.len() / channels,
                        "rate": config.sample_rate(),
                    }));
                },
                |error| eprintln!("loopback diagnostic stream: {error}"),
                None,
            )
        };
    }
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => stream!(f32),
        cpal::SampleFormat::I16 => stream!(i16),
        cpal::SampleFormat::I32 => stream!(i32),
        cpal::SampleFormat::U16 => stream!(u16),
        format => panic!("unsupported diagnostic sample format {format:?}"),
    }
    .unwrap();
    stream.play().unwrap();
    eprintln!("LIVE_LOOPBACK_READY");
    thread::sleep(Duration::from_secs(15));
    drop(stream);
    let packets: Vec<_> = receiver.try_iter().collect();
    assert!(
        !packets.is_empty(),
        "play a public audio stimulus during this test"
    );
    let output = std::env::var_os("SGT_LIVE_AUDIO_TIMESTAMPS").unwrap();
    std::fs::write(output, serde_json::to_vec_pretty(&packets).unwrap()).unwrap();
}
