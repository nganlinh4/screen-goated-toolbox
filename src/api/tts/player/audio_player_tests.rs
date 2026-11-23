use super::{playback_format, playback_stalled, playback_work_present, synchronize_reset};
use std::time::{Duration, Instant};

#[test]
fn manager_stop_clears_queued_audio_and_requests_device_reset() {
    use std::collections::VecDeque;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    };
    let player = Arc::new(super::AudioPlayer {
        _sample_rate: 48_000,
        shared_buffer: Arc::new(Mutex::new(VecDeque::from(vec![1, 2, 3]))),
        device_padding_frames: Arc::new(AtomicU32::new(2)),
        submitted_frames: Arc::new(AtomicU64::new(2)),
        reset_generation: Arc::new(AtomicU64::new(0)),
        device_failure: Arc::new(super::DeviceFailure::default()),
        shutdown: Arc::new(AtomicBool::new(false)),
        deck_ready: Default::default(),
        _thread: None,
        wsola: Mutex::new(crate::api::tts::wsola::WsolaStretcher::new(24_000)),
    });
    let manager = crate::api::tts::manager::TtsManager::new();
    *manager.playback_device.lock().unwrap() = Arc::downgrade(&player);
    manager.stop();
    assert!(player.shared_buffer.lock().unwrap().is_empty());
    assert_eq!(player.reset_generation.load(Ordering::SeqCst), 1);
}

#[test]
fn stop_serializes_with_inflight_stretch_and_queue_handoff() {
    use std::collections::VecDeque;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, AtomicU64},
        mpsc,
    };
    let player = Arc::new(super::AudioPlayer {
        _sample_rate: 48_000,
        shared_buffer: Arc::new(Mutex::new(VecDeque::new())),
        device_padding_frames: Arc::new(AtomicU32::new(0)),
        submitted_frames: Arc::new(AtomicU64::new(0)),
        reset_generation: Arc::new(AtomicU64::new(0)),
        device_failure: Arc::new(super::DeviceFailure::default()),
        shutdown: Arc::new(AtomicBool::new(false)),
        deck_ready: Default::default(),
        _thread: None,
        wsola: Mutex::new(crate::api::tts::wsola::WsolaStretcher::new(24_000)),
    });
    let buffer = player.shared_buffer.lock().unwrap();
    let producer = Arc::clone(&player);
    let producer = std::thread::spawn(move || producer.play(&1000i16.to_le_bytes(), false));
    let deadline = Instant::now() + Duration::from_secs(2);
    while player.wsola.try_lock().is_ok() {
        assert!(
            Instant::now() < deadline,
            "play did not reach queue handoff"
        );
        std::thread::yield_now();
    }
    let stopper = Arc::clone(&player);
    let (done_tx, done_rx) = mpsc::channel();
    let stopper = std::thread::spawn(move || {
        stopper.stop();
        done_tx.send(()).unwrap();
    });
    assert!(done_rx.recv_timeout(Duration::from_millis(50)).is_err());
    drop(buffer);
    producer.join().unwrap();
    stopper.join().unwrap();
    assert!(player.shared_buffer.lock().unwrap().is_empty());
}

#[test]
fn stream_tail_is_enqueued_once_and_cancel_discards_it() {
    use std::collections::VecDeque;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, AtomicU64},
    };
    let player = super::AudioPlayer {
        _sample_rate: 48_000,
        shared_buffer: Arc::new(Mutex::new(VecDeque::new())),
        device_padding_frames: Arc::new(AtomicU32::new(0)),
        submitted_frames: Arc::new(AtomicU64::new(0)),
        reset_generation: Arc::new(AtomicU64::new(0)),
        device_failure: Arc::new(super::DeviceFailure::default()),
        shutdown: Arc::new(AtomicBool::new(false)),
        deck_ready: Default::default(),
        _thread: None,
        wsola: Mutex::new(crate::api::tts::wsola::WsolaStretcher::new(24_000)),
    };
    assert!(
        player
            .wsola
            .lock()
            .unwrap()
            .stretch(&[1000, 2000], 0.75)
            .is_empty()
    );
    assert!(player.flush_stretcher(&|| false));
    assert_eq!(player.shared_buffer.lock().unwrap().len(), 4);
    assert!(player.flush_stretcher(&|| false));
    assert_eq!(player.shared_buffer.lock().unwrap().len(), 4);
    assert!(
        player
            .wsola
            .lock()
            .unwrap()
            .stretch(&[3000], 0.75)
            .is_empty()
    );
    assert!(!player.flush_stretcher(&|| true));
    assert!(player.shared_buffer.lock().unwrap().is_empty());
    assert!(player.wsola.lock().unwrap().finish().is_empty());
}

#[test]
fn stream_format_matches_queued_pcm_independent_of_endpoint_format() {
    let format = playback_format(48_000);
    let (tag, channels, rate, bytes_per_second, alignment, bits) = (
        format.wFormatTag,
        format.nChannels,
        format.nSamplesPerSec,
        format.nAvgBytesPerSec,
        format.nBlockAlign,
        format.wBitsPerSample,
    );
    assert_eq!(
        (tag, channels, rate, bytes_per_second, alignment, bits),
        (1, 1, 48_000, 96_000, 2, 16)
    );
}

