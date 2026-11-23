//! One background thread that writes cursor changes to the system.
//!
//! Applying a pack rasterizes every cursor, writes the registry and
//! broadcasts to every top-level window; none of that may run on the UI
//! thread. Live slider previews are superseded by any newer request, so a
//! drag never builds a backlog. Commits (a pack click or a slider release)
//! always run, because their outcome is shown to the user.

use super::backend::{CursorCollectionSpec, apply_cursor_base_size, apply_downloaded_collection};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

pub(super) enum ApplyTarget {
    /// `chosen` marks a pack click; a size change re-applies the current pack.
    Pack {
        spec: CursorCollectionSpec,
        files: HashMap<String, PathBuf>,
        chosen: bool,
    },
    SystemSize,
}

pub(super) struct ApplyRequest {
    pub(super) target: ApplyTarget,
    pub(super) size: u32,
    pub(super) live_preview_only: bool,
}

pub(super) struct ApplyOutcome {
    /// The pack the user chose, for a pack click; `None` for size changes.
    pub(super) chosen_pack: Option<CursorCollectionSpec>,
    pub(super) result: Result<u32, String>,
}

#[derive(Default)]
struct Queue {
    pending: VecDeque<ApplyRequest>,
    running: bool,
    closed: bool,
}

pub(super) struct ApplyWorker {
    shared: Arc<(Mutex<Queue>, Condvar)>,
    outcomes: Receiver<ApplyOutcome>,
}

/// A newer request makes every pending live preview obsolete.
fn enqueue(pending: &mut VecDeque<ApplyRequest>, request: ApplyRequest) {
    pending.retain(|queued| !queued.live_preview_only);
    pending.push_back(request);
}

impl ApplyWorker {
    pub(super) fn spawn() -> Self {
        let shared = Arc::new((Mutex::new(Queue::default()), Condvar::new()));
        let (sender, outcomes) = mpsc::channel();
        let worker_shared = shared.clone();
        let _ = thread::Builder::new()
            .name("pointer-apply".to_string())
            .spawn(move || run(worker_shared, sender));
        Self { shared, outcomes }
    }

    pub(super) fn submit(&self, request: ApplyRequest) {
        let (lock, signal) = &*self.shared;
        if let Ok(mut queue) = lock.lock() {
            enqueue(&mut queue.pending, request);
            signal.notify_one();
        }
    }

    pub(super) fn try_outcome(&self) -> Option<ApplyOutcome> {
        self.outcomes.try_recv().ok()
    }

    pub(super) fn is_busy(&self) -> bool {
        self.shared
            .0
            .lock()
            .map(|queue| queue.running || !queue.pending.is_empty())
            .unwrap_or(false)
    }
}

impl Drop for ApplyWorker {
    fn drop(&mut self) {
        let (lock, signal) = &*self.shared;
        if let Ok(mut queue) = lock.lock() {
            queue.closed = true;
            signal.notify_one();
        }
    }
}

fn run(shared: Arc<(Mutex<Queue>, Condvar)>, outcomes: Sender<ApplyOutcome>) {
    let (lock, signal) = &*shared;
    loop {
        let request = {
            let Ok(mut queue) = lock.lock() else {
                return;
            };
            queue.running = false;
            loop {
                if queue.closed {
                    return;
                }
                if let Some(request) = queue.pending.pop_front() {
                    queue.running = true;
                    break request;
                }
                queue = match signal.wait(queue) {
                    Ok(queue) => queue,
                    Err(_) => return,
                };
            }
        };
        let outcome = execute(request);
        if outcomes.send(outcome).is_err() {
            return;
        }
    }
}

fn execute(request: ApplyRequest) -> ApplyOutcome {
    let ApplyRequest {
        target,
        size,
        live_preview_only,
    } = request;
    match target {
        ApplyTarget::Pack {
            spec,
            files,
            chosen,
        } => ApplyOutcome {
            chosen_pack: chosen.then_some(spec),
            result: apply_downloaded_collection(spec, &files, size, live_preview_only)
                .map(|()| size),
        },
        ApplyTarget::SystemSize => ApplyOutcome {
            chosen_pack: None,
            result: apply_cursor_base_size(size),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{ApplyRequest, ApplyTarget, enqueue};
    use std::collections::VecDeque;

    fn size_request(size: u32, live_preview_only: bool) -> ApplyRequest {
        ApplyRequest {
            target: ApplyTarget::SystemSize,
            size,
            live_preview_only,
        }
    }

    #[test]
    fn a_drag_keeps_only_the_latest_preview_and_every_commit() {
        let mut pending = VecDeque::new();
        enqueue(&mut pending, size_request(40, true));
        enqueue(&mut pending, size_request(48, true));
        enqueue(&mut pending, size_request(56, false));
        enqueue(&mut pending, size_request(64, true));
        enqueue(&mut pending, size_request(72, true));
        enqueue(&mut pending, size_request(80, false));
        let sizes: Vec<_> = pending
            .iter()
            .map(|request| (request.size, request.live_preview_only))
            .collect();
        assert_eq!(sizes, vec![(56, false), (80, false)]);
    }
}
