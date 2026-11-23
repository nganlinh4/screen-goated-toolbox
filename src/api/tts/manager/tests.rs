use super::TtsManager;
use crate::api::tts::types::AudioEvent;
use crate::api::tts::types::TtsRequestProfile;
use crate::config::TtsMethod;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

fn artifact_profile() -> TtsRequestProfile {
    TtsRequestProfile {
        method: TtsMethod::EdgeTTS,
        gemini_model: String::new(),
        gemini_voice: String::new(),
        gemini_speed: String::new(),
        gemini_instruction: String::new(),
        gemini_language_conditions: Vec::new(),
        #[cfg(feature = "recorder-worker")]
        gemini_parallel_requests: 2,
        google_speed: String::new(),
        #[cfg(feature = "recorder-worker")]
        edge_voice: String::new(),
        edge_settings: Default::default(),
        language_code_override: Some("eng".to_string()),
    }
}

#[test]
fn cancelled_artifact_removes_only_its_own_queued_work() {
    let manager = TtsManager::new();
    let unrelated_id = manager.speak("keep this speech", 0);
    let generation = manager.interrupt_generation.load(Ordering::SeqCst);

    let result = manager.synthesize_to_wav_with_profile_cancel(
        "cancel this artifact",
        artifact_profile(),
        Arc::new(AtomicBool::new(true)),
    );

    assert!(result.is_err());
    assert_eq!(
        manager.interrupt_generation.load(Ordering::SeqCst),
        generation
    );
    let queue = manager.work_queue.lock().unwrap();
    assert_eq!(queue.len(), 1);
    assert_eq!(queue.front().unwrap().0.req._id, unrelated_id);
    assert_eq!(manager.playback_queue.lock().unwrap().len(), 1);
}

#[test]
fn stopping_one_queued_request_preserves_other_speech() {
    let manager = TtsManager::new();
    let first = manager.speak("first", 0);
    let second = manager.speak("second", 0);
    manager.stop_if_active(first);
    assert!(!manager.is_speaking(first));
    assert!(manager.is_speaking(second));
    let queue = manager.work_queue.lock().unwrap();
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].0.req._id, second);
}

#[test]
fn cancelling_a_waiting_artifact_preserves_later_queued_speech() {
    struct CancelOnDrop(Arc<AtomicBool>);
    impl Drop for CancelOnDrop {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }
    let manager = TtsManager::new();
    let cancelled = Arc::new(AtomicBool::new(false));
    let generation = manager.interrupt_generation.load(Ordering::SeqCst);
    std::thread::scope(|scope| {
        let _cancel_on_exit = CancelOnDrop(cancelled.clone());
        let request_cancel = cancelled.clone();
        let owner = &manager;
        let waiting = scope.spawn(move || {
            owner.synthesize_to_wav_with_profile_cancel(
                "artifact",
                artifact_profile(),
                request_cancel,
            )
        });
        let deadline = Instant::now() + Duration::from_secs(3);
        while manager.work_queue.lock().unwrap().is_empty() {
            assert!(Instant::now() < deadline, "artifact was not admitted");
            std::thread::yield_now();
        }
        let replacement = manager.speak("later speech", 0);
        cancelled.store(true, Ordering::SeqCst);
        assert!(waiting.join().unwrap().is_err());
        assert_eq!(
            manager.interrupt_generation.load(Ordering::SeqCst),
            generation
        );
        assert!(manager.is_speaking(replacement));
        let queue = manager.work_queue.lock().unwrap();
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].0.req._id, replacement);
        assert!(!queue[0].0.cancel.load(Ordering::SeqCst));
    });
}

#[test]
fn active_cancellation_does_not_cancel_next_request() {
    let manager = TtsManager::new();
    let first = manager.speak("first", 0);
    let second = manager.speak("second", 0);
    let item = manager.playback_queue.lock().unwrap().pop_front().unwrap();
    *manager.active_playback.lock().unwrap() = Some((item.2, item.4, item.5.clone()));
    manager.stop_if_active(first);
    assert!(item.5.load(Ordering::SeqCst));
    assert!(!manager.is_speaking(first));
    assert!(manager.is_speaking(second));
}

