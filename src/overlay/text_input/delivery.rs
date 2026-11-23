//! Bounded ownership and acknowledgments for renderer operations.
use std::collections::HashMap;
use std::sync::{
    Arc, LazyLock, Mutex, Weak,
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
};
use std::time::{Duration, Instant};

use crate::overlay::result::ChainCancelToken;

const MAX_IN_FLIGHT: usize = 128;
const ACK_TIMEOUT: Duration = Duration::from_secs(15);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_OBSERVER_ID: AtomicU64 = AtomicU64::new(1);
static WATCHDOG_RUNNING: AtomicBool = AtomicBool::new(false);
static OPERATIONS: LazyLock<Mutex<HashMap<u64, Arc<Operation>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ApplyResult {
    Applied,
    Rejected,
    Skipped,
    Uncertain,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Settlement {
    Applied,
    Rejected,
    Skipped,
    Cancelled,
    AppliedBeforeCancellationSettled,
    Uncertain,
}

type Observer = Arc<dyn Fn(Settlement) + Send + Sync>;
type CancelCallback = Arc<dyn Fn() + Send + Sync>;

#[derive(Default)]
struct State {
    admitted: bool,
    deadline: Option<Instant>,
    cancelled: bool,
    apply: Option<ApplyResult>,
    revoke: Option<bool>,
    reported: Option<Settlement>,
    closed: bool,
    quiescence_published: bool,
    observers: Vec<Observer>,
    quiescence_observers: Vec<(u64, CancelCallback)>,
}

pub(super) struct Operation {
    id: u64,
    session: u64,
    nonce: String,
    sequence: AtomicU64,
    owner: Option<Arc<ChainCancelToken>>,
    revoke: Arc<dyn Fn(u64) + Send + Sync>,
    state: Mutex<State>,
    cancel_callback: Mutex<Option<CancelCallback>>,
}

impl Operation {
    /// Reserve capacity before admitting work to the renderer.
    pub(super) fn new(
        session: u64,
        nonce: String,
        owner: Option<&Arc<ChainCancelToken>>,
        revoke: Arc<dyn Fn(u64) + Send + Sync>,
    ) -> Option<Arc<Self>> {
        let id = NEXT_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .ok()?;
        let operation = Arc::new(Self {
            id,
            session,
            nonce,
            sequence: AtomicU64::new(0),
            owner: owner.cloned(),
            revoke,
            state: Mutex::new(State {
                deadline: Some(Instant::now() + ACK_TIMEOUT),
                ..State::default()
            }),
            cancel_callback: Mutex::new(None),
        });
        {
            let mut operations = OPERATIONS.lock().unwrap_or_else(|e| e.into_inner());
            if operations.len() >= MAX_IN_FLIGHT {
                return None;
            }
            operations.insert(id, Arc::clone(&operation));
            if !WATCHDOG_RUNNING.swap(true, Ordering::AcqRel)
                && std::thread::Builder::new()
                    .name("sgt-text-delivery-watchdog".into())
                    .spawn(watchdog)
                    .is_err()
            {
                WATCHDOG_RUNNING.store(false, Ordering::Release);
                operations.remove(&id);
                return None;
            }
        }
        if let Some(owner) = owner {
            let weak = Arc::downgrade(&operation);
            let callback: CancelCallback = Arc::new(move || {
                if let Some(operation) = weak.upgrade() {
                    operation.cancel_and_notify();
                }
            });
            *operation
                .cancel_callback
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = Some(Arc::clone(&callback));
            owner.register_cancel_callback(&callback);
        }
        Some(operation)
    }

    pub(super) fn id(&self) -> u64 {
        self.id
    }

    pub(super) fn session(&self) -> u64 {
        self.session
    }

    pub(super) fn nonce(&self) -> &str {
        &self.nonce
    }

    /// Assign the queue order once, before the operation is enqueued.
    pub(super) fn set_sequence(&self, sequence: u64) -> bool {
        sequence != 0
            && self
                .sequence
                .compare_exchange(0, sequence, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
    }

    pub(super) fn sequence(&self) -> Option<u64> {
        match self.sequence.load(Ordering::Acquire) {
            0 => None,
            sequence => Some(sequence),
        }
    }

    /// An operation may enter the renderer only once and only before cancellation.
    pub(super) fn admit(&self) -> bool {
        if let Some(owner) = &self.owner {
            return match owner.run_if_active(|| self.admit_inner()) {
                Some(admitted) => admitted,
                None => {
                    self.cancel_and_notify();
                    false
                }
            };
        }
        self.admit_inner()
    }

    fn admit_inner(&self) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.cancelled || state.admitted || state.reported.is_some() {
            return false;
        }
        state.admitted = true;
        true
    }

    fn cancel_and_notify(&self) {
        if self.cancel() {
            (self.revoke)(self.id);
        }
    }

    /// Returns true when the renderer must be asked to revoke this operation.
    pub(super) fn cancel(&self) -> bool {
        let (revoke, notification) = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            if state.cancelled || state.closed {
                return false;
            }
            state.cancelled = true;
            if state.admitted {
                (true, None)
            } else {
                (false, report(&mut state, Settlement::Cancelled, true))
            }
        };
        self.finish(notification);
        revoke
    }

    pub(super) fn ack_apply(&self, result: ApplyResult) -> Option<Settlement> {
        let notification = if let Some(owner) = &self.owner {
            match owner.run_if_active(|| self.ack_apply_inner(result)) {
                Some(notification) => notification,
                None => {
                    self.cancel_and_notify();
                    self.ack_apply_inner(result)
                }
            }
        } else {
            self.ack_apply_inner(result)
        };
        self.finish(notification)
    }

    fn ack_apply_inner(&self, result: ApplyResult) -> Option<Notification> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.admitted && !state.closed && state.apply.is_none() {
            state.apply = Some(result);
        }
        settle(&mut state)
    }

    pub(super) fn ack_revoke(&self, acknowledged: bool) -> Option<Settlement> {
        let notification = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            if state.cancelled && !state.closed && state.revoke.is_none() {
                state.revoke = Some(acknowledged);
            }
            settle(&mut state)
        };
        self.finish(notification)
    }

    /// Missing or invalid renderer evidence is not proof that an effect stopped.
    pub(super) fn fail_uncertain(&self) -> Option<Settlement> {
        let notification = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            let safe_to_release = !state.admitted;
            let outcome = if safe_to_release {
                Settlement::Rejected
            } else {
                Settlement::Uncertain
            };
            report(&mut state, outcome, safe_to_release)
        };
        self.finish(notification)
    }

    pub(super) fn settlement(&self) -> Option<Settlement> {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .reported
    }

    pub(super) fn observe(&self, observer: Observer) {
        let result = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(result) = state.reported {
                Some(result)
            } else {
                state.observers.push(Arc::clone(&observer));
                None
            }
        };
        if let Some(result) = result {
            observer(result);
        }
    }

    /// Notify once all renderer effects for this operation are accounted for.
    pub(super) fn observe_quiescence(&self, observer: CancelCallback) {
        let closed = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            if state.quiescence_published {
                true
            } else {
                state.quiescence_observers.push((0, Arc::clone(&observer)));
                false
            }
        };
        if closed {
            observer();
        }
    }

    fn subscribe_quiescence(self: &Arc<Self>, observer: CancelCallback) -> Option<Subscription> {
        let id = NEXT_OBSERVER_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .ok()?;
        let closed = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            if state.quiescence_published {
                true
            } else {
                state.quiescence_observers.push((id, Arc::clone(&observer)));
                false
            }
        };
        if closed {
            observer();
            None
        } else {
            Some(Subscription {
                id,
                operation: Arc::downgrade(self),
            })
        }
    }

    fn finish(&self, notification: Option<Notification>) -> Option<Settlement> {
        if let Some((result, closed, observers)) = notification {
            let quiescence_observers = if closed {
                OPERATIONS
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(&self.id);
                let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
                state.quiescence_published = true;
                std::mem::take(&mut state.quiescence_observers)
            } else {
                Vec::new()
            };
            for observer in observers {
                observer(result);
            }
            for (_, observer) in quiescence_observers {
                observer();
            }
            Some(result)
        } else {
            self.settlement()
        }
    }
}

