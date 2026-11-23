use super::*;

#[test]
fn direct_speech_generation_matches_shared_contract() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/parity-fixtures/live-translate/direct-speech-generation.json"
    )))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let mut epoch = 0;
        let mut visible = true;
        let mut produced = std::collections::HashMap::new();
        for event in case["events"].as_array().unwrap() {
            if event["hide"].as_bool() == Some(true) || event["stop"].as_bool() == Some(true) {
                visible = false;
                epoch += 1;
            }
            if event["show"].as_bool() == Some(true) || event["restart"].as_bool() == Some(true) {
                visible = true;
            }
            if let Some(name) = event["produce"].as_str() {
                let mut segment = SegmentPlayback::new();
                segment.playback_epoch = visible.then_some(epoch);
                produced.insert(name, segment);
            }
            if let Some(name) = event["deliver"].as_str() {
                let segment = produced.get_mut(name).unwrap();
                queue_audio(segment, vec![1], epoch, visible);
                assert_eq!(
                    !segment.chunks.is_empty(),
                    event["expected"].as_bool().unwrap(),
                    "{}",
                    case["name"]
                );
            }
        }
    }
}

#[test]
fn hidden_or_replaced_playback_discards_buffered_audio() {
    let mut segments = BTreeMap::new();
    let mut first = SegmentPlayback::new();
    first.chunks.push_back(vec![1]);
    let mut later = SegmentPlayback::new();
    later.chunks.push_back(vec![2]);
    segments.insert(0, first);
    segments.insert(1, later);
    assert!(!retire_stale_audio(&mut segments, 4, 4, true));
    assert_eq!(segments[&0].chunks.len(), 1);
    assert!(retire_stale_audio(&mut segments, 4, 5, true));
    assert!(segments.values().all(|segment| segment.chunks.is_empty()));
    segments.get_mut(&0).unwrap().chunks.push_back(vec![3]);
    assert!(retire_stale_audio(&mut segments, 5, 5, false));
    assert!(segments[&0].chunks.is_empty());
    segments.get_mut(&0).unwrap().chunks.push_back(vec![4]);
    assert!(!retire_stale_audio(&mut segments, 5, 5, true));
    assert_eq!(segments[&0].chunks.front().unwrap(), &[4]);
}

#[test]
fn stale_undelivered_segment_does_not_block_new_generation() {
    let mut stale = SegmentPlayback::new();
    stale.playback_epoch = Some(1);
    let mut current = SegmentPlayback::new();
    current.playback_epoch = Some(2);
    let mut segments = BTreeMap::from([(0, stale), (1, current)]);
    let mut next = 0;
    let mut playback = None;
    drain_ordered_audio(
        &mut segments,
        &mut next,
        &mut playback,
        HWND::default(),
        S2sMode::LegacyInterpreter,
        &AtomicBool::new(false),
        2,
        &mut Vec::new(),
        &super::super::backlog::S2sBacklogRun::default(),
        &mut Vec::new(),
    );
    assert_eq!(next, 1);
    assert!(segments.contains_key(&1));
}

#[test]
fn retired_run_cannot_publish_delayed_segments_into_restarted_capture() {
    let session_id =
        crate::overlay::realtime_webview::state::REALTIME_SESSION_ID.load(Ordering::SeqCst);
    let engine_revision = crate::overlay::realtime_webview::state::TRANSCRIPTION_ENGINE_REVISION
        .load(Ordering::SeqCst);
    for _change in ["source", "model", "language"] {
        let state = Arc::new(Mutex::new(crate::api::realtime_audio::RealtimeState::new()));
        let context = Arc::new(Mutex::new(S2sContextMemory::default()));
        let old_stop = Arc::new(AtomicBool::new(false));
        let (old_tx, old_rx) = mpsc::channel();
        let old_state = state.clone();
        let old_context = context.clone();
        let coordinator_stop = old_stop.clone();
        let coordinator = std::thread::spawn(move || {
            coordinate_output(
                old_rx,
                coordinator_stop,
                Arc::new(AtomicBool::new(false)),
                session_id,
                engine_revision,
                HWND::default(),
                None,
                old_state,
                old_context,
                S2sMode::LegacyInterpreter,
                Arc::new(super::super::backlog::S2sBacklogRun::default()),
            );
        });
        old_stop.store(true, Ordering::SeqCst);
        let _ = old_tx.send(S2sEvent::OutputText {
            id: 0,
            text: "old translation".into(),
        });
        coordinator.join().unwrap();
        assert!(state.lock().unwrap().display_translation.is_empty());

        let (new_tx, new_rx) = mpsc::channel();
        new_tx
            .send(S2sEvent::OutputText {
                id: 0,
                text: "new translation".into(),
            })
            .unwrap();
        drop(new_tx);
        coordinate_output(
            new_rx,
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            session_id,
            engine_revision,
            HWND::default(),
            None,
            state.clone(),
            context,
            S2sMode::LegacyInterpreter,
            Arc::new(super::super::backlog::S2sBacklogRun::default()),
        );
        assert_eq!(state.lock().unwrap().display_translation, "new translation");
    }
}

