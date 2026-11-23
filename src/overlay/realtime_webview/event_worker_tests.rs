use super::*;
use std::sync::mpsc::sync_channel;

#[test]
fn full_queue_rejects_without_discarding_order_or_leaking_bytes() {
    let (sender, receiver) = sync_channel(2);
    let reserved = Arc::new(AtomicUsize::new(0));
    let queue = Queue {
        sender,
        reserved: reserved.clone(),
    };
    assert!(queue.push(1, 5, ChildEvent::Ready, 7));
    assert!(queue.push(1, 5, ChildEvent::Heartbeat, 11));
    assert!(!queue.push(1, 5, ChildEvent::ResyncRequested, 13));
    assert_eq!(reserved.load(Ordering::SeqCst), 18);
    let first = receiver.recv().unwrap();
    let second = receiver.recv().unwrap();
    assert!(matches!(first.event, ChildEvent::Ready));
    assert!(matches!(second.event, ChildEvent::Heartbeat));
    drop(first);
    drop(second);
    assert_eq!(reserved.load(Ordering::SeqCst), 0);
}

#[test]
fn queue_enforces_individual_and_total_byte_limits() {
    let (sender, receiver) = sync_channel(16);
    let reserved = Arc::new(AtomicUsize::new(0));
    let queue = Queue {
        sender,
        reserved: reserved.clone(),
    };
    assert!(!queue.push(1, 1, ChildEvent::Ready, MAX_EVENT_BYTES + 1));
    for _ in 0..(MAX_QUEUED_BYTES / MAX_EVENT_BYTES) {
        assert!(queue.push(1, 1, ChildEvent::Ready, MAX_EVENT_BYTES));
    }
    assert!(!queue.push(1, 1, ChildEvent::Ready, 1));
    drop(receiver);
    assert_eq!(reserved.load(Ordering::SeqCst), 0);
}

#[test]
fn old_generations_and_sessions_are_rejected_but_resync_uses_current_scene() {
    let normal = ChildEvent::Ready;
    assert!(should_dispatch(3, 7, 3, 7, &normal));
    assert!(!should_dispatch(3, 7, 2, 7, &normal));
    assert!(!should_dispatch(3, 7, 3, 6, &normal));
    assert!(!should_dispatch(3, 0, 3, 0, &normal));
    assert!(should_dispatch(3, 7, 3, 6, &ChildEvent::ResyncRequested));
    assert!(!should_dispatch(3, 7, 2, 6, &ChildEvent::ResyncRequested));
}

#[test]
fn reader_bounds_frames_without_consuming_an_unbounded_line() {
    let payload = vec![b'x'; MAX_EVENT_BYTES + 20];
    let mut reader = std::io::Cursor::new(payload);
    assert_eq!(
        read_frame(&mut reader).unwrap_err().kind(),
        std::io::ErrorKind::InvalidData
    );
    assert_eq!(reader.position(), (MAX_EVENT_BYTES + 1) as u64);
    let mut valid = std::io::Cursor::new(b"first\nsecond".to_vec());
    assert_eq!(read_frame(&mut valid).unwrap().unwrap(), b"first\n");
    assert_eq!(read_frame(&mut valid).unwrap().unwrap(), b"second");
    assert!(read_frame(&mut valid).unwrap().is_none());
}

#[test]
fn heartbeat_is_routed_outside_a_full_ordered_queue() {
    let (sender, _receiver) = sync_channel(1);
    let queue = Queue {
        sender,
        reserved: Arc::new(AtomicUsize::new(0)),
    };
    assert!(queue.push(1, 1, ChildEvent::ResyncRequested, 1));
    assert!(!queue.push(1, 1, ChildEvent::ResyncRequested, 1));
    assert!(matches!(route(ChildEvent::Heartbeat), Route::Heartbeat));
    assert!(matches!(route(ChildEvent::Ready), Route::Ready));
    assert!(matches!(
        route(ChildEvent::ResyncRequested),
        Route::Ordered(ChildEvent::ResyncRequested)
    ));
}

#[test]
fn blocked_handler_does_not_block_heartbeat_routing_or_reorder_events() {
    use std::sync::mpsc::channel;
    use std::time::Duration;

    let (sender, receiver) = sync_channel(2);
    let queue = Queue {
        sender,
        reserved: Arc::new(AtomicUsize::new(0)),
    };
    let (started_tx, started_rx) = channel();
    let (release_tx, release_rx) = channel();
    let worker = std::thread::spawn(move || {
        let first = receiver.recv().unwrap();
        started_tx.send(()).unwrap();
        release_rx.recv().unwrap();
        let second = receiver.recv().unwrap();
        (first.session_id, second.session_id)
    });
    assert!(queue.push(1, 11, ChildEvent::ResyncRequested, 1));
    started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(matches!(route(ChildEvent::Heartbeat), Route::Heartbeat));
    assert!(queue.push(1, 12, ChildEvent::ResyncRequested, 1));
    release_tx.send(()).unwrap();
    assert_eq!(worker.join().unwrap(), (11, 12));
}

#[test]
fn stall_timeout_is_bounded_and_system_pause_restarts_observation() {
    let in_flight = AtomicU64::new(10);
    assert!(!claim_stall(&in_flight, 10 + STALL_TIMEOUT_MS));
    reset_stall(&in_flight, 100_000);
    assert!(!claim_stall(&in_flight, 100_000 + STALL_TIMEOUT_MS));
    assert!(claim_stall(&in_flight, 100_001 + STALL_TIMEOUT_MS));
    assert!(!claim_stall(&in_flight, 100_002 + STALL_TIMEOUT_MS));
    in_flight.store(0, Ordering::SeqCst);
    reset_stall(&in_flight, 200_000);
    assert_eq!(in_flight.load(Ordering::SeqCst), 0);
}