type Notification = (Settlement, bool, Vec<Observer>);

fn report(state: &mut State, result: Settlement, closed: bool) -> Option<Notification> {
    if state.reported.is_some() {
        return None;
    }
    state.reported = Some(result);
    state.closed = closed;
    Some((result, closed, std::mem::take(&mut state.observers)))
}

fn settle(state: &mut State) -> Option<Notification> {
    if state.closed || !state.admitted {
        return None;
    }
    let apply = state.apply?;
    if apply == ApplyResult::Uncertain {
        return report(state, Settlement::Uncertain, false);
    }
    let result = if state.cancelled {
        if !state.revoke? {
            return report(state, Settlement::Uncertain, false);
        }
        if apply == ApplyResult::Applied {
            Settlement::AppliedBeforeCancellationSettled
        } else {
            Settlement::Cancelled
        }
    } else {
        match apply {
            ApplyResult::Applied => Settlement::Applied,
            ApplyResult::Skipped => Settlement::Skipped,
            ApplyResult::Rejected => Settlement::Rejected,
            ApplyResult::Uncertain => unreachable!("uncertain results do not settle"),
        }
    };
    if state.reported == Some(Settlement::Uncertain) {
        state.closed = true;
        return Some((Settlement::Uncertain, true, Vec::new()));
    }
    report(state, result, true)
}

