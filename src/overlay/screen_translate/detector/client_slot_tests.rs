use super::*;
use std::sync::mpsc;

fn token() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}

#[test]
fn preparation_is_registered_before_launch_and_stop_rejects_late_publication() {
    let slot = ClientSlot::new();
    let cancelled = token();
    let mut preparation = slot.begin_prepare(Arc::clone(&cancelled)).unwrap();
    assert!(slot.begin_prepare(token()).is_none());
    slot.stop();
    assert!(cancelled.load(Ordering::Acquire));
    preparation.value = Some(17);
    preparation.retain();
    drop(preparation);
    assert!(slot.acquire(token()).unwrap().value.is_none());
}

#[test]
fn a_cancelled_waiter_exits_while_the_active_operation_remains_busy() {
    let slot = ClientSlot::<u32>::new();
    let active = slot.acquire(token()).unwrap();
    let cancelled = token();
    std::thread::scope(|scope| {
        let (started, ready) = mpsc::channel();
        let (done, result) = mpsc::channel();
        let waiting = Arc::clone(&cancelled);
        let slot_ref = &slot;
        scope.spawn(move || {
            started.send(()).unwrap();
            done.send(slot_ref.acquire(waiting).is_err()).unwrap();
        });
        ready.recv().unwrap();
        cancelled.store(true, Ordering::Release);
        assert!(result.recv_timeout(Duration::from_secs(1)).unwrap());
        drop(active);
    });
}

#[test]
fn healthy_drained_client_survives_request_cancellation_but_not_a_stop() {
    let slot = ClientSlot::new();
    let cancelled = token();
    let mut lease = slot.acquire(Arc::clone(&cancelled)).unwrap();
    lease.value = Some(21);
    cancelled.store(true, Ordering::Release);
    lease.retain();
    drop(lease);
    let next = slot.acquire(token()).unwrap();
    assert_eq!(next.value, Some(21));
    drop(next);
    assert!(slot.acquire(token()).unwrap().value.is_none());
}

#[test]
fn a_waiter_admitted_before_stop_cannot_start_in_the_new_generation() {
    let slot = ClientSlot::<u32>::new();
    let active = slot.acquire(token()).unwrap();
    std::thread::scope(|scope| {
        let (done, result) = mpsc::channel();
        let slot_ref = &slot;
        scope.spawn(move || done.send(slot_ref.acquire(token()).is_err()).unwrap());
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        loop {
            if Arc::strong_count(&slot.state.lock().unwrap().generation) >= 3 {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "waiter did not enter admission"
            );
            std::thread::yield_now();
        }
        slot.stop();
        assert!(result.recv_timeout(Duration::from_secs(1)).unwrap());
        drop(active);
    });
    assert!(slot.acquire(token()).is_ok());
}

#[test]
fn an_interrupted_exchange_does_not_reuse_a_previously_healthy_client() {
    let slot = ClientSlot::new();
    let mut prepared = slot.begin_prepare(token()).unwrap();
    prepared.value = Some(42);
    prepared.retain();
    drop(prepared);
    let _ = std::panic::catch_unwind(|| {
        let _lease = slot.acquire(token()).unwrap();
        panic!("fixture interrupted exchange");
    });
    assert!(slot.acquire(token()).unwrap().value.is_none());
}

struct BlockingDrop {
    started: mpsc::Sender<()>,
    finish: mpsc::Receiver<()>,
}

impl Drop for BlockingDrop {
    fn drop(&mut self) {
        self.started.send(()).unwrap();
        self.finish.recv_timeout(Duration::from_secs(3)).unwrap();
    }
}

#[test]
fn stop_does_not_wait_for_idle_client_teardown() {
    let slot = ClientSlot::new();
    let (started, dropping) = mpsc::channel();
    let (finish, finished) = mpsc::channel();
    let mut lease = slot.acquire(token()).unwrap();
    lease.value = Some(BlockingDrop {
        started,
        finish: finished,
    });
    lease.retain();
    drop(lease);
    slot.stop();
    dropping.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(slot.acquire(token()).unwrap().value.is_none());
    finish.send(()).unwrap();
}

#[test]
fn failed_client_teardown_keeps_single_flight_without_holding_the_mutex() {
    let slot = ClientSlot::new();
    let (started, dropping) = mpsc::channel();
    let (finish, finished) = mpsc::channel();
    std::thread::scope(|scope| {
        let mut lease = slot.acquire(token()).unwrap();
        lease.value = Some(BlockingDrop {
            started,
            finish: finished,
        });
        scope.spawn(move || drop(lease));
        dropping.recv_timeout(Duration::from_secs(1)).unwrap();
        slot.stop();
        assert!(slot.begin_prepare(token()).is_none());
        finish.send(()).unwrap();
    });
    assert!(slot.acquire(token()).unwrap().value.is_none());
}

#[test]
fn unwinding_startup_releases_the_active_reservation() {
    let slot = ClientSlot::<u32>::new();
    let _ = std::panic::catch_unwind(|| {
        let _lease = slot.begin_prepare(token()).unwrap();
        panic!("fixture startup failure");
    });
    assert!(slot.begin_prepare(token()).is_some());
}
