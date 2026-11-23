use super::*;
use crate::overlay::computer_control::controller::world::SurfaceIdentity;

fn frame(frame_id: u64, captured_at: Instant) -> uia_task::SnapshotFrame {
    uia_task::SnapshotFrame {
        b64: "AQID".into(),
        source: uia_task::FrameSource {
            view: super::super::session::View {
                x: 0,
                y: 0,
                w: 100,
                h: 100,
            },
            frame_id,
            surface: SurfaceIdentity::Native {
                hwnd: frame_id,
                pid: 2,
                generation: 3,
            },
        },
        captured_at,
        byte_count: 3,
    }
}

fn activity() -> Activity {
    Activity {
        active: true,
        action_pending: false,
        playing: false,
        awaiting: true,
        engaged: true,
        recent_voice: false,
        voiced: false,
        uncommitted_audio: false,
    }
}

#[test]
fn pending_input_rejects_pre_onset_and_stale_frames_even_after_transcript() {
    let _owner = super::super::super::session_retention::start_session();
    let old = Instant::now() - Duration::from_secs(60);
    let cache = FrameCache::new(Some(frame(1, old)));
    let mut state = Reader::default();
    state.frame_delivery.begin_input();
    state.frame_delivery.confirm_input();
    let requested = state.frame_delivery.request.unwrap().at;
    for captured_at in [old, requested - Duration::from_millis(1)] {
        cache.shared.state.lock().unwrap().frame = Some(Arc::new(frame(2, captured_at)));
        cache
            .deliver_due(&mut state, activity(), |_, _| panic!("old frame delivered"))
            .unwrap();
        assert!(state.frame_delivery.pending());
        assert!(state.source_frame.is_none());
    }
    cache.shared.state.lock().unwrap().frame = Some(Arc::new(frame(3, Instant::now())));
    let mut delivered = None;
    cache
        .deliver_due(&mut state, activity(), |frame, trigger| {
            delivered = Some((frame.source.frame_id, trigger.to_owned()));
            Ok(())
        })
        .unwrap();
    assert_eq!(delivered, Some((3, "user_input".into())));
    assert_eq!(state.source_frame.unwrap().frame_id, 3);
    assert!(!state.frame_delivery.pending());
}

#[test]
fn pending_input_survives_playback_and_a_pending_action() {
    let _owner = super::super::super::session_retention::start_session();
    let mut state = Reader::default();
    state.frame_delivery.begin_input();
    let cache = FrameCache::new(Some(frame(1, Instant::now())));
    let mut live = activity();
    live.playing = true;
    live.action_pending = true;
    live.uncommitted_audio = true;
    let mut sent = false;
    cache
        .deliver_due(&mut state, live, |_, _| {
            sent = true;
            Ok(())
        })
        .unwrap();
    assert!(sent);
    assert_eq!(state.source_frame.unwrap().frame_id, 1);
    assert!(!state.frame_delivery.pending());
}

#[test]
fn failed_delivery_keeps_request_and_does_not_bind_source_or_memory() {
    let _owner = super::super::super::session_retention::start_session();
    let mut state = Reader::default();
    state.frame_delivery.begin_input();
    let cache = FrameCache::new(Some(frame(1, Instant::now())));
    assert!(
        cache
            .deliver_due(&mut state, activity(), |_, _| anyhow::bail!(
                "closed socket"
            ))
            .is_err()
    );
    assert!(state.frame_delivery.pending());
    assert!(state.source_frame.is_none());
    state.connection_generation += 1;
    state
        .frame_delivery
        .observe_connection(state.connection_generation, Instant::now());
    assert!(
        state.frame_delivery.pending(),
        "transport recovery must not lose the observation request"
    );
    cache
        .deliver_due(&mut state, activity(), |_, _| {
            panic!("pre-reconnect cache delivered")
        })
        .unwrap();
}

#[test]
fn idle_expired_noise_and_completed_turns_retire_pending_capture() {
    let now = Instant::now();
    let mut delivery = FrameDelivery::default();
    delivery.begin_input();
    let mut idle = activity();
    idle.active = false;
    idle.engaged = false;
    assert_eq!(
        delivery.trigger(&idle, now + INPUT_REQUEST_WINDOW + Duration::from_secs(1)),
        None
    );
    assert!(!delivery.pending());
    delivery.begin_input();
    delivery.confirm_input();
    assert_eq!(delivery.trigger(&idle, Instant::now()), None);
    assert!(!delivery.pending());
}