#[test]
fn long_playback_remains_active_until_rendering_stalls() {
    let start = Instant::now();
    let mut last_progress = start;
    let mut last_submitted = 0;
    for second in 1..=90 {
        assert!(!playback_stalled(
            &mut last_submitted,
            &mut last_progress,
            second * 48_000,
            start + Duration::from_secs(second)
        ));
    }
    let submitted = last_submitted;
    assert!(!playback_stalled(
        &mut last_submitted,
        &mut last_progress,
        submitted,
        start + Duration::from_secs(120)
    ));
    assert!(playback_stalled(
        &mut last_submitted,
        &mut last_progress,
        submitted,
        start + Duration::from_secs(121)
    ));
}

#[test]
fn device_padding_remains_pending_work_after_shared_queue_handoff() {
    assert!(playback_work_present(true, 0));
    assert!(playback_work_present(false, 480));
    assert!(!playback_work_present(false, 0));
}

#[test]
fn cancellation_while_only_device_padding_remains_requests_reset() {
    use std::collections::VecDeque;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        mpsc,
    };
    let player = Arc::new(super::AudioPlayer {
        _sample_rate: 48_000,
        shared_buffer: Arc::new(Mutex::new(VecDeque::new())),
        device_padding_frames: Arc::new(AtomicU32::new(480)),
        submitted_frames: Arc::new(AtomicU64::new(480)),
        reset_generation: Arc::new(AtomicU64::new(0)),
        device_failure: Arc::new(super::DeviceFailure::default()),
        shutdown: Arc::new(AtomicBool::new(false)),
        deck_ready: Default::default(),
        _thread: None,
        wsola: Mutex::new(crate::api::tts::wsola::WsolaStretcher::new(24_000)),
    });
    let cancelled = Arc::new(AtomicBool::new(false));
    let (observed_tx, observed_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let thread_player = player.clone();
    let thread_cancelled = cancelled.clone();
    std::thread::spawn(move || {
        let first = AtomicBool::new(true);
        let drained = thread_player.drain(|| {
            if first.swap(false, Ordering::SeqCst) {
                let _ = observed_tx.send(());
                false
            } else {
                thread_cancelled.load(Ordering::SeqCst)
            }
        });
        let _ = done_tx.send(drained);
    });
    observed_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    cancelled.store(true, Ordering::SeqCst);
    assert!(!done_rx.recv_timeout(Duration::from_secs(1)).unwrap());
    assert_eq!(player.reset_generation.load(Ordering::SeqCst), 1);
}

#[test]
fn device_failure_with_unconsumed_frames_cannot_complete_drain() {
    use std::collections::VecDeque;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        mpsc,
    };
    let failure = Arc::new(super::DeviceFailure::default());
    failure.set_padding(480);
    let player = Arc::new(super::AudioPlayer {
        _sample_rate: 48_000,
        shared_buffer: Arc::new(Mutex::new(VecDeque::new())),
        device_padding_frames: Arc::new(AtomicU32::new(480)),
        submitted_frames: Arc::new(AtomicU64::new(480)),
        reset_generation: Arc::new(AtomicU64::new(0)),
        device_failure: failure.clone(),
        shutdown: Arc::new(AtomicBool::new(false)),
        deck_ready: Default::default(),
        _thread: None,
        wsola: Mutex::new(crate::api::tts::wsola::WsolaStretcher::new(24_000)),
    });
    let (observed_tx, observed_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let drain_player = player.clone();
    std::thread::spawn(move || {
        let first = AtomicBool::new(true);
        let drained = drain_player.drain(|| {
            if first.swap(false, Ordering::SeqCst) {
                let _ = observed_tx.send(());
            }
            drain_player.failure_generation() != 0
        });
        let _ = done_tx.send(drained);
    });
    observed_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    {
        let _deck = player.shared_buffer.lock().unwrap();
        failure.finish_stream(true, 0);
        player.device_padding_frames.store(0, Ordering::Release);
    }
    assert!(!done_rx.recv_timeout(Duration::from_secs(1)).unwrap());
}