#[test]
fn chain_cancellation_reaches_queued_speech_and_preserves_sibling() {
    let manager = TtsManager::new();
    let root = crate::overlay::result::ChainCancelToken::new();
    let left = crate::overlay::result::ChainCancelToken::child(&root);
    let right = crate::overlay::result::ChainCancelToken::child(&root);
    let first = manager.speak_cancellable("first", 0, left.clone()).unwrap();
    let second = manager.speak_cancellable("second", 0, right).unwrap();
    left.cancel();
    assert!(!manager.is_speaking(first));
    assert!(manager.is_speaking(second));
    let queue = manager.work_queue.lock().unwrap();
    assert!(
        queue
            .iter()
            .find(|(request, _)| request.req._id == first)
            .unwrap()
            .0
            .cancel
            .load(Ordering::SeqCst)
    );
    assert!(
        !queue
            .iter()
            .find(|(request, _)| request.req._id == second)
            .unwrap()
            .0
            .cancel
            .load(Ordering::SeqCst)
    );
}

#[test]
fn playback_interrupt_does_not_cancel_artifact_generation() {
    let manager = Arc::new(TtsManager::new());
    let artifact_manager = manager.clone();
    let worker = std::thread::spawn(move || {
        artifact_manager.synthesize_to_wav_with_profile_cancel(
            "artifact",
            artifact_profile(),
            Arc::new(AtomicBool::new(false)),
        )
    });
    let deadline = Instant::now() + Duration::from_secs(3);
    while manager.work_queue.lock().unwrap().is_empty() {
        assert!(Instant::now() < deadline, "artifact was not queued");
        std::thread::yield_now();
    }
    manager.stop();
    let (queued, tx) = manager.work_queue.lock().unwrap().pop_front().unwrap();
    assert!(queued.artifact);
    assert!(!queued.is_cancelled(&manager));
    tx.send(AudioEvent::Data(vec![0, 0])).unwrap();
    tx.send(AudioEvent::End).unwrap();
    assert!(worker.join().unwrap().is_ok());
}

#[test]
fn live_bridges_and_speech_use_one_identity_space() {
    let manager = TtsManager::new();
    let speech = manager.speak("speech", 0);
    let (_, rx) = std::sync::mpsc::channel();
    let live = manager.enqueue_audio(rx, 0, true);
    assert_ne!(speech, live);
    manager.stop_if_active(live);
    assert!(manager.is_speaking(speech));
}

#[test]
fn realtime_stop_preserves_other_owners_and_rejects_stale_enqueues() {
    let manager = TtsManager::new();
    let speech = manager.speak("other window", 0);
    let epoch = manager.realtime_epoch();
    let realtime = manager.speak_realtime("live text", 0, epoch).unwrap();
    let (_, rx) = std::sync::mpsc::channel();
    let bridge = manager.enqueue_audio(rx, 0, true);
    manager.stop_realtime();
    assert!(manager.is_speaking(speech));
    assert!(!manager.is_speaking(realtime));
    assert!(!manager.is_speaking(bridge));
    assert!(manager.speak_realtime("stale", 0, epoch).is_none());
    let stop = AtomicBool::new(false);
    let visible = AtomicBool::new(true);
    let (_, live_rx) = std::sync::mpsc::channel();
    let next_bridge = manager
        .enqueue_realtime_audio(live_rx, 0, &stop, &visible, manager.realtime_epoch())
        .unwrap();
    assert!(manager.is_speaking(next_bridge));
    stop.store(true, Ordering::SeqCst);
    let (_, stale_rx) = std::sync::mpsc::channel();
    assert!(
        manager
            .enqueue_realtime_audio(stale_rx, 0, &stop, &visible, manager.realtime_epoch())
            .is_none()
    );
    let queue = manager.work_queue.lock().unwrap();
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].0.req._id, speech);
}