#[test]
fn ordinary_periodic_frames_respect_age_playback_and_action_ownership() {
    let _owner = super::super::super::session_retention::start_session();
    let mut state = Reader::default();
    let now = Instant::now();
    state.frame_delivery.last_sent = now - FRAME_INTERVAL - Duration::from_secs(1);
    let cache = FrameCache::new(Some(frame(1, now - MAX_FRAME_AGE - Duration::from_secs(1))));
    let mut ready = activity();
    ready.awaiting = false;
    cache
        .deliver_due(&mut state, ready, |_, _| {
            panic!("stale periodic frame delivered")
        })
        .unwrap();
    cache.shared.state.lock().unwrap().frame = Some(Arc::new(frame(2, Instant::now())));
    for (playing, pending) in [(true, false), (false, true)] {
        let mut busy = activity();
        busy.awaiting = false;
        busy.playing = playing;
        busy.action_pending = pending;
        cache
            .deliver_due(&mut state, busy, |_, _| {
                panic!("ambient frame competed with owned output/action")
            })
            .unwrap();
    }
    let mut ready = activity();
    ready.awaiting = false;
    cache
        .deliver_due(&mut state, ready, |_, trigger| {
            assert_eq!(trigger, "periodic");
            Ok(())
        })
        .unwrap();
    assert_eq!(state.source_frame.unwrap().frame_id, 2);
}

#[test]
fn confirmation_reuses_onset_request_and_text_without_onset_starts_one() {
    let mut delivery = FrameDelivery::default();
    assert!(delivery.confirm_input());
    let first = delivery.request.unwrap().at;
    delivery.begin_input();
    let onset = delivery.request.unwrap().at;
    assert!(onset >= first);
    assert!(!delivery.confirm_input());
    assert_eq!(delivery.request.unwrap().at, onset);
    // The leading frame may already have arrived while the user was speaking.
    delivery.begin_input();
    delivery.request = None;
    assert!(!delivery.confirm_input());
    assert!(!delivery.pending());
}

#[test]
fn fresh_request_wakes_idle_worker_and_rejects_capture_already_in_flight() {
    let _owner = super::super::super::session_retention::start_session();
    let cache = FrameCache::new(None);
    cache.shared.state.lock().unwrap().enabled = false;
    let shared = Arc::clone(&cache.shared);
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = Arc::clone(&stop);
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let mut id = 0;
        capture_loop(
            shared,
            worker_stop,
            || {
                id += 1;
                let captured_at = Instant::now();
                started_tx.send(id)?;
                release_rx.recv_timeout(Duration::from_secs(2))?;
                Ok(frame(id, captured_at))
            },
            None,
        );
    });
    let mut state = Reader::default();
    state.frame_delivery.begin_input();
    cache
        .deliver_due(&mut state, activity(), |_, _| {
            panic!("empty cache delivered")
        })
        .unwrap();
    assert_eq!(started_rx.recv_timeout(Duration::from_secs(2)).unwrap(), 1);
    state.frame_delivery.begin_input();
    cache
        .deliver_due(&mut state, activity(), |_, _| {
            panic!("in-flight frame delivered")
        })
        .unwrap();
    release_tx.send(()).unwrap();
    assert_eq!(started_rx.recv_timeout(Duration::from_secs(2)).unwrap(), 2);
    cache
        .deliver_due(&mut state, activity(), |_, _| {
            panic!("previous capture satisfied new input")
        })
        .unwrap();
    release_tx.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while cache
        .shared
        .state
        .lock()
        .unwrap()
        .frame
        .as_ref()
        .is_none_or(|frame| frame.source.frame_id != 2)
    {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    cache
        .deliver_due(&mut state, activity(), |frame, _| {
            assert_eq!(frame.source.frame_id, 2);
            Ok(())
        })
        .unwrap();
    stop.store(true, Ordering::SeqCst);
    cache.shared.wake.notify_one();
    worker.join().unwrap();
}