#[test]
fn session_stop_rejects_events_before_private_run_shutdown() {
    let session_id =
        crate::overlay::realtime_webview::state::REALTIME_SESSION_ID.load(Ordering::SeqCst);
    let engine_revision = crate::overlay::realtime_webview::state::TRANSCRIPTION_ENGINE_REVISION
        .load(Ordering::SeqCst);
    let state = Arc::new(Mutex::new(crate::api::realtime_audio::RealtimeState::new()));
    let (tx, rx) = mpsc::channel();
    tx.send(S2sEvent::OutputText {
        id: 0,
        text: "retired".into(),
    })
    .unwrap();
    drop(tx);
    coordinate_output(
        rx,
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(true)),
        session_id,
        engine_revision,
        HWND::default(),
        None,
        state.clone(),
        Arc::new(Mutex::new(S2sContextMemory::default())),
        S2sMode::LegacyInterpreter,
        Arc::new(super::super::backlog::S2sBacklogRun::default()),
    );
    assert!(state.lock().unwrap().display_translation.is_empty());
}

#[test]
fn retired_engine_rejects_direct_speech_text() {
    let session_id =
        crate::overlay::realtime_webview::state::REALTIME_SESSION_ID.load(Ordering::SeqCst);
    let engine_revision = crate::overlay::realtime_webview::state::TRANSCRIPTION_ENGINE_REVISION
        .load(Ordering::SeqCst)
        .wrapping_sub(1);
    let state = Arc::new(Mutex::new(crate::api::realtime_audio::RealtimeState::new()));
    let (tx, rx) = mpsc::channel();
    tx.send(S2sEvent::OutputText {
        id: 0,
        text: "retired".into(),
    })
    .unwrap();
    drop(tx);
    coordinate_output(
        rx,
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
        session_id,
        engine_revision,
        HWND::default(),
        None,
        state.clone(),
        Arc::new(Mutex::new(S2sContextMemory::default())),
        S2sMode::LegacyInterpreter,
        Arc::new(super::super::backlog::S2sBacklogRun::default()),
    );
    assert!(state.lock().unwrap().display_translation.is_empty());
}

#[test]
fn dropped_old_channel_retires_only_its_backlog_lease() {
    let old_run = Arc::new(super::super::backlog::S2sBacklogRun::default());
    let new_run = Arc::new(super::super::backlog::S2sBacklogRun::default());
    let old = old_run.lease(1_000);
    let new = new_run.lease(2_000);
    let old_weak = Arc::downgrade(&old);
    let new_weak = Arc::downgrade(&new);
    let (old_tx, old_rx) = mpsc::channel();
    assert!(
        old_tx
            .send(S2sEvent::Queued {
                id: 0,
                audio_ms: 1_000,
                queued_at: Instant::now(),
                playback_epoch: Some(0),
                backlog: old,
            })
            .is_ok()
    );
    drop(old_rx);
    assert!(old_weak.upgrade().is_none());
    assert!(new_weak.upgrade().is_some());
    drop(new);
    assert!(new_weak.upgrade().is_none());
}