#[test]
fn hidden_bridge_admission_is_fenced_and_fresh_audio_can_resume() {
    let manager = Arc::new(TtsManager::new());
    let visible = Arc::new(AtomicBool::new(true));
    let stopped = Arc::new(AtomicBool::new(false));
    let other = manager.speak("other owner", 0);
    let epoch = manager.realtime_epoch();
    let (_, rx) = std::sync::mpsc::channel();
    let old = manager
        .enqueue_realtime_audio(rx, 0, &stopped, &visible, epoch)
        .unwrap();

    let operation = manager.playback_operations.lock().unwrap();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let worker_manager = manager.clone();
    let worker_visible = visible.clone();
    let worker_stopped = stopped.clone();
    let worker = std::thread::spawn(move || {
        let (_, rx) = std::sync::mpsc::channel();
        ready_tx.send(()).unwrap();
        worker_manager.enqueue_realtime_audio(rx, 0, &worker_stopped, &worker_visible, epoch)
    });
    ready_rx.recv().unwrap();
    visible.store(false, Ordering::SeqCst);
    drop(operation);
    assert!(worker.join().unwrap().is_none());
    manager.stop_realtime();
    assert!(!manager.is_speaking(old));
    assert!(manager.is_speaking(other));

    visible.store(true, Ordering::SeqCst);
    let (_, stale_rx) = std::sync::mpsc::channel();
    assert!(
        manager
            .enqueue_realtime_audio(stale_rx, 0, &stopped, &visible, epoch)
            .is_none()
    );
    let (_, fresh_rx) = std::sync::mpsc::channel();
    let fresh = manager
        .enqueue_realtime_audio(fresh_rx, 0, &stopped, &visible, manager.realtime_epoch())
        .unwrap();
    assert!(manager.is_speaking(fresh));
    assert!(manager.is_speaking(other));
}

#[test]
fn realtime_text_backlog_tracks_owners_across_completion_and_cancellation() {
    let manager = TtsManager::new();
    let epoch = manager.realtime_epoch();
    let first = manager.speak_realtime("first", 0, epoch).unwrap();
    let unrelated = manager.speak("other owner", 0);
    let (_, bridge_rx) = std::sync::mpsc::channel();
    let bridge = manager.enqueue_audio(bridge_rx, 0, true);
    let second = manager.speak_realtime("second", 0, epoch).unwrap();
    assert_eq!(manager.realtime_text_backlog(), 2);

    manager.finish_realtime_text(first);
    assert_eq!(manager.realtime_text_backlog(), 1);
    manager.finish_realtime_text(first);
    manager.stop_if_active(unrelated);
    manager.stop_if_active(bridge);
    assert_eq!(manager.realtime_text_backlog(), 1);

    let active = {
        let mut queue = manager.playback_queue.lock().unwrap();
        let position = queue.iter().position(|item| item.2 == second).unwrap();
        queue.remove(position).unwrap()
    };
    *manager.active_playback.lock().unwrap() = Some((active.2, active.4, active.5.clone()));
    manager.stop_realtime();
    assert!(active.5.load(Ordering::SeqCst));
    assert_eq!(manager.realtime_text_backlog(), 0);
    assert!(manager.speak_realtime("stale", 0, epoch).is_none());
    let next = manager
        .speak_realtime("next", 0, manager.realtime_epoch())
        .unwrap();
    manager.finish_realtime_text(second);
    assert_eq!(manager.realtime_text_backlog(), 1);
    manager.stop_if_active(next);
    assert_eq!(manager.realtime_text_backlog(), 0);
}

#[test]
fn stop_serializes_with_playback_admission() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    };
    use std::time::Duration;
    let manager = Arc::new(super::TtsManager::new());
    let cancel = Arc::new(AtomicBool::new(false));
    *manager.active_playback.lock().unwrap() = Some((42, false, cancel.clone()));
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let effects = Arc::new(AtomicUsize::new(0));
    let worker_manager = manager.clone();
    let worker_cancel = cancel.clone();
    let worker_effects = effects.clone();
    let worker = std::thread::spawn(move || {
        worker_manager.admit_playback(
            || worker_cancel.load(Ordering::SeqCst),
            || {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                worker_effects.fetch_add(1, Ordering::SeqCst);
            },
        )
    });
    entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let stopper_manager = manager.clone();
    let (stopped_tx, stopped_rx) = mpsc::channel();
    let stopper = std::thread::spawn(move || {
        stopper_manager.stop_if_active(42);
        stopped_tx.send(()).unwrap();
    });
    assert!(stopped_rx.recv_timeout(Duration::from_millis(50)).is_err());
    release_tx.send(()).unwrap();
    assert!(worker.join().unwrap());
    stopper.join().unwrap();
    assert!(cancel.load(Ordering::SeqCst));
    assert!(!manager.admit_playback(
        || cancel.load(Ordering::SeqCst),
        || {
            effects.fetch_add(1, Ordering::SeqCst);
        }
    ));
    assert_eq!(effects.load(Ordering::SeqCst), 1);
}