#[test]
fn action_source_cannot_replace_input_frame_before_confirmation_and_noise_expires() {
    let _owner = super::super::super::session_retention::start_session();
    let mut state = Reader::default();
    state.frame_delivery.begin_input();
    assert!(!state.frame_delivery.accepts_action_source());
    let cache = FrameCache::new(Some(frame(1, Instant::now())));
    cache
        .deliver_due(&mut state, activity(), |_, _| Ok(()))
        .unwrap();
    assert!(!state.frame_delivery.accepts_action_source());
    assert!(!state.frame_delivery.confirm_input());
    assert!(state.frame_delivery.accepts_action_source());
    state.frame_delivery.begin_input();
    cache.shared.state.lock().unwrap().frame = Some(Arc::new(frame(2, Instant::now())));
    cache
        .deliver_due(&mut state, activity(), |_, _| Ok(()))
        .unwrap();
    let expired = Instant::now() + INPUT_REQUEST_WINDOW + Duration::from_secs(1);
    state.frame_delivery.trigger(&activity(), expired);
    assert!(state.frame_delivery.accepts_action_source());
}

#[test]
fn silence_refresh_requires_capture_after_request_even_with_young_cache() {
    let _owner = super::super::super::session_retention::start_session();
    let cache = FrameCache::new(Some(frame(1, Instant::now())));
    let mut state = Reader::default();
    state
        .frame_delivery
        .request("silence_nudge", Instant::now());
    cache
        .deliver_due(&mut state, activity(), |_, _| {
            panic!("pre-request nudge frame delivered")
        })
        .unwrap();
    assert!(state.frame_delivery.pending());
    cache.shared.state.lock().unwrap().frame = Some(Arc::new(frame(2, Instant::now())));
    cache
        .deliver_due(&mut state, activity(), |_, trigger| {
            assert_eq!(trigger, "silence_nudge");
            Ok(())
        })
        .unwrap();
    assert_eq!(state.source_frame.unwrap().frame_id, 2);
}

#[test]
fn pending_observation_requires_capture_after_an_old_action_settles() {
    let _owner = super::super::super::session_retention::start_session();
    let mut state = Reader::default();
    state.frame_delivery.begin_input();
    let cache = FrameCache::new(Some(frame(1, Instant::now())));
    state.frame_delivery.defer_periodic();
    cache
        .deliver_due(&mut state, activity(), |_, _| {
            panic!("capture from before settlement delivered")
        })
        .unwrap();
    assert!(state.frame_delivery.pending());
    cache.shared.state.lock().unwrap().frame = Some(Arc::new(frame(2, Instant::now())));
    cache
        .deliver_due(&mut state, activity(), |_, _| Ok(()))
        .unwrap();
    assert_eq!(state.source_frame.unwrap().frame_id, 2);
}

#[test]
fn old_action_after_a_leading_delivery_requires_another_fresh_observation() {
    let _owner = super::super::super::session_retention::start_session();
    let mut state = Reader::default();
    state.frame_delivery.begin_input();
    let cache = FrameCache::new(Some(frame(1, Instant::now())));
    cache
        .deliver_due(&mut state, activity(), |_, _| Ok(()))
        .unwrap();
    assert!(!state.frame_delivery.pending());
    assert!(!state.frame_delivery.action_settled(true));
    assert!(state.frame_delivery.pending());
    cache
        .deliver_due(&mut state, activity(), |_, _| {
            panic!("pre-settlement leading frame reused")
        })
        .unwrap();
    cache.shared.state.lock().unwrap().frame = Some(Arc::new(frame(2, Instant::now())));
    cache
        .deliver_due(&mut state, activity(), |_, trigger| {
            assert_eq!(trigger, "post_action_refresh");
            Ok(())
        })
        .unwrap();
    assert_eq!(state.source_frame.unwrap().frame_id, 2);
    assert!(!state.frame_delivery.confirm_input());
    assert!(state.frame_delivery.accepts_action_source());
    assert!(!state.frame_delivery.action_settled(false));
    assert!(
        state.frame_delivery.pending(),
        "a superseded job still requires fresh state after its receipt settles"
    );
}
