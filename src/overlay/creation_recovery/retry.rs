use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const RETRY_DELAY: Duration = Duration::from_secs(2);

enum State {
    Idle(Option<Instant>),
    Running,
    Complete,
}
struct Gate(Mutex<State>);

impl Gate {
    fn begin(&self, now: Instant) -> Option<Attempt<'_>> {
        let mut state = self.0.lock().unwrap_or_else(|error| error.into_inner());
        if !matches!(*state, State::Idle(None))
            && !matches!(*state, State::Idle(Some(retry)) if now >= retry)
        {
            return None;
        }
        *state = State::Running;
        Some(Attempt {
            gate: self,
            complete: false,
        })
    }

    fn pending(&self) -> bool {
        !matches!(
            *self.0.lock().unwrap_or_else(|error| error.into_inner()),
            State::Complete
        )
    }
}

pub(crate) struct Attempt<'a> {
    gate: &'a Gate,
    complete: bool,
}

impl Attempt<'_> {
    pub(crate) fn finish(&mut self) {
        self.complete = true;
    }
}

impl Drop for Attempt<'_> {
    fn drop(&mut self) {
        let mut state = self
            .gate
            .0
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        *state = if self.complete {
            State::Complete
        } else {
            State::Idle(Some(Instant::now() + RETRY_DELAY))
        };
    }
}

pub(crate) struct Retry {
    gate: Gate,
    worker_active: AtomicBool,
    product: &'static str,
    recover: fn(),
    started: AtomicBool,
}

impl Retry {
    pub(crate) const fn new(product: &'static str, recover: fn()) -> Self {
        Self {
            gate: Gate(Mutex::new(State::Idle(None))),
            worker_active: AtomicBool::new(false),
            product,
            recover,
            started: AtomicBool::new(false),
        }
    }

    pub(crate) fn begin(&self) -> Option<Attempt<'_>> {
        self.gate.begin(Instant::now())
    }
    pub(crate) fn pending(&self) -> bool {
        self.gate.pending()
    }

    pub(crate) fn start<G>(
        &'static self,
        admit: impl FnOnce() -> Option<G>,
        recover: impl FnOnce() -> bool,
    ) {
        if crate::overlay::creation_close::is_closing(self.product) {
            return;
        }
        if self
            .started
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            self.run(admit, recover);
        } else {
            self.schedule();
        }
    }

    pub(crate) fn run<G>(
        &'static self,
        admit: impl FnOnce() -> Option<G>,
        recover: impl FnOnce() -> bool,
    ) {
        if crate::overlay::creation_close::is_closing(self.product) {
            return;
        }
        let Some(_admission) = admit() else {
            self.schedule();
            return;
        };
        let Some(mut attempt) = self.begin() else {
            if self.pending() {
                self.schedule();
            }
            return;
        };
        self.schedule();
        if recover() {
            attempt.finish();
        }
    }

    pub(crate) fn schedule(&'static self) {
        if crate::overlay::creation_close::is_closing(self.product)
            || !self.pending()
            || self
                .worker_active
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
        {
            return;
        }
        std::thread::spawn(move || {
            let _worker = Worker(self);
            loop {
                std::thread::sleep(RETRY_DELAY);
                if crate::overlay::creation_close::is_closing(self.product) || !self.pending() {
                    break;
                }
                (self.recover)();
            }
        });
    }
}

struct Worker(&'static Retry);
impl Drop for Worker {
    fn drop(&mut self) {
        self.0.worker_active.store(false, Ordering::Release);
        if self.0.pending() && !crate::overlay::creation_close::is_closing(self.0.product) {
            self.0.schedule();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::{Sender, channel};

    static WORKER_RETRY: Retry = Retry::new("svg", recover_on_worker);
    static COMPLETED: Mutex<Option<Sender<()>>> = Mutex::new(None);

    fn recover_on_worker() {
        WORKER_RETRY.run(|| Some(()), || true);
        if !WORKER_RETRY.pending()
            && let Some(sender) = COMPLETED.lock().unwrap().take()
        {
            sender.send(()).unwrap();
        }
    }

    #[test]
    fn queued_recovery_worker_retries_without_repeating_foreground_io() {
        let name = format!(
            "{}::queued_recovery_worker_retries_without_repeating_foreground_io",
            module_path!().split_once("::").unwrap().1
        );
        crate::overlay::creation_submission_test::run_isolated_without_fault(&name, |_| {
            let (sender, receiver) = channel();
            *COMPLETED.lock().unwrap() = Some(sender);
            WORKER_RETRY.start(|| Some(()), || false);
            assert!(WORKER_RETRY.pending());
            for _ in 0..20 {
                WORKER_RETRY.start(
                    || -> Option<()> { panic!("foreground admission repeated") },
                    || panic!("foreground recovery repeated"),
                );
            }
            receiver.recv_timeout(Duration::from_secs(8)).unwrap();
            assert!(!WORKER_RETRY.pending());
            WORKER_RETRY.start(
                || -> Option<()> { panic!("completed recovery restarted") },
                || panic!("completed recovery restarted"),
            );
        });
    }

    #[test]
    fn failed_attempt_retries_after_backoff_and_success_is_once_only() {
        let gate = Gate(Mutex::new(State::Idle(None)));
        let now = Instant::now();
        let first = gate.begin(now).unwrap();
        assert!(gate.begin(now).is_none());
        drop(first);
        assert!(gate.begin(Instant::now()).is_none());
        let mut retry = gate.begin(Instant::now() + Duration::from_secs(3)).unwrap();
        retry.finish();
        drop(retry);
        assert!(!gate.pending());
        assert!(
            gate.begin(Instant::now() + Duration::from_secs(6))
                .is_none()
        );
    }
}