pub(super) fn get(id: u64) -> Option<Arc<Operation>> {
    OPERATIONS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&id)
        .cloned()
}

/// Return timed-out operations without holding the registry during callbacks.
pub(super) fn expired(now: Instant) -> Vec<Arc<Operation>> {
    let operations: Vec<_> = OPERATIONS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .values()
        .cloned()
        .collect();
    operations
        .into_iter()
        .filter(|op| {
            let state = op.state.lock().unwrap_or_else(|e| e.into_inner());
            state.reported.is_none() && state.deadline.is_some_and(|deadline| deadline <= now)
        })
        .collect()
}

fn watchdog() {
    loop {
        std::thread::sleep(Duration::from_millis(250));
        for operation in expired(Instant::now()) {
            operation.fail_uncertain();
        }
        let operations = OPERATIONS.lock().unwrap_or_else(|e| e.into_inner());
        let pending = operations.values().any(|operation| {
            let state = operation.state.lock().unwrap_or_else(|e| e.into_inner());
            state.reported.is_none() && state.deadline.is_some()
        });
        if !pending {
            WATCHDOG_RUNNING.store(false, Ordering::Release);
            return;
        }
    }
}

/// Report lost renderer evidence without assuming that its document is gone.
pub(super) fn abandon_document(nonce: &str) {
    let operations: Vec<_> = OPERATIONS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .values()
        .filter(|operation| operation.nonce == nonce)
        .cloned()
        .collect();
    for operation in operations {
        operation.fail_uncertain();
    }
}

/// Retire both queued and already admitted operations for an editor session.
pub(super) fn cancel_session(session: u64) {
    let operations: Vec<_> = OPERATIONS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .values()
        .filter(|operation| operation.session == session)
        .cloned()
        .collect();
    for operation in operations {
        operation.cancel_and_notify();
    }
}

/// Notify when all registered operations from earlier editor sessions are quiescent.
pub(super) fn observe_prior_quiescence(session: u64, observer: CancelCallback) -> PriorObservation {
    let operations: Vec<_> = OPERATIONS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .values()
        .filter(|operation| operation.session < session)
        .cloned()
        .collect();
    cancel_and_observe_operations(operations, observer)
}

pub(super) struct PriorObservation {
    _subscriptions: Vec<Subscription>,
}

struct Subscription {
    id: u64,
    operation: Weak<Operation>,
}

impl Drop for Subscription {
    fn drop(&mut self) {
        if let Some(operation) = self.operation.upgrade() {
            operation
                .state
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .quiescence_observers
                .retain(|(id, _)| *id != self.id);
        }
    }
}

fn cancel_and_observe_operations(
    operations: Vec<Arc<Operation>>,
    observer: CancelCallback,
) -> PriorObservation {
    for operation in &operations {
        operation.cancel_and_notify();
    }
    observe_operations_quiescence(operations, observer)
}

fn observe_operations_quiescence(
    operations: Vec<Arc<Operation>>,
    observer: CancelCallback,
) -> PriorObservation {
    if operations.is_empty() {
        observer();
        return PriorObservation {
            _subscriptions: Vec::new(),
        };
    }
    let remaining = Arc::new(AtomicUsize::new(operations.len()));
    let mut subscriptions = Vec::new();
    for operation in operations {
        let remaining = Arc::clone(&remaining);
        let observer = Arc::clone(&observer);
        if let Some(subscription) = operation.subscribe_quiescence(Arc::new(move || {
            if remaining.fetch_sub(1, Ordering::AcqRel) == 1 {
                observer();
            }
        })) {
            subscriptions.push(subscription);
        }
    }
    PriorObservation {
        _subscriptions: subscriptions,
    }
}

/// Only call after destruction of this exact document has been established.
#[cfg(test)]
fn retire_document(nonce: &str) {
    let retired = {
        let mut operations = OPERATIONS.lock().unwrap_or_else(|e| e.into_inner());
        let ids: Vec<_> = operations
            .iter()
            .filter_map(|(id, op)| (op.nonce == nonce).then_some(*id))
            .collect();
        ids.into_iter()
            .filter_map(|id| operations.remove(&id))
            .collect::<Vec<_>>()
    };
    for operation in retired {
        let notification = {
            let mut state = operation.state.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(result) = state.reported {
                state.closed = true;
                Some((result, true, Vec::new()))
            } else {
                report(&mut state, Settlement::Uncertain, true)
            }
        };
        operation.finish(notification);
    }
}

#[cfg(test)]
#[path = "delivery_tests.rs"]
mod tests;
