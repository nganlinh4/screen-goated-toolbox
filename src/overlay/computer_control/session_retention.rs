//! Session-owned policy for automatic diagnostics and conversation retention.

use std::cell::RefCell;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock, Weak};

static ACTIVE: Mutex<Option<Weak<State>>> = Mutex::new(None);
static IDLE: OnceLock<SessionRetention> = OnceLock::new();
thread_local! {
    static CURRENT: RefCell<Option<SessionRetention>> = const { RefCell::new(None) };
}

#[derive(Clone)]
pub(super) struct SessionRetention(Arc<State>);

struct State {
    persistent: RwLock<bool>,
    closed: AtomicBool,
}

impl Default for SessionRetention {
    fn default() -> Self {
        Self(Arc::new(State {
            persistent: RwLock::new(true),
            closed: AtomicBool::new(false),
        }))
    }
}

impl SessionRetention {
    pub(super) fn enter(&self) -> Scope {
        let previous = CURRENT.with(|current| current.replace(Some(self.clone())));
        Scope {
            previous,
            owner: None,
            thread: PhantomData,
        }
    }

    pub(super) fn forbid(&self) {
        *self
            .0
            .persistent
            .write()
            .unwrap_or_else(|error| error.into_inner()) = false;
    }

    pub(super) fn allows(&self) -> bool {
        *self
            .0
            .persistent
            .read()
            .unwrap_or_else(|error| error.into_inner())
    }

    pub(super) fn same_session(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    fn persist<T>(&self, effect: impl FnOnce() -> T) -> Option<T> {
        let allowed = self
            .0
            .persistent
            .read()
            .unwrap_or_else(|error| error.into_inner());
        allowed.then(effect)
    }

    pub(super) fn while_open<T>(&self, effect: impl FnOnce() -> T) -> Option<T> {
        let _guard = self
            .0
            .persistent
            .read()
            .unwrap_or_else(|error| error.into_inner());
        (!self.0.closed.load(Ordering::SeqCst)).then(effect)
    }

    fn close(&self) {
        let mut allowed = self
            .0
            .persistent
            .write()
            .unwrap_or_else(|error| error.into_inner());
        *allowed = false;
        self.0.closed.store(true, Ordering::SeqCst);
    }
}

pub(super) struct Scope {
    previous: Option<SessionRetention>,
    owner: Option<SessionRetention>,
    thread: PhantomData<Rc<()>>,
}

impl Drop for Scope {
    fn drop(&mut self) {
        if let Some(owner) = &self.owner {
            owner.close();
            super::artifacts::retire_session(owner);
        }
        CURRENT.with(|current| current.replace(self.previous.take()));
    }
}

pub(super) fn start_session() -> Scope {
    let policy = SessionRetention::default();
    // Tests bind explicit thread-local owners without changing other tests.
    if !cfg!(test) {
        *ACTIVE.lock().unwrap_or_else(|error| error.into_inner()) = Some(Arc::downgrade(&policy.0));
        super::artifacts::sweep_orphaned_cache();
    }
    let mut scope = policy.enter();
    scope.owner = Some(policy);
    scope
}

pub(super) fn current() -> SessionRetention {
    CURRENT
        .with(|current| current.borrow().clone())
        .unwrap_or_else(|| {
            ACTIVE
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .as_ref()
                .and_then(Weak::upgrade)
                .map(SessionRetention)
                .unwrap_or_else(|| IDLE.get_or_init(SessionRetention::default).clone())
        })
}

/// Stop the live session, if any, from persisting anything further. Writes
/// already inside `persist` finish first; the idle policy is left unchanged.
pub(super) fn forbid_active_session() {
    let active = ACTIVE
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .as_ref()
        .and_then(Weak::upgrade);
    if let Some(state) = active {
        SessionRetention(state).forbid();
    }
}

/// Serialize actual sink writes with the transition to nonpersistent input.
/// Effects must not nest this boundary or change the session policy.
pub(super) fn persist<T>(effect: impl FnOnce() -> T) -> Option<T> {
    current().persist(effect)
}

/// Detached work keeps its original policy even after another session starts.
pub(super) fn inherit<T>(effect: impl FnOnce() -> T) -> impl FnOnce() -> T {
    let owner = current();
    move || {
        let _scope = owner.enter();
        effect()
    }
}

#[cfg(test)]
mod tests;