#[test]
fn next_owner_baseline_waits_for_old_device_accounting() {
    use std::collections::VecDeque;
    use std::sync::{
        Arc, Barrier, Mutex,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        mpsc,
    };
    let deck = Arc::new(Mutex::new(VecDeque::new()));
    let played = Arc::new(AtomicU64::new(0));
    let progress = Arc::new(crate::api::tts::manager::PlaybackProgress::new(
        played.clone(),
        Arc::new(AtomicBool::new(false)),
    ));
    let player = Arc::new(super::AudioPlayer {
        _sample_rate: 48_000,
        shared_buffer: deck.clone(),
        device_padding_frames: Arc::new(AtomicU32::new(0)),
        submitted_frames: Arc::new(AtomicU64::new(0)),
        reset_generation: Arc::new(AtomicU64::new(0)),
        device_failure: Arc::new(super::DeviceFailure::default()),
        shutdown: Arc::new(AtomicBool::new(false)),
        deck_ready: Default::default(),
        _thread: None,
        wsola: Mutex::new(crate::api::tts::wsola::WsolaStretcher::new(24_000)),
    });
    let entered = Arc::new(Barrier::new(2));
    let renderer_entered = entered.clone();
    let renderer_deck = deck.clone();
    let renderer_played = played.clone();
    let (release_tx, release_rx) = mpsc::channel();
    let renderer = std::thread::spawn(move || {
        super::with_output_deck(&renderer_deck, || {
            renderer_entered.wait();
            release_rx.recv().unwrap();
            renderer_played.fetch_add(120, Ordering::Release);
        });
    });
    entered.wait();
    let (started_tx, started_rx) = mpsc::channel();
    let next_player = player.clone();
    let next_progress = progress.clone();
    let next = std::thread::spawn(move || {
        next_player.stop();
        next_player.begin_request(Some(&next_progress));
        started_tx.send(()).unwrap();
    });
    assert!(started_rx.recv_timeout(Duration::from_millis(50)).is_err());
    release_tx.send(()).unwrap();
    started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    renderer.join().unwrap();
    next.join().unwrap();
    played.fetch_add(80, Ordering::Release);
    assert_eq!(progress.played_frames(), 80);
}

#[test]
fn reset_completes_before_new_owner_is_submitted_and_retries_failures() {
    let mut applied = 0;
    let mut events = Vec::new();
    assert!(
        synchronize_reset(&mut applied, 1, || {
            events.push("failed reset");
            anyhow::bail!("device reset failed")
        })
        .is_err()
    );
    assert_eq!(applied, 0);
    synchronize_reset(&mut applied, 1, || {
        events.push("reset");
        Ok(())
    })
    .unwrap();
    events.push("submit new owner");
    synchronize_reset(&mut applied, 1, || panic!("reset repeated")).unwrap();
    assert_eq!(events, ["failed reset", "reset", "submit new owner"]);
}

#[test]
#[ignore = "plays a short quiet generated tone on the configured Windows output"]
fn native_playback_device_acceptance() {
    let player = super::AudioPlayer::new(crate::api::tts::types::PLAYBACK_SAMPLE_RATE);
    let sample_rate = crate::api::tts::types::SOURCE_SAMPLE_RATE;
    let count = sample_rate as usize * 3 / 4;
    let pcm = (0..count)
        .flat_map(|index| {
            let envelope = (index.min(count - index) as f32 / 240.0).min(1.0);
            let phase = index as f32 * 440.0 * std::f32::consts::TAU / sample_rate as f32;
            ((phase.sin() * 1600.0 * envelope) as i16).to_le_bytes()
        })
        .collect::<Vec<_>>();
    player.play_native_stream(&pcm);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while player.has_pending_playback() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let drained = !player.has_pending_playback();
    let submitted = player.submitted_frames();
    player.stop();
    eprintln!("[PlaybackAcceptance] submitted_frames={submitted} drained={drained}");
    assert!(
        submitted > 0,
        "Windows did not accept generated audio frames"
    );
    assert!(
        drained,
        "generated audio did not drain from queue and device"
    );
}

#[test]
fn idle_render_thread_parks_until_audio_or_reset_wakes_it() {
    use std::collections::VecDeque;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    };
    let player = Arc::new(super::AudioPlayer {
        _sample_rate: 48_000,
        shared_buffer: Arc::new(Mutex::new(VecDeque::new())),
        device_padding_frames: Arc::new(AtomicU32::new(0)),
        submitted_frames: Arc::new(AtomicU64::new(0)),
        reset_generation: Arc::new(AtomicU64::new(0)),
        device_failure: Arc::new(super::DeviceFailure::default()),
        shutdown: Arc::new(AtomicBool::new(false)),
        deck_ready: Default::default(),
        _thread: None,
        wsola: Mutex::new(crate::api::tts::wsola::WsolaStretcher::new(24_000)),
    });
    let park = |player: Arc<super::AudioPlayer>| {
        std::thread::spawn(move || {
            let started = Instant::now();
            let deck = super::park_until_work(
                &player.deck_ready,
                player.shared_buffer.lock().unwrap(),
                || player.reset_generation.load(Ordering::SeqCst) != 0,
            );
            (deck.len(), started.elapsed())
        })
    };

    let parked = park(player.clone());
    std::thread::sleep(Duration::from_millis(100));
    player.enqueue(vec![7, 7]);
    let (queued, waited) = parked.join().unwrap();
    assert_eq!(queued, 2);
    assert!(
        waited < super::IDLE_PARK / 2,
        "audio did not wake: {waited:?}"
    );

    player.shared_buffer.lock().unwrap().clear();
    let parked = park(player.clone());
    std::thread::sleep(Duration::from_millis(100));
    player.stop();
    let (_, waited) = parked.join().unwrap();
    assert!(
        waited < super::IDLE_PARK / 2,
        "reset did not wake: {waited:?}"
    );
}
