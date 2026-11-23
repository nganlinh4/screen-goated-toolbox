//! Bounded, ordered delivery of renderer events that can block the parent.
use std::io::{self, BufRead, Read};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, LazyLock};

use super::protocol::ChildEvent;

pub(super) enum Route {
    Ready,
    Heartbeat,
    Failure(super::protocol::RendererFailureKind),
    Ordered(ChildEvent),
}

pub(super) fn route(event: ChildEvent) -> Route {
    match event {
        ChildEvent::Ready => Route::Ready,
        ChildEvent::Heartbeat => Route::Heartbeat,
        ChildEvent::RendererFailure { kind } => Route::Failure(kind),
        other => Route::Ordered(other),
    }
}

pub(super) const MAX_EVENT_BYTES: usize = 1024 * 1024;
const MAX_QUEUED_BYTES: usize = 4 * 1024 * 1024;
const MAX_QUEUED_EVENTS: usize = 64;
const STALL_TIMEOUT_MS: u64 = 30_000;
static DISABLED: AtomicBool = AtomicBool::new(false);
static IN_FLIGHT_SINCE: AtomicU64 = AtomicU64::new(0);

pub(super) fn read_frame(reader: &mut impl BufRead) -> io::Result<Option<Vec<u8>>> {
    let mut line = Vec::new();
    let read = reader
        .take((MAX_EVENT_BYTES + 1) as u64)
        .read_until(b'\n', &mut line)?;
    if read > MAX_EVENT_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "renderer event exceeded size limit",
        ));
    }
    Ok((read != 0).then_some(line))
}

struct QueuedEvent {
    generation: u64,
    session_id: u64,
    event: ChildEvent,
    bytes: usize,
    reserved: Arc<AtomicUsize>,
}

impl Drop for QueuedEvent {
    fn drop(&mut self) {
        self.reserved.fetch_sub(self.bytes, Ordering::SeqCst);
    }
}

struct Queue {
    sender: SyncSender<QueuedEvent>,
    reserved: Arc<AtomicUsize>,
}

fn should_dispatch(
    live_generation: u64,
    current_session: u64,
    generation: u64,
    session_id: u64,
    event: &ChildEvent,
) -> bool {
    generation != 0
        && generation == live_generation
        && (matches!(event, ChildEvent::ResyncRequested)
            || (session_id != 0 && current_session == session_id))
}

impl Queue {
    fn push(&self, generation: u64, session_id: u64, event: ChildEvent, bytes: usize) -> bool {
        if bytes > MAX_EVENT_BYTES
            || self
                .reserved
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
                    current
                        .checked_add(bytes)
                        .filter(|sum| *sum <= MAX_QUEUED_BYTES)
                })
                .is_err()
        {
            return false;
        }
        let item = QueuedEvent {
            generation,
            session_id,
            event,
            bytes,
            reserved: self.reserved.clone(),
        };
        match self.sender.try_send(item) {
            Ok(()) => true,
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => false,
        }
    }
}

static QUEUE: LazyLock<Option<Queue>> = LazyLock::new(|| {
    let (sender, receiver) = sync_channel::<QueuedEvent>(MAX_QUEUED_EVENTS);
    let reserved = Arc::new(AtomicUsize::new(0));
    std::thread::Builder::new()
        .name("sgt-realtime-events".into())
        .spawn(move || {
            while let Ok(item) = receiver.recv() {
                if DISABLED.load(Ordering::SeqCst)
                    || !should_dispatch(
                        super::supervisor::current_generation(),
                        super::state::REALTIME_SESSION_ID.load(Ordering::SeqCst),
                        item.generation,
                        item.session_id,
                        &item.event,
                    )
                {
                    continue;
                }
                IN_FLIGHT_SINCE.store(super::supervisor::now_ms().max(1), Ordering::SeqCst);
                if matches!(item.event, ChildEvent::ResyncRequested) {
                    super::parent::queue_snapshot();
                } else {
                    super::parent::handle_child_event(
                        item.event.clone(),
                        item.session_id,
                        item.generation,
                    );
                }
                IN_FLIGHT_SINCE.store(0, Ordering::SeqCst);
            }
        })
        .ok()
        .map(|_| Queue { sender, reserved })
});

pub(super) fn is_disabled() -> bool {
    DISABLED.load(Ordering::SeqCst)
}

pub(super) fn enqueue(generation: u64, session_id: u64, event: ChildEvent, bytes: usize) -> bool {
    !is_disabled()
        && QUEUE
            .as_ref()
            .is_some_and(|queue| queue.push(generation, session_id, event, bytes))
}

pub(super) fn check_stall(now_ms: u64, generation: u64) {
    if claim_stall(&IN_FLIGHT_SINCE, now_ms) {
        fail(generation, "event handling stalled");
    }
}

fn claim_stall(in_flight: &AtomicU64, now_ms: u64) -> bool {
    let started = in_flight.load(Ordering::SeqCst);
    if started != 0
        && now_ms.saturating_sub(started) > STALL_TIMEOUT_MS
        && in_flight
            .compare_exchange(started, u64::MAX, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    {
        return true;
    }
    false
}

pub(super) fn reset_stall_after_pause(now_ms: u64) {
    reset_stall(&IN_FLIGHT_SINCE, now_ms);
}

fn reset_stall(in_flight: &AtomicU64, now_ms: u64) {
    let _ = in_flight.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |started| {
        (started != 0).then_some(now_ms.max(1))
    });
}

pub(super) fn fail(generation: u64, reason: &str) {
    if DISABLED.swap(true, Ordering::SeqCst) {
        return;
    }
    crate::log_info!("[RealtimeCompositor] event processing unavailable: {reason}");
    crate::overlay::auto_copy_badge::show_error_notification(
        "Realtime controls are unavailable. Restart the app to try again.",
    );
    super::supervisor::fail_generation(generation, reason, true);
}

#[cfg(test)]
#[path = "event_worker_tests.rs"]
mod tests;
