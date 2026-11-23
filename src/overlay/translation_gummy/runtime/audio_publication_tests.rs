use super::*;
use std::cell::Cell;
use std::sync::mpsc::channel;
use std::time::Duration;

#[test]
fn gummy_audio_publication_never_requests_full_state_for_meter_samples() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../parity-fixtures/translation-gummy/meter-delivery.json"
    ))
    .unwrap();
    let owner = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let control = Mutex::new(Some(SessionControl {
        stop: owner.clone(),
        playback_ids: Arc::new(Mutex::new(Vec::new())),
    }));
    let updates = Cell::new(0);
    let full = Cell::new(0);
    let meter = Cell::new(0);
    let count = fixture["delivery"]["burstSamples"].as_u64().unwrap();
    for _ in 0..count {
        assert!(publish_audio_owned_with(
            &control,
            &owner,
            || updates.set(updates.get() + 1),
            |kind| match kind {
                UiUpdate::FullState => full.set(full.get() + 1),
                UiUpdate::AudioLevel => meter.set(meter.get() + 1),
            }
        ));
    }
    assert_eq!(updates.get(), count);
    assert_eq!(
        full.get(),
        fixture["delivery"]["fullStateUpdates"].as_u64().unwrap()
    );
    assert_eq!(meter.get(), count);
}

#[test]
fn gummy_audio_retired_owners_cannot_mutate_or_schedule_meter_updates() {
    let old = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let current = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let control = Mutex::new(Some(SessionControl {
        stop: current.clone(),
        playback_ids: Arc::new(Mutex::new(Vec::new())),
    }));
    let reject = |owner: &Arc<std::sync::atomic::AtomicBool>| {
        assert!(!publish_audio_owned_with(
            &control,
            owner,
            || panic!("retired owner changed meter"),
            |_| panic!("retired owner queued an update")
        ));
    };
    reject(&old);
    current.store(true, Ordering::SeqCst);
    reject(&current);
    *control.lock().unwrap() = None;
    reject(&old);
}

#[test]
fn gummy_audio_retirement_cannot_overtake_an_admitted_notification() {
    let owner = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let control = Arc::new(Mutex::new(Some(SessionControl {
        stop: owner.clone(),
        playback_ids: Arc::new(Mutex::new(Vec::new())),
    })));
    let (started, wait_started) = channel();
    let (release, wait_release) = channel();
    let worker_control = control.clone();
    let worker_owner = owner.clone();
    let worker = std::thread::spawn(move || {
        assert!(publish_audio_owned_with(
            &worker_control,
            &worker_owner,
            || {},
            |kind| {
                assert_eq!(kind, UiUpdate::AudioLevel);
                started.send(()).unwrap();
                wait_release.recv_timeout(Duration::from_secs(5)).unwrap();
            }
        ));
    });
    wait_started.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(matches!(
        control.try_lock(),
        Err(std::sync::TryLockError::WouldBlock)
    ));
    release.send(()).unwrap();
    worker.join().unwrap();
    *control.lock().unwrap() = None;
    assert!(!publish_audio_owned_with(
        &control,
        &owner,
        || panic!("retired mutation"),
        |_| panic!("retired notification")
    ));
}
