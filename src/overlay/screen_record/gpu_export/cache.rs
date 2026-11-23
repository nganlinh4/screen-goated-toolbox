use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

const WAIT_SLICE: Duration = Duration::from_millis(50);

enum State<T> {
    Empty,
    Running {
        id: u64,
        started: Instant,
        expired: bool,
    },
    Ready(Arc<T>),
    Failed {
        id: u64,
        error: String,
    },
}

struct CacheState<T> {
    state: State<T>,
    next_id: u64,
}

struct Inner<T> {
    state: Mutex<CacheState<T>>,
    changed: Condvar,
}

/// One driver initializer at a time; no driver call or destruction holds the lock.
pub(super) struct ContextCache<T> {
    inner: Arc<Inner<T>>,
}

impl<T: Send + Sync + 'static> ContextCache<T> {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                state: Mutex::new(CacheState {
                    state: State::Empty,
                    next_id: 0,
                }),
                changed: Condvar::new(),
            }),
        }
    }

    pub fn get_or_initialize(
        &self,
        cancel: Option<&AtomicBool>,
        timeout: Duration,
        usable: impl Fn(&T) -> bool,
        create: impl FnOnce() -> Result<T, String> + Send + 'static,
    ) -> Result<Arc<T>, String> {
        let started_waiting = Instant::now();
        let mut factory = Some(create);
        let mut awaited_id = None;
        loop {
            if cancel.is_some_and(|flag| flag.load(Ordering::Acquire)) {
                return Err("GPU initialization cancelled".to_string());
            }
            let mut locked = self
                .inner
                .state
                .lock()
                .map_err(|_| "GPU context cache is unavailable".to_string())?;
            match &locked.state {
                State::Ready(context) if usable(context) => return Ok(Arc::clone(context)),
                State::Failed { id, error } if awaited_id == Some(*id) => return Err(error.clone()),
                State::Ready(_) | State::Empty | State::Failed { .. } => {
                    let make = factory
                        .take()
                        .ok_or_else(|| "GPU initialization expired; retry export".to_string())?;
                    locked.next_id = locked
                        .next_id
                        .checked_add(1)
                        .ok_or_else(|| "GPU context generation exhausted".to_string())?;
                    let id = locked.next_id;
                    let started = Instant::now();
                    let retired = match std::mem::replace(&mut locked.state, State::Empty) {
                        State::Ready(context) => Some(context),
                        _ => None,
                    };
                    // Keep our own reference until spawn succeeds. A failed spawn
                    // drops its closure on this thread; that must not drop a device
                    // while the cache lock is held.
                    let retired = Arc::new(Mutex::new(retired));
                    let worker_retired = Arc::clone(&retired);
                    locked.state = State::Running {
                        id,
                        started,
                        expired: false,
                    };
                    let inner = Arc::clone(&self.inner);
                    if let Err(error) = std::thread::Builder::new().name("sgt-gpu-context-init".to_string()).spawn(move || {
                        // Device destruction can block in a driver as well. Keep it
                        // in the single-flight worker and outside the cache lock.
                        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            let old = worker_retired.lock().ok().and_then(|mut slot| slot.take());
                            drop(old);
                            make()
                        }))
                        .unwrap_or_else(|_| Err("GPU initialization or device retirement panicked".to_string()));
                        let mut locked = match inner.state.lock() { Ok(value) => value, Err(_) => return };
                        let publish = matches!(&locked.state, State::Running { id: active, started, expired: false } if *active == id && started.elapsed() < timeout);
                        if publish {
                            locked.state = match result {
                                Ok(value) => State::Ready(Arc::new(value)),
                                Err(error) => State::Failed { id, error },
                            };
                            inner.changed.notify_all();
                        } else {
                            drop(locked);
                            // Keep the generation occupied until stale-device
                            // destruction also completes, without holding the lock.
                            let retired = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(result))).is_ok();
                            if let Ok(mut locked) = inner.state.lock()
                                && matches!(&locked.state, State::Running { id: active, .. } if *active == id)
                            {
                                locked.state = if retired {
                                    State::Empty
                                } else {
                                    State::Failed { id, error: "GPU stale device retirement panicked".to_string() }
                                };
                                inner.changed.notify_all();
                            }
                        }
                    }) {
                        let old = retired.lock().map_err(|_| "GPU context retirement is unavailable".to_string())?.take();
                        locked.state = old.map_or(State::Empty, State::Ready);
                        self.inner.changed.notify_all();
                        return Err(format!("Could not start GPU initialization: {error}"));
                    }
                    awaited_id = Some(id);
                }
                State::Running {
                    id,
                    started,
                    expired,
                } => {
                    awaited_id = Some(*id);
                    if *expired || started.elapsed() >= timeout {
                        if let State::Running { expired, .. } = &mut locked.state {
                            *expired = true;
                        }
                        return Err(
                            "GPU initialization did not finish; retry after it stops".to_string()
                        );
                    }
                    if started_waiting.elapsed() >= timeout {
                        return Err("Timed out waiting for GPU initialization".to_string());
                    }
                    let _guard = self
                        .inner
                        .changed
                        .wait_timeout(locked, WAIT_SLICE)
                        .map_err(|_| "GPU context cache is unavailable".to_string())?;
                }
            }
        }
    }

    pub fn ready_snapshot(&self) -> Option<(Arc<T>, u64)> {
        let state = self.inner.state.lock().ok()?;
        match &state.state {
            State::Ready(context) => Some((context.clone(), state.next_id)),
            _ => None,
        }
    }

    #[cfg(test)]
    pub fn is_ready(&self) -> bool {
        self.inner
            .state
            .lock()
            .is_ok_and(|state| matches!(state.state, State::Ready(_)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn initializer_panic_allows_retry() {
        let cache = ContextCache::<u32>::new();
        let error = cache
            .get_or_initialize(
                None,
                Duration::from_secs(1),
                |_| true,
                || panic!("initializer failure"),
            )
            .unwrap_err();
        assert!(error.contains("panicked"));
        let value = cache
            .get_or_initialize(None, Duration::from_secs(1), |_| true, || Ok(42))
            .unwrap();
        assert_eq!(*value, 42);
    }

    #[test]
    fn retired_device_panic_allows_retry_without_running_factory() {
        #[derive(Debug)]
        struct Device(bool);
        impl Drop for Device {
            fn drop(&mut self) {
                assert!(!self.0, "retirement failure");
            }
        }
        let cache = ContextCache::new();
        drop(
            cache
                .get_or_initialize(None, Duration::from_secs(1), |_| true, || Ok(Device(true)))
                .unwrap(),
        );
        let created = Arc::new(AtomicBool::new(false));
        let called = Arc::clone(&created);
        let error = cache
            .get_or_initialize(
                None,
                Duration::from_secs(1),
                |_| false,
                move || {
                    called.store(true, Ordering::Release);
                    Ok(Device(false))
                },
            )
            .unwrap_err();
        assert!(error.contains("panicked"));
        assert!(!created.load(Ordering::Acquire));
        assert!(
            cache
                .get_or_initialize(None, Duration::from_secs(1), |_| true, || Ok(Device(false)))
                .is_ok()
        );
    }

    #[test]
    fn stale_device_drop_panic_keeps_single_flight_and_allows_retry() {
        struct Device {
            value: u32,
            block_drop: Mutex<Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>>,
        }
        impl Drop for Device {
            fn drop(&mut self) {
                if let Some((started, release)) = self.block_drop.get_mut().unwrap().take() {
                    started.send(()).unwrap();
                    release.recv().unwrap();
                    panic!("stale retirement failure");
                }
            }
        }
        let cache = ContextCache::new();
        let (create_tx, create_rx) = mpsc::channel();
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let timeout = Duration::from_millis(100);
        assert!(
            cache
                .get_or_initialize(
                    None,
                    timeout,
                    |_: &Device| true,
                    move || {
                        create_rx.recv().unwrap();
                        Ok(Device {
                            value: 1,
                            block_drop: Mutex::new(Some((started_tx, release_rx))),
                        })
                    }
                )
                .is_err()
        );
        create_tx.send(()).unwrap();
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(
            cache
                .get_or_initialize(
                    None,
                    timeout,
                    |_: &Device| true,
                    || { panic!("must not overlap stale destruction") }
                )
                .is_err()
        );
        release_tx.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match cache.get_or_initialize(
                None,
                timeout,
                |_: &Device| true,
                || {
                    Ok(Device {
                        value: 2,
                        block_drop: Mutex::new(None),
                    })
                },
            ) {
                Ok(device) => {
                    assert_eq!(device.value, 2);
                    break;
                }
                Err(_) if Instant::now() < deadline => std::thread::yield_now(),
                Err(error) => panic!("stale device did not retire: {error}"),
            }
        }
    }

    #[test]
    fn failed_creation_can_retry_and_lost_context_is_replaced() {
        let cache = ContextCache::new();
        assert!(
            cache
                .get_or_initialize(
                    None,
                    Duration::from_secs(1),
                    |_| true,
                    || Err::<AtomicBool, _>("temporary".into())
                )
                .is_err()
        );
        let first = cache
            .get_or_initialize(
                None,
                Duration::from_secs(1),
                |v| !v.load(Ordering::Acquire),
                || Ok(AtomicBool::new(false)),
            )
            .unwrap();
        let same = cache
            .get_or_initialize(
                None,
                Duration::from_secs(1),
                |v| !v.load(Ordering::Acquire),
                || unreachable!(),
            )
            .unwrap();
        assert!(Arc::ptr_eq(&first, &same));
        first.store(true, Ordering::Release);
        let replacement = cache
            .get_or_initialize(
                None,
                Duration::from_secs(1),
                |v| !v.load(Ordering::Acquire),
                || Ok(AtomicBool::new(false)),
            )
            .unwrap();
        assert!(!Arc::ptr_eq(&first, &replacement));
    }

    #[test]
    fn timed_out_initializer_is_single_flight_and_late_result_is_retired() {
        let cache = ContextCache::new();
        let (release_tx, release_rx) = mpsc::channel();
        let wait = Duration::from_millis(120);
        let error = cache
            .get_or_initialize(
                None,
                wait,
                |_: &u32| true,
                move || {
                    release_rx.recv().unwrap();
                    Ok(1)
                },
            )
            .unwrap_err();
        assert!(error.contains("initialization"));
        assert!(
            cache
                .get_or_initialize(
                    None,
                    wait,
                    |_: &u32| true,
                    || panic!("must not start a second initializer")
                )
                .is_err()
        );
        release_tx.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match cache.get_or_initialize(None, wait, |_: &u32| true, || Ok(2)) {
                Ok(value) => {
                    assert_eq!(*value, 2);
                    break;
                }
                Err(_) if Instant::now() < deadline => std::thread::yield_now(),
                Err(error) => panic!("stale initializer did not retire: {error}"),
            }
        }
    }

    #[test]
    fn cancellation_releases_waiter_without_stopping_other_consumers() {
        let cache = Arc::new(ContextCache::new());
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cache = Arc::clone(&cache);
        let worker_cancel = Arc::clone(&cancel);
        let waiter = std::thread::spawn(move || {
            worker_cache.get_or_initialize(
                Some(&worker_cancel),
                Duration::from_secs(2),
                |_: &u32| true,
                move || {
                    started_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    Ok(7)
                },
            )
        });
        started_rx.recv().unwrap();
        cancel.store(true, Ordering::Release);
        assert!(waiter.join().unwrap().unwrap_err().contains("cancel"));
        release_tx.send(()).unwrap();
        let value = cache
            .get_or_initialize(
                None,
                Duration::from_secs(2),
                |_: &u32| true,
                || panic!("must reuse shared initializer"),
            )
            .unwrap();
        assert_eq!(*value, 7);
    }

    #[test]
    fn stale_device_destruction_remains_single_flight() {
        struct TestDevice {
            value: u32,
            block_drop: Mutex<Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>>,
        }
        impl Drop for TestDevice {
            fn drop(&mut self) {
                if let Some((started, release)) = self.block_drop.get_mut().unwrap().take() {
                    let _ = started.send(());
                    let _ = release.recv();
                }
            }
        }
        let cache = ContextCache::new();
        let (create_tx, create_rx) = mpsc::channel();
        let (drop_started_tx, drop_started_rx) = mpsc::channel();
        let (drop_release_tx, drop_release_rx) = mpsc::channel();
        let timeout = Duration::from_millis(100);
        assert!(
            cache
                .get_or_initialize(
                    None,
                    timeout,
                    |_: &TestDevice| true,
                    move || {
                        create_rx.recv().unwrap();
                        Ok(TestDevice {
                            value: 1,
                            block_drop: Mutex::new(Some((drop_started_tx, drop_release_rx))),
                        })
                    }
                )
                .is_err()
        );
        create_tx.send(()).unwrap();
        drop_started_rx
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
        assert!(
            cache
                .get_or_initialize(
                    None,
                    timeout,
                    |_: &TestDevice| true,
                    || panic!("destruction has not finished")
                )
                .is_err()
        );
        drop_release_tx.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match cache.get_or_initialize(
                None,
                timeout,
                |_: &TestDevice| true,
                || {
                    Ok(TestDevice {
                        value: 2,
                        block_drop: Mutex::new(None),
                    })
                },
            ) {
                Ok(device) => {
                    assert_eq!(device.value, 2);
                    break;
                }
                Err(_) if Instant::now() < deadline => std::thread::yield_now(),
                Err(error) => panic!("device destruction did not finish: {error}"),
            }
        }
    }
}
