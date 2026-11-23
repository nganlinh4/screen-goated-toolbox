use super::*;
static ADMISSION_TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn failed_editor_post_rejects_shutdown_with_detached_work() {
    let _test = ADMISSION_TEST_LOCK.lock().unwrap();
    let (started, running) = mpsc::channel();
    let (stop, stopped) = mpsc::channel();
    let job = std::thread::spawn(move || {
        started.send(()).unwrap();
        stopped.recv().unwrap();
    });
    running.recv().unwrap();
    let result = prepare(0); // No WebView exists in the worker test process.
    stop.send(()).unwrap();
    job.join().unwrap();
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("without its editor")
    );
}

#[test]
fn prepare_closes_recording_admission_and_only_its_owner_can_reopen_it() {
    let _test = ADMISSION_TEST_LOCK.lock().unwrap();
    reopen_recording_admission();
    let before = recording_generation();
    let first = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    assert!(
        PREPARE_OWNER
            .compare_exchange(0, first, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    );
    assert!(close_recording_admission(first));
    let during = recording_generation();
    assert!(!during.is_multiple_of(2));
    reopen_recording_admission();
    assert_eq!(recording_generation(), during);
    assert!(start_recording_with_generation(&serde_json::Value::Null, before).is_err());
    cancel_owned(first);
    assert!(recording_generation().is_multiple_of(2));
    let second = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    assert!(
        PREPARE_OWNER
            .compare_exchange(0, second, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    );
    assert!(close_recording_admission(second));
    cancel_owned(first);
    assert!(!recording_generation().is_multiple_of(2));
    cancel_owned(second);
    assert!(recording_generation().is_multiple_of(2));
    assert!(start_recording_with_generation(&serde_json::Value::Null, during).is_err());
}

#[test]
fn prepare_fences_a_start_queued_while_it_waits_for_the_recording_lock() {
    let _test = ADMISSION_TEST_LOCK.lock().unwrap();
    reopen_recording_admission();
    let before = recording_generation();
    let owner = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    assert!(
        PREPARE_OWNER
            .compare_exchange(0, owner, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    );
    let recording = RECORDING.lock().unwrap();
    let (entered, waiting) = mpsc::channel();
    let closing = std::thread::spawn(move || {
        entered.send(()).unwrap();
        close_recording_admission(owner)
    });
    waiting.recv().unwrap();
    let queued = std::thread::spawn(move || {
        start_recording_with_generation(&serde_json::Value::Null, before)
    });
    drop(recording);
    assert!(closing.join().unwrap());
    assert!(queued.join().unwrap().is_err());
    cancel_owned(owner);
    assert!(recording_generation().is_multiple_of(2));
}

#[test]
fn cancelled_prepare_cannot_close_admission_after_cancel_or_cleanup() {
    let _test = ADMISSION_TEST_LOCK.lock().unwrap();
    reopen_recording_admission();
    let before = recording_generation();
    let first = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    assert!(
        PREPARE_OWNER
            .compare_exchange(0, first, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    );
    // Cancellation wins before the prepare thread enters its close helper.
    cancel_owned(first);
    let reopened = recording_generation();
    assert!(reopened.is_multiple_of(2));
    assert_ne!(reopened, before);
    assert!(!close_recording_admission(first));
    assert_eq!(recording_generation(), reopened);

    let second = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    assert!(
        PREPARE_OWNER
            .compare_exchange(0, second, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    );
    assert!(close_recording_admission(second));
    let newer_generation = recording_generation();
    assert!(!close_recording_admission(first));
    assert_eq!(recording_generation(), newer_generation);
    with_recording_cleanup(|| {});
    cancel_owned(second);
    assert!(!recording_generation().is_multiple_of(2));
    assert!(!close_recording_admission(first));
    assert!(!recording_generation().is_multiple_of(2));
    reopen_recording_admission();
    assert!(recording_generation().is_multiple_of(2));
}

#[test]
fn committed_close_cannot_be_cancelled_or_commit_a_stale_owner() {
    let _test = ADMISSION_TEST_LOCK.lock().unwrap();
    reopen_recording_admission();
    let first = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    assert!(
        PREPARE_OWNER
            .compare_exchange(0, first, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    );
    assert!(close_recording_admission(first));
    assert!(commit_prepared_owner(first));
    cancel_owned(first);
    assert_eq!(PREPARE_OWNER.load(Ordering::SeqCst), first);
    assert!(!recording_generation().is_multiple_of(2));
    assert!(!commit_prepared_owner(first));
    release_closed_window_owner(first);
    assert_eq!(PREPARE_OWNER.load(Ordering::SeqCst), 0);
    let next = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    assert!(
        PREPARE_OWNER
            .compare_exchange(0, next, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    );
    assert!(close_recording_admission(next));
    assert!(!commit_prepared_owner(first));
    assert!(commit_prepared_owner(next));
    release_closed_window_owner(first);
    assert_eq!(PREPARE_OWNER.load(Ordering::SeqCst), next);
    release_closed_window_owner(next);
}

#[test]
fn show_cannot_reopen_a_cleanup_waiting_for_the_recording_lock() {
    let _test = ADMISSION_TEST_LOCK.lock().unwrap();
    reopen_recording_admission();
    let before = recording_generation();
    // Cleanup has announced itself but has not yet acquired the receipt
    // lock. A queued show must not clear its admission barrier.
    let activity = RecordingCleanupActivity::begin();
    RECORDING_CLEANUP.store(true, Ordering::SeqCst);
    set_recording_admission(&RECORDING_GENERATION, false);
    let closing = recording_generation();
    reopen_recording_admission();
    assert_eq!(recording_generation(), closing);
    assert!(RECORDING_CLEANUP.load(Ordering::SeqCst));
    assert!(start_recording_with_generation(&serde_json::Value::Null, before).is_err());
    // Even if a show already passed its check, active cleanup must reject
    // a newly queued start before it can acquire a recording session.
    RECORDING_CLEANUP.store(false, Ordering::SeqCst);
    set_recording_admission(&RECORDING_GENERATION, true);
    assert!(
        start_recording_with_generation(&serde_json::Value::Null, recording_generation()).is_err()
    );
    RECORDING_CLEANUP.store(true, Ordering::SeqCst);
    set_recording_admission(&RECORDING_GENERATION, false);
    drop(activity);
    reopen_recording_admission();
    assert!(recording_generation().is_multiple_of(2));
    assert!(!RECORDING_CLEANUP.load(Ordering::SeqCst));
}

#[test]
fn show_during_cleanup_reopens_only_after_the_last_cleanup_and_prepare() {
    let _test = ADMISSION_TEST_LOCK.lock().unwrap();
    reopen_recording_admission();
    let activity = RecordingCleanupActivity::begin();
    RECORDING_CLEANUP.store(true, Ordering::SeqCst);
    set_recording_admission(&RECORDING_GENERATION, false);
    let during = recording_generation();
    reopen_recording_admission();
    assert_eq!(recording_generation(), during);
    assert!(start_recording_with_generation(&serde_json::Value::Null, during).is_err());
    drop(activity);
    assert!(recording_generation().is_multiple_of(2));
    assert!(!RECORDING_CLEANUP.load(Ordering::SeqCst));

    let owner = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    assert!(
        PREPARE_OWNER
            .compare_exchange(0, owner, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    );
    let activity = RecordingCleanupActivity::begin();
    RECORDING_CLEANUP.store(true, Ordering::SeqCst);
    set_recording_admission(&RECORDING_GENERATION, false);
    reopen_recording_admission();
    drop(activity);
    assert!(!recording_generation().is_multiple_of(2));
    cancel_owned(owner);
    assert!(recording_generation().is_multiple_of(2));

    let first = RecordingCleanupActivity::begin();
    RECORDING_CLEANUP.store(true, Ordering::SeqCst);
    set_recording_admission(&RECORDING_GENERATION, false);
    reopen_recording_admission();
    let later = RecordingCleanupActivity::begin();
    drop(first);
    drop(later);
    assert!(!recording_generation().is_multiple_of(2));
    reopen_recording_admission();
}

#[test]
fn cleanup_waits_for_start_to_install_its_session() {
    let _test = ADMISSION_TEST_LOCK.lock().unwrap();
    let starting = RECORDING.lock().unwrap();
    let (entered, waiting) = mpsc::channel();
    let (cleaned, completion) = mpsc::channel();
    let cleanup = std::thread::spawn(move || {
        entered.send(()).unwrap();
        with_recording_cleanup(|| cleaned.send(()).unwrap());
    });
    waiting.recv().unwrap();
    assert!(completion.try_recv().is_err());
    drop(starting);
    completion.recv().unwrap();
    cleanup.join().unwrap();
    reopen_recording_admission();
}

#[test]
fn cleanup_rejects_queued_starts_even_after_reopening() {
    let _test = ADMISSION_TEST_LOCK.lock().unwrap();
    reopen_recording_admission();
    let before = recording_generation();
    let (entered, waiting) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let cleanup = std::thread::spawn(move || {
        with_recording_cleanup(|| {
            entered.send(()).unwrap();
            released.recv().unwrap();
        })
    });
    waiting.recv().unwrap();
    let during = recording_generation();
    let queued = [before, during].map(|generation| {
        std::thread::spawn(move || {
            start_recording_with_generation(&serde_json::Value::Null, generation)
        })
    });
    release.send(()).unwrap();
    cleanup.join().unwrap();
    for job in queued {
        assert!(job.join().unwrap().is_err());
    }
    reopen_recording_admission();
    assert!(start_recording_with_generation(&serde_json::Value::Null, before).is_err());
    assert!(start_recording_with_generation(&serde_json::Value::Null, during).is_err());
}

#[test]
fn recovery_preserves_pending_result_and_exposes_active_recordings() {
    let mut receipt = RecordingReceipt::default();
    assert!(receipt.recovery(false, false).is_null());
    assert_eq!(
        receipt.recovery(true, true),
        serde_json::json!({"recordingActive": true, "recordingMode": "withCursor"})
    );
    receipt.pending = Some(serde_json::json!({"recordingId":"saved-result"}));
    assert_eq!(receipt.recovery(true, false)["recordingId"], "saved-result");
}

#[test]
fn prior_worker_recovery_has_no_live_capture_to_wait_for() {
    let mut receipt = RecordingReceipt {
        recovered_from_previous_worker: true,
        ..RecordingReceipt::default()
    };
    assert!(receipt.can_release_interrupted(false));
    receipt.recovered_from_previous_worker = false;
    assert!(!receipt.can_release_interrupted(false));
    assert!(receipt.can_release_interrupted(true));
}

#[test]
fn only_the_saved_recording_can_release_its_pending_receipt() {
    let mut receipt = RecordingReceipt {
        pending: Some(serde_json::json!({"recordingId": "a", "videoFilePath": "recording.mp4"})),
        last_saved: None,
        failure_id: 0,
        last_released: None,
        ..RecordingReceipt::default()
    };
    assert!(receipt.acknowledge("b").is_err());
    assert!(receipt.pending.is_some());
    assert!(receipt.acknowledge("a").is_ok());
    assert!(receipt.pending.is_none());
    assert!(receipt.acknowledge("a").is_ok());
    receipt.pending = Some(serde_json::json!({"recordingId": "c"}));
    assert!(receipt.acknowledge("a").is_ok());
    assert_eq!(receipt.pending.unwrap()["recordingId"], "c");
}

#[test]
fn stale_shutdown_acknowledgements_cannot_release_a_new_request() {
    let _test = ADMISSION_TEST_LOCK.lock().unwrap();
    let (sender, receiver) = mpsc::sync_channel(1);
    *PENDING.lock().unwrap() = Some((33, sender));
    assert!(acknowledge(&serde_json::json!({"requestId":32,"saved":true})).is_err());
    assert!(receiver.try_recv().is_err());
    assert!(acknowledge(&serde_json::json!({"requestId":33,"saved":false})).is_ok());
    assert!(!receiver.recv().unwrap());
    assert!(acknowledge(&serde_json::json!({"requestId":33,"saved":true})).is_err());
}
