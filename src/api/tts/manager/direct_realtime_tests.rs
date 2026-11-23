use super::{RealtimeAudioSink, TtsManager};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

#[derive(Default)]
struct Sink {
    plays: AtomicUsize,
    stops: AtomicUsize,
}

#[test]
fn structural_realtime_retirement_returns_queued_notifications() {
    let manager = TtsManager::new();
    let epoch = manager.realtime_epoch();
    let request = manager.speak_realtime("queued", 0, epoch).unwrap();
    let removed = manager.retire_realtime();
    assert_eq!(removed, vec![(0, request)]);
    assert!(!manager.is_speaking(request));
    manager.notify_retired_playback(removed);
}

#[test]
fn delayed_old_retirement_does_not_cancel_replacement_audio() {
    let manager = Arc::new(TtsManager::new());
    let old_epoch = manager.realtime_epoch();
    let (release_tx, release_rx) = mpsc::channel();
    let (ready_tx, ready_rx) = mpsc::channel();
    let worker = {
        let manager = manager.clone();
        std::thread::spawn(move || {
            ready_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            manager.retire_realtime_if_epoch(old_epoch)
        })
    };
    ready_rx.recv().unwrap();
    manager.stop_realtime();
    let replacement_epoch = manager.realtime_epoch();
    let replacement = manager
        .speak_realtime("new session", 0, replacement_epoch)
        .unwrap();
    release_tx.send(()).unwrap();
    assert!(worker.join().unwrap().is_none());
    assert!(manager.is_speaking(replacement));
    assert_eq!(
        manager.retire_realtime_if_epoch(replacement_epoch),
        Some(vec![(0, replacement)])
    );
}

#[test]
fn session_transition_invalidates_epoch_captured_after_early_stop() {
    let manager = TtsManager::new();
    manager.stop_realtime();
    // An old selector can commit after the early stop but before the new
    // session is published; the transition must invalidate even this epoch.
    let stale_epoch = manager.realtime_epoch();
    manager.invalidate_realtime_epoch();
    let new_epoch = manager.realtime_epoch();
    let replacement = manager.speak_realtime("replacement", 0, new_epoch).unwrap();
    assert!(manager.retire_realtime_if_epoch(stale_epoch).is_none());
    assert!(manager.is_speaking(replacement));
}

#[test]
fn in_flight_retirement_does_not_invalidate_the_new_session_epoch() {
    struct StopGate {
        entered: mpsc::Sender<()>,
        release: Mutex<mpsc::Receiver<()>>,
    }
    impl RealtimeAudioSink for StopGate {
        fn play(&self, _: &[u8]) {}
        fn stop(&self) {
            self.entered.send(()).unwrap();
            self.release.lock().unwrap().recv().unwrap();
        }
    }
    let manager = Arc::new(TtsManager::new());
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let sink: Arc<dyn RealtimeAudioSink> = Arc::new(StopGate {
        entered: entered_tx,
        release: Mutex::new(release_rx),
    });
    let epoch = manager.realtime_epoch();
    assert!(manager.admit_direct_realtime_audio(
        &sink,
        &[1],
        &AtomicBool::new(false),
        &AtomicBool::new(true),
        epoch
    ));
    let worker = {
        let manager = manager.clone();
        std::thread::spawn(move || manager.retire_realtime_if_epoch(epoch))
    };
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    manager.invalidate_realtime_epoch();
    let new_epoch = manager.realtime_epoch();
    release_tx.send(()).unwrap();
    assert!(worker.join().unwrap().is_some());
    assert!(
        manager
            .speak_realtime("new session", 0, new_epoch)
            .is_some()
    );
}

impl RealtimeAudioSink for Sink {
    fn play(&self, _: &[u8]) {
        self.plays.fetch_add(1, Ordering::SeqCst);
    }
    fn stop(&self) {
        self.stops.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn direct_audio_rejects_retired_epochs_and_releases_only_its_owner() {
    let manager = TtsManager::new();
    let first = Arc::new(Sink::default());
    let first_sink: Arc<dyn RealtimeAudioSink> = first.clone();
    let second = Arc::new(Sink::default());
    let second_sink: Arc<dyn RealtimeAudioSink> = second.clone();
    let stopped = AtomicBool::new(false);
    let visible = AtomicBool::new(true);
    let old_epoch = manager.realtime_epoch();
    let unrelated = manager.speak("other owner", 0);
    assert!(manager.admit_direct_realtime_audio(&first_sink, &[1], &stopped, &visible, old_epoch));
    visible.store(false, Ordering::SeqCst);
    manager.stop_realtime();
    assert_eq!(first.stops.load(Ordering::SeqCst), 1);
    assert!(manager.is_speaking(unrelated));
    visible.store(true, Ordering::SeqCst);
    assert!(!manager.admit_direct_realtime_audio(&first_sink, &[2], &stopped, &visible, old_epoch));
    let epoch = manager.realtime_epoch();
    assert!(manager.admit_direct_realtime_audio(&first_sink, &[3], &stopped, &visible, epoch));
    assert!(manager.admit_direct_realtime_audio(&second_sink, &[4], &stopped, &visible, epoch));
    assert_eq!(first.stops.load(Ordering::SeqCst), 2);
    manager.release_direct_realtime(&first_sink);
    assert_eq!(second.stops.load(Ordering::SeqCst), 0);
    manager.release_direct_realtime(&second_sink);
    assert_eq!(second.stops.load(Ordering::SeqCst), 1);
    stopped.store(true, Ordering::SeqCst);
    assert!(!manager.admit_direct_realtime_audio(&second_sink, &[5], &stopped, &visible, epoch));
}

struct BlockingSink {
    entered: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
    stopped: AtomicBool,
}

impl RealtimeAudioSink for BlockingSink {
    fn play(&self, _: &[u8]) {
        self.entered.send(()).unwrap();
        self.release.lock().unwrap().recv().unwrap();
    }
    fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
    }
}

#[test]
fn hide_waits_for_admitted_effect_then_stops_and_rejects_stale_audio() {
    let manager = Arc::new(TtsManager::new());
    let visible = Arc::new(AtomicBool::new(true));
    let stopped = Arc::new(AtomicBool::new(false));
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let sink = Arc::new(BlockingSink {
        entered: entered_tx,
        release: Mutex::new(release_rx),
        stopped: AtomicBool::new(false),
    });
    let owner: Arc<dyn RealtimeAudioSink> = sink.clone();
    let epoch = manager.realtime_epoch();
    let worker_manager = manager.clone();
    let worker_visible = visible.clone();
    let worker_stopped = stopped.clone();
    let worker_sink = owner.clone();
    let worker = std::thread::spawn(move || {
        worker_manager.admit_direct_realtime_audio(
            &worker_sink,
            &[1],
            &worker_stopped,
            &worker_visible,
            epoch,
        )
    });
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    visible.store(false, Ordering::SeqCst);
    let stop_manager = manager.clone();
    let (done_tx, done_rx) = mpsc::channel();
    let stopper = std::thread::spawn(move || {
        stop_manager.stop_realtime();
        done_tx.send(()).unwrap();
    });
    assert!(done_rx.recv_timeout(Duration::from_millis(50)).is_err());
    release_tx.send(()).unwrap();
    assert!(worker.join().unwrap());
    done_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    stopper.join().unwrap();
    assert!(sink.stopped.load(Ordering::SeqCst));
    visible.store(true, Ordering::SeqCst);
    assert!(!manager.admit_direct_realtime_audio(&owner, &[2], &stopped, &visible, epoch));
}
