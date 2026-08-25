//! Bounded process-wide execution for short-lived background work.
//!
//! Window message loops, COM apartments, audio capture, and long-lived protocol
//! sessions keep dedicated threads. Everything else should use this module so a
//! burst of UI actions cannot create an unbounded number of operating-system
//! threads.

use anyhow::{Result, anyhow};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, LazyLock, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const INTERACTIVE_QUEUE: usize = 32;
const IO_QUEUE: usize = 128;
const CPU_QUEUE: usize = 32;
const MAINTENANCE_QUEUE: usize = 16;

static NEXT_TASK_ID: AtomicU64 = AtomicU64::new(1);
static GLOBAL_STATS: Stats = Stats::new();
static INTERACTIVE: LazyLock<Executor> = LazyLock::new(|| Executor::new(TaskClass::Interactive));
static IO: LazyLock<Executor> = LazyLock::new(|| Executor::new(TaskClass::Io));
static CPU: LazyLock<Executor> = LazyLock::new(|| Executor::new(TaskClass::Cpu));
static MAINTENANCE: LazyLock<Executor> = LazyLock::new(|| Executor::new(TaskClass::Maintenance));

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TaskClass {
    Interactive,
    Io,
    Cpu,
    Maintenance,
}

impl TaskClass {
    fn executor(self) -> &'static Executor {
        match self {
            Self::Interactive => &INTERACTIVE,
            Self::Io => &IO,
            Self::Cpu => &CPU,
            Self::Maintenance => &MAINTENANCE,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Interactive => "interactive",
            Self::Io => "io",
            Self::Cpu => "cpu",
            Self::Maintenance => "maintenance",
        }
    }
}

#[derive(Clone, Default)]
pub(crate) struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub(crate) fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

pub(crate) struct TaskContext {
    id: u64,
    cancellation: CancellationToken,
    deadline: Option<Instant>,
}

impl TaskContext {
    pub(crate) fn id(&self) -> u64 {
        self.id
    }

    pub(crate) fn should_stop(&self) -> bool {
        self.cancellation.is_cancelled()
            || self
                .deadline
                .is_some_and(|deadline| Instant::now() >= deadline)
    }
}

#[derive(Clone)]
pub(crate) struct TaskTicket {
    id: u64,
    cancellation: CancellationToken,
}

impl TaskTicket {
    pub(crate) fn id(&self) -> u64 {
        self.id
    }

