use super::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Barrier, Mutex};

struct FakeThread(Arc<AtomicBool>);
impl ThreadLease for FakeThread {
    fn state(&self) -> ThreadState {
        if self.0.load(Ordering::SeqCst) {
            ThreadState::Alive
        } else {
            ThreadState::Exited
        }
    }
}

fn ready(hook: usize, alive: &Arc<AtomicBool>) -> (Prepared<FakeThread>, mpsc::Receiver<()>) {
    let (activate, receiver) = mpsc::channel();
    (
        Prepared {
            thread: FakeThread(alive.clone()),
            thread_id: hook as u32,
            hook,
            activate,
        },
        receiver,
    )
}

#[test]
fn late_start_is_retained_without_spawning_a_second_worker() {
    let mut lifecycle = Lifecycle::new();
    let (tx, rx) = mpsc::channel();
    let starts = AtomicUsize::new(0);
    assert!(
        lifecycle
            .acquire(
                Duration::ZERO,
                || {
                    starts.fetch_add(1, Ordering::SeqCst);
                    Ok(rx)
                },
                || {},
                |_| {}
            )
            .is_err()
    );
    let alive = Arc::new(AtomicBool::new(true));
    let (prepared, activated) = ready(11, &alive);
    tx.send(Some(prepared)).unwrap();
    let owner = lifecycle
        .acquire(
            Duration::ZERO,
            || panic!("duplicate startup"),
            || {},
            |_| {},
        )
        .unwrap();
    activated.try_recv().unwrap();
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    assert!(lifecycle.owns(&owner));
}

#[test]
fn concurrent_callers_share_one_pending_start() {
    let lifecycle = Arc::new(Mutex::new(Lifecycle::<FakeThread>::new()));
    let (tx, rx) = mpsc::channel();
    let receiver = Arc::new(Mutex::new(Some(rx)));
    let starts = Arc::new(AtomicUsize::new(0));
    let gate = Arc::new(Barrier::new(3));
    let mut workers = Vec::new();
    for _ in 0..2 {
        let lifecycle = lifecycle.clone();
        let receiver = receiver.clone();
        let starts = starts.clone();
        let gate = gate.clone();
        workers.push(std::thread::spawn(move || {
            gate.wait();
            lifecycle
                .lock()
                .unwrap()
                .acquire(
                    Duration::ZERO,
                    || {
                        starts.fetch_add(1, Ordering::SeqCst);
                        Ok(receiver.lock().unwrap().take().unwrap())
                    },
                    || {},
                    |_| {},
                )
                .is_err()
        }));
    }
    gate.wait();
    for worker in workers {
        assert!(worker.join().unwrap());
    }
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    let alive = Arc::new(AtomicBool::new(true));
    let (prepared, activated) = ready(11, &alive);
    tx.send(Some(prepared)).unwrap();
    let owner = lifecycle
        .lock()
        .unwrap()
        .acquire(
            Duration::ZERO,
            || panic!("duplicate startup"),
            || {},
            |_| {},
        )
        .unwrap();
    activated.try_recv().unwrap();
    assert!(lifecycle.lock().unwrap().owns(&owner));
}

#[test]
fn failed_start_retries_and_dead_owner_is_retired_before_replacement() {
    let mut lifecycle = Lifecycle::new();
    let (tx, rx) = mpsc::channel();
    tx.send(None).unwrap();
    assert!(
        lifecycle
            .acquire(Duration::ZERO, || Ok(rx), || {}, |_| {})
            .is_err()
    );
    lifecycle.retry_at = Instant::now();
    let alive = Arc::new(AtomicBool::new(true));
    let (prepared, _activation) = ready(11, &alive);
    let (tx, rx) = mpsc::channel();
    tx.send(Some(prepared)).unwrap();
    let first = lifecycle
        .acquire(Duration::ZERO, || Ok(rx), || {}, |_| {})
        .unwrap();
    alive.store(false, Ordering::SeqCst);
    let next_alive = Arc::new(AtomicBool::new(true));
    let (prepared, _activation) = ready(12, &next_alive);
    let (tx, rx) = mpsc::channel();
    tx.send(Some(prepared)).unwrap();
    let retired = AtomicUsize::new(0);
    let second = lifecycle
        .acquire(
            Duration::ZERO,
            || Ok(rx),
            || {
                retired.fetch_add(1, Ordering::SeqCst);
            },
            |_| {},
        )
        .unwrap();
    assert_eq!(retired.load(Ordering::SeqCst), 1);
    assert!(!lifecycle.owns(&first));
    assert!(lifecycle.owns(&second));
    assert!(second.epoch > first.epoch);
}

#[test]
fn failed_activation_keeps_reservation_until_worker_exits() {
    let mut lifecycle = Lifecycle::new();
    let alive = Arc::new(AtomicBool::new(true));
    let (prepared, activation) = ready(11, &alive);
    drop(activation);
    let (tx, rx) = mpsc::channel();
    tx.send(Some(prepared)).unwrap();
    assert!(
        lifecycle
            .acquire(Duration::ZERO, || Ok(rx), || {}, |_| {})
            .is_err()
    );
    assert!(
        lifecycle
            .acquire(
                Duration::ZERO,
                || panic!("worker still alive"),
                || {},
                |_| {}
            )
            .is_err()
    );
    alive.store(false, Ordering::SeqCst);
    let (tx, rx) = mpsc::channel();
    tx.send(None).unwrap();
    assert!(
        lifecycle
            .acquire(Duration::ZERO, || Ok(rx), || {}, |_| {})
            .is_err()
    );
}

#[test]
fn exhausted_tracker_epochs_never_reuse_the_last_identity() {
    let mut lifecycle = Lifecycle::new();
    lifecycle.next_epoch = u64::MAX;
    let alive = Arc::new(AtomicBool::new(true));
    let (prepared, activation) = ready(11, &alive);
    let (tx, rx) = mpsc::channel();
    tx.send(Some(prepared)).unwrap();
    assert!(
        lifecycle
            .acquire(
                Duration::ZERO,
                || Ok(rx),
                || {},
                |_| panic!("exhausted epoch")
            )
            .is_err()
    );
    assert!(activation.try_recv().is_err());
    alive.store(false, Ordering::SeqCst);
    assert!(
        lifecycle
            .acquire(Duration::ZERO, || panic!("exhausted epoch"), || {}, |_| {})
            .is_err()
    );
}

#[test]
fn unknown_thread_liveness_cannot_admit_a_replacement() {
    struct UnknownThread;
    impl ThreadLease for UnknownThread {
        fn state(&self) -> ThreadState {
            ThreadState::Unknown
        }
    }
    let mut lifecycle = Lifecycle::new();
    let (activate, receiver) = mpsc::channel();
    let (tx, rx) = mpsc::channel();
    tx.send(Some(Prepared {
        thread: UnknownThread,
        thread_id: 9,
        hook: 9,
        activate,
    }))
    .unwrap();
    assert!(
        lifecycle
            .acquire(
                Duration::ZERO,
                || Ok(rx),
                || {},
                |_| panic!("unknown thread")
            )
            .is_err()
    );
    assert!(receiver.try_recv().is_err());
    assert!(
        lifecycle
            .acquire(
                Duration::ZERO,
                || panic!("unproven retirement"),
                || {},
                |_| {}
            )
            .is_err()
    );
}