    pub(crate) fn cancel(&self) {
        self.cancellation.cancel();
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct RuntimeSnapshot {
    pub(crate) queued: usize,
    pub(crate) active: usize,
    pub(crate) completed: u64,
    pub(crate) rejected: u64,
    pub(crate) workers: usize,
}

pub(crate) fn spawn(
    class: TaskClass,
    name: &'static str,
    work: impl FnOnce(TaskContext) + Send + 'static,
) -> Result<TaskTicket> {
    spawn_with_deadline(class, name, None, work)
}

pub(crate) fn spawn_detached(
    class: TaskClass,
    name: &'static str,
    work: impl FnOnce() + Send + 'static,
) {
    if let Err(error) = spawn(class, name, move |_| work()) {
        crate::log_info!(
            "[TaskRuntime] detached_not_queued class={} name={name} error={error:#}",
            class.label()
        );
    }
}

pub(crate) fn spawn_with_timeout(
    class: TaskClass,
    name: &'static str,
    timeout: Duration,
    work: impl FnOnce(TaskContext) + Send + 'static,
) -> Result<TaskTicket> {
    spawn_with_deadline(class, name, Some(Instant::now() + timeout), work)
}

fn spawn_with_deadline(
    class: TaskClass,
    name: &'static str,
    deadline: Option<Instant>,
    work: impl FnOnce(TaskContext) + Send + 'static,
) -> Result<TaskTicket> {
    let id = NEXT_TASK_ID.fetch_add(1, Ordering::Relaxed);
    let cancellation = CancellationToken::default();
    let task = Task {
        id,
        name,
        cancellation: cancellation.clone(),
        deadline,
        work: Box::new(work),
    };
    if let Err(error) = class.executor().submit(task) {
        let current = snapshot();
        crate::log_info!(
            "[TaskRuntime] submit_failed class={} id={id} queued={} active={} workers={} completed={} rejected={} error={error:#}",
            class.label(),
            current.queued,
            current.active,
            current.workers,
            current.completed,
            current.rejected
        );
        return Err(error);
    }
    Ok(TaskTicket { id, cancellation })
}

pub(crate) fn snapshot() -> RuntimeSnapshot {
    GLOBAL_STATS.snapshot()
}

struct Task {
    id: u64,
    name: &'static str,
    cancellation: CancellationToken,
    deadline: Option<Instant>,
    work: Box<dyn FnOnce(TaskContext) + Send + 'static>,
}

struct Executor {
    class: TaskClass,
    sender: SyncSender<Task>,
    receiver: Arc<Mutex<Receiver<Task>>>,
    state: Arc<ExecutorState>,
    max_workers: usize,
}

struct ExecutorState {
    queued: AtomicUsize,
    active: AtomicUsize,
    workers: AtomicUsize,
}

struct Stats {
    queued: AtomicUsize,
    active: AtomicUsize,
    completed: AtomicU64,
    rejected: AtomicU64,
    workers: AtomicUsize,
}

impl Stats {
    const fn new() -> Self {
        Self {
            queued: AtomicUsize::new(0),
            active: AtomicUsize::new(0),
            completed: AtomicU64::new(0),
            rejected: AtomicU64::new(0),
            workers: AtomicUsize::new(0),
        }
    }

    fn snapshot(&self) -> RuntimeSnapshot {
        RuntimeSnapshot {
            queued: self.queued.load(Ordering::Acquire),
            active: self.active.load(Ordering::Acquire),
            completed: self.completed.load(Ordering::Relaxed),
            rejected: self.rejected.load(Ordering::Relaxed),
            workers: self.workers.load(Ordering::Relaxed),
        }
    }
}

impl Executor {
    fn new(class: TaskClass) -> Self {
        let (max_workers, capacity) = pool_shape(class);
        let (sender, receiver) = sync_channel::<Task>(capacity);
        let receiver = Arc::new(Mutex::new(receiver));
        let state = Arc::new(ExecutorState {
            queued: AtomicUsize::new(0),
            active: AtomicUsize::new(0),
            workers: AtomicUsize::new(0),
        });
        let executor = Self {
            class,
            sender,
            receiver,
            state,
            max_workers,
        };
        executor
            .start_worker()
            .expect("failed to start bounded task worker");
        executor
    }

    fn submit(&self, task: Task) -> Result<()> {
        GLOBAL_STATS.queued.fetch_add(1, Ordering::AcqRel);
        self.state.queued.fetch_add(1, Ordering::AcqRel);
        match self.sender.try_send(task) {
            Ok(()) => {
                self.scale_for_demand();
                Ok(())
            }
            Err(TrySendError::Full(task)) => {
                GLOBAL_STATS.queued.fetch_sub(1, Ordering::AcqRel);
                self.state.queued.fetch_sub(1, Ordering::AcqRel);
                GLOBAL_STATS.rejected.fetch_add(1, Ordering::Relaxed);
                Err(anyhow!(
                    "{} task queue is full; rejected {} ({})",
                    self.class.label(),
                    task.name,
                    task.id
                ))
            }
            Err(TrySendError::Disconnected(task)) => {
                GLOBAL_STATS.queued.fetch_sub(1, Ordering::AcqRel);
                self.state.queued.fetch_sub(1, Ordering::AcqRel);
                GLOBAL_STATS.rejected.fetch_add(1, Ordering::Relaxed);
                Err(anyhow!(
                    "{} task runtime is unavailable; rejected {} ({})",
                    self.class.label(),
                    task.name,
                    task.id
                ))
            }
        }
    }

    fn scale_for_demand(&self) {
        let demand = self
            .state
            .active
            .load(Ordering::Acquire)
            .saturating_add(self.state.queued.load(Ordering::Acquire))
            .min(self.max_workers);
        while self.state.workers.load(Ordering::Acquire) < demand {
            if let Err(error) = self.start_worker() {
                crate::log_info!(
                    "[TaskRuntime] worker_start_failed class={} error={error:#}",
                    self.class.label()
                );
                break;
            }
        }
    }

    fn start_worker(&self) -> Result<()> {
        let index = self
            .state
            .workers
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |workers| {
                (workers < self.max_workers).then_some(workers + 1)
            })
            .map_err(|_| anyhow!("{} worker limit reached", self.class.label()))?;
        let receiver = Arc::clone(&self.receiver);
        let state = Arc::clone(&self.state);
        let name = format!("sgt-{}-{}", self.class.label(), index + 1);
        if let Err(error) = thread::Builder::new()
            .name(name)
            .spawn(move || worker_loop(receiver, state))
        {
            self.state.workers.fetch_sub(1, Ordering::AcqRel);
            return Err(error.into());
        }
        GLOBAL_STATS.workers.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}

fn pool_shape(class: TaskClass) -> (usize, usize) {
    let parallelism = thread::available_parallelism().map_or(4, usize::from);
    match class {
        TaskClass::Interactive => (2, INTERACTIVE_QUEUE),
        TaskClass::Io => (parallelism.clamp(2, 6), IO_QUEUE),
        TaskClass::Cpu => (parallelism.saturating_sub(1).clamp(1, 6), CPU_QUEUE),
        TaskClass::Maintenance => (1, MAINTENANCE_QUEUE),
    }
}

fn worker_loop(receiver: Arc<Mutex<Receiver<Task>>>, state: Arc<ExecutorState>) {
    loop {
        let task = {
            let Ok(receiver) = receiver.lock() else {
                return;
            };
            match receiver.recv() {
                Ok(task) => task,
                Err(_) => return,
            }
        };
        GLOBAL_STATS.queued.fetch_sub(1, Ordering::AcqRel);
        state.queued.fetch_sub(1, Ordering::AcqRel);
        GLOBAL_STATS.active.fetch_add(1, Ordering::AcqRel);
        state.active.fetch_add(1, Ordering::AcqRel);
        let context = TaskContext {
            id: task.id,
            cancellation: task.cancellation,
            deadline: task.deadline,
        };
        if !context.should_stop()
            && catch_unwind(AssertUnwindSafe(|| (task.work)(context))).is_err()
        {
            crate::log_info!(
                "[TaskRuntime] task_panicked name={} id={}",
                task.name,
                task.id
            );
        }
        GLOBAL_STATS.active.fetch_sub(1, Ordering::AcqRel);
        state.active.fetch_sub(1, Ordering::AcqRel);
        GLOBAL_STATS.completed.fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_is_monotonic() {
        let cancellation = CancellationToken::default();
        assert!(!cancellation.is_cancelled());
        cancellation.cancel();
        assert!(cancellation.is_cancelled());
    }

    #[test]
    fn expired_deadline_skips_work() {
        let ran = Arc::new(AtomicBool::new(false));
        let ran_in_task = Arc::clone(&ran);
        let _ticket = spawn_with_timeout(
            TaskClass::Maintenance,
            "expired-test",
            Duration::ZERO,
            move |_| ran_in_task.store(true, Ordering::Release),
        )
        .unwrap();
        thread::sleep(Duration::from_millis(25));
        assert!(!ran.load(Ordering::Acquire));
    }

    #[test]
    fn pool_shapes_are_bounded() {
        for class in [
            TaskClass::Interactive,
            TaskClass::Io,
            TaskClass::Cpu,
            TaskClass::Maintenance,
        ] {
            let (workers, capacity) = pool_shape(class);
            assert!((1..=6).contains(&workers));
            assert!((16..=128).contains(&capacity));
        }
    }
}
