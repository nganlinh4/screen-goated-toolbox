//! Session-bound automatic insertion. Provider polling never waits for editor I/O.

mod diagnostics;
mod editor;
mod handoff;
mod policy;
#[cfg(test)]
mod policy_tests;
mod routing;

use std::collections::VecDeque;
use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

use editor::{AppendTarget, BestEffortTarget, Editor};
use policy::{Event, Policy};

static GENERATION: AtomicU64 = AtomicU64::new(0);
static WORKERS: AtomicUsize = AtomicUsize::new(0);
static INPUT_LEASE: Mutex<()> = Mutex::new(());
const MAX_EVENTS: usize = 128;
const MAX_TEXT_BYTES: usize = 128 * 1024;

#[derive(Default)]
struct Mailbox {
    received: u64,
    queue: VecDeque<Event>,
    accepting: bool,
    ready: bool,
    done: bool,
    overflow: bool,
    draining: bool,
    expired: bool,
}

#[derive(Default)]
struct Shared {
    generation: u64,
    state: Mutex<Mailbox>,
    changed: Condvar,
}

/// One owner per preset audio session; disabled auto-paste allocates no worker.
pub(crate) struct StreamingAutoPaste {
    shared: Option<Arc<Shared>>,
}

impl StreamingAutoPaste {
    pub(crate) fn new(enabled: bool, abort: Arc<AtomicBool>) -> Self {
        if !enabled {
            return Self { shared: None };
        }
        let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
        if WORKERS
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| {
                (n < 2).then_some(n + 1)
            })
            .is_err()
        {
            crate::log_info!("[AutoPaste] suspended reason=worker_capacity");
            return Self { shared: None };
        }
        let foreground = unsafe { GetForegroundWindow() }.0 as usize;
        let shared = Arc::new(Shared {
            generation,
            ..Default::default()
        });
        shared.state.lock().unwrap().accepting = true;
        let worker_shared = shared.clone();
        if std::thread::Builder::new()
            .name("preset-auto-paste".into())
            .spawn(move || {
                let _completion = Completion(worker_shared.clone());
                let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
                if initialized {
                    run_worker(
                        &worker_shared,
                        generation,
                        abort,
                        HWND(foreground as *mut _),
                    );
                    unsafe {
                        CoUninitialize();
                    }
                }
            })
            .is_err()
        {
            WORKERS.fetch_sub(1, Ordering::SeqCst);
            return Self { shared: None };
        }
        // Refuse late target capture; do not bind to a field selected much later.
        let state = shared.state.lock().unwrap();
        let (mut state, _) = shared
            .changed
            .wait_timeout_while(state, Duration::from_secs(2), |s| !s.ready && !s.done)
            .unwrap();
        if !state.ready {
            state.accepting = false;
            state.overflow = true;
            crate::log_info!("[AutoPaste] suspended reason=target_capture_unavailable");
        }
        drop(state);
        Self {
            shared: Some(shared),
        }
    }

    pub(crate) fn interim(&self, text: &str) {
        self.enqueue(Event::Interim(text.to_owned()));
    }
    pub(crate) fn final_text(&self, text: &str) {
        self.enqueue(Event::Final(text.to_owned()));
    }

    pub(crate) fn begin_drain(&self) {
        if let Some(shared) = &self.shared {
            let mut state = shared.state.lock().unwrap();
            state.draining = true;
            state
                .queue
                .retain(|event| !matches!(event, Event::Interim(_)));
        }
    }

    fn enqueue(&self, event: Event) {
        let Some(shared) = &self.shared else {
            return;
        };
        let mut state = shared.state.lock().unwrap();
        if !state.accepting || state.done {
            return;
        }
        if state.draining && matches!(event, Event::Interim(_)) {
            return;
        }
        state.received += 1;
        diagnostics::received(shared.generation, state.received, &event);
        crate::log_info!(
            "[AutoPaste] session={} received={} kind={} chars={} queued={}",
            shared.generation,
            state.received,
            event.kind(),
            event.chars(),
            state.queue.len()
        );
        enqueue(&mut state, event);
        shared.changed.notify_one();
    }

    pub(crate) fn finish(&self) {
        self.close();
        if let Some(shared) = &self.shared {
            let state = shared.state.lock().unwrap();
            let (mut state, _) = shared
                .changed
                .wait_timeout_while(state, Duration::from_secs(2), |s| !s.done)
                .unwrap();
            if !state.done {
                state.expired = true;
                state.queue.clear();
                shared.changed.notify_all();
                crate::log_info!("[AutoPaste] suspended reason=finish_deadline");
            }
        }
    }

    fn close(&self) {
        if let Some(shared) = &self.shared {
            let mut state = shared.state.lock().unwrap();
            if state.accepting {
                state.accepting = false;
                state.queue.push_back(Event::Finish);
                shared.changed.notify_one();
            }
        }
    }
}

impl Drop for StreamingAutoPaste {
    fn drop(&mut self) {
        self.close();
    }
}

struct Completion(Arc<Shared>);
impl Drop for Completion {
    fn drop(&mut self) {
        let mut state = self.0.state.lock().unwrap_or_else(|e| e.into_inner());
        state.done = true;
        state.accepting = false;
        state.queue.clear();
        self.0.changed.notify_all();
        WORKERS.fetch_sub(1, Ordering::SeqCst);
    }
}

enum Target {
    Replace(Editor),
    Append(AppendTarget),
    BestEffort(BestEffortTarget),
}
impl Target {
    fn backend(&self) -> &'static str {
        match self {
            Self::Replace(_) => "verified_range",
            Self::Append(_) => "verified_append",
            Self::BestEffort(_) => "unverified_keyboard",
        }
    }
    fn capture(foreground: HWND) -> anyhow::Result<Self> {
        Editor::capture(foreground)
            .map(Self::Replace)
            .or_else(|error| {
                if error.is::<editor::UnsafeCapture>() {
                    return Err(error);
                }
                crate::log_info!(
                    "[AutoPaste] fallback=keyboard_revisions_best_effort detail={error:#}"
                );
                BestEffortTarget::capture(foreground).map(Self::BestEffort)
            })
            .or_else(|error| {
                if error.is::<editor::UnsafeCapture>() {
                    return Err(error);
                }
                AppendTarget::capture(foreground).map(Self::Append)
            })
    }
    fn check(&self) -> anyhow::Result<()> {
        match self {
            Self::Replace(e) => e.check(),
            Self::Append(e) => e.check(),
            Self::BestEffort(e) => e.check(),
        }
    }
    fn replace(&mut self, old: &str, new: &str, allowed: &dyn Fn() -> bool) -> anyhow::Result<()> {
        match self {
            Self::Replace(e) => e.replace(old, new, allowed),
            Self::Append(e) => {
                anyhow::ensure!(old.is_empty());
                e.append(new, allowed)
            }
            Self::BestEffort(e) => e.replace(old, new, allowed),
        }
    }
}

fn run_worker(shared: &Shared, generation: u64, abort: Arc<AtomicBool>, foreground: HWND) {
    let mut target = {
        let _lease = INPUT_LEASE.lock().unwrap();
        if abort.load(Ordering::SeqCst) || GENERATION.load(Ordering::SeqCst) != generation {
            return;
        }
        match Target::capture(foreground) {
            Ok(target) => target,
            Err(_) => {
                crate::log_info!("[AutoPaste] suspended reason=no_safe_target");
                return;
            }
        }
    };
    let mut policy = Policy::new(matches!(target, Target::Replace(_) | Target::BestEffort(_)));
    {
        let mut state = shared.state.lock().unwrap();
        if state.overflow {
            return;
        }
        state.ready = true;
        shared.changed.notify_all();
    }
    crate::log_info!(
        "[AutoPaste] session={generation} ready backend={} replaceable={}",
        target.backend(),
        matches!(target, Target::Replace(_) | Target::BestEffort(_))
    );
    let mut last_interim = Instant::now() - Duration::from_secs(1);
    let mut handoff = false;
    let mut routing = routing::Routing::default();
    let mut settled_target = (0usize, Instant::now());
    let mut dispatched = 0u64;
    loop {
        if handoff {
            std::thread::sleep(Duration::from_millis(10));
        }
        let mut state = shared.state.lock().unwrap();
        if state.queue.is_empty() && !state.overflow && !abort.load(Ordering::SeqCst) {
            state = shared
                .changed
                .wait_timeout(state, Duration::from_millis(50))
                .unwrap()
                .0;
        }
        let cancelled = abort.load(Ordering::SeqCst) || state.overflow;
        if state.expired {
            break;
        }
        // Retain queued speech through a shortcut, without suspending ownership.
        // Ctrl+Backspace is not a character deletion; deliver after release.
        if !cancelled && editor::shortcut_held() {
            drop(state);
            std::thread::sleep(Duration::from_millis(10));
            continue;
        }
        let event = if cancelled {
            Some(Event::Finish)
        } else {
            state.queue.pop_front()
        };
        drop(state);
        let _lease = INPUT_LEASE.lock().unwrap();
        if GENERATION.load(Ordering::SeqCst) != generation {
            break;
        }
        #[cfg(test)]
        if editor::require_test_target(unsafe { GetForegroundWindow() }).is_err() {
            crate::log_info!("[AutoPaste] UI test stopped: foreground left the test process");
            break;
        }
        if !handoff && let Err(error) = target.check() {
            policy.suspend();
            handoff = true;
            routing.detach();
            settled_target = (0, Instant::now());
            crate::log_info!("[AutoPaste] handoff=waiting detail={error:#}");
        }
        if handoff {
            if cancelled || matches!(event, Some(Event::Finish)) {
                break;
            }
            let foreground = unsafe { GetForegroundWindow() };
            if foreground.0 as usize != settled_target.0 {
                settled_target = (foreground.0 as usize, Instant::now());
            }
            if settled_target.1.elapsed() < Duration::from_millis(250) {
                defer_event(shared, event);
                continue;
            }
            match Target::capture(foreground) {
                Ok(next) => {
                    if !handoff::ready(&next, shared, generation, &abort) {
                        defer_event(shared, event);
                        settled_target = (0, Instant::now());
                        crate::log_info!(
                            "[AutoPaste] session={generation} handoff=candidate_changed queued_speech_retained=true"
                        );
                        continue;
                    }
                    target = next;
                    policy =
                        Policy::new(matches!(target, Target::Replace(_) | Target::BestEffort(_)));
                    handoff = false;
                    crate::log_info!(
                        "[AutoPaste] session={generation} handoff=resumed backend={}",
                        target.backend()
                    );
                }
                _ => {
                    defer_event(shared, event);
                    settled_target.1 = Instant::now();
                    continue;
                }
            }
        }
        let Some(event) = event else {
            continue;
        };
        if matches!(event, Event::Interim(_)) && shared.state.lock().unwrap().draining {
            continue;
        }
        if matches!(event, Event::Interim(_)) && last_interim.elapsed() < Duration::from_millis(100)
        {
            // Keep the newest hypothesis, but never reorder it across a final.
            let mut state = shared.state.lock().unwrap();
            if !matches!(
                state.queue.front(),
                Some(Event::Interim(_) | Event::Final(_) | Event::Finish)
            ) {
                state.queue.push_front(event);
            }
            drop(state);
            drop(_lease);
            std::thread::sleep(Duration::from_millis(10));
            continue;
        }
        if abort.load(Ordering::SeqCst) && !matches!(event, Event::Finish) {
            continue;
        }
        let allowed = || {
            let state = shared.state.lock().unwrap();
            permits(
                &state,
                &event,
                GENERATION.load(Ordering::SeqCst) == generation,
                abort.load(Ordering::SeqCst),
            )
        };
        dispatched += 1;
        let (routed_event, cut) = routing.project(&event);
        if cut != 0 {
            crate::log_info!(
                "[AutoPaste] session={generation} dispatch={dispatched} handoff=routed source_chars={} destination_chars={}",
                event.chars(),
                routed_event.chars()
            );
        }
        let replacement = policy.plan(&routed_event);
        if let Some(replacement) = &replacement {
            diagnostics::dispatched(
                generation,
                dispatched,
                event.kind(),
                &replacement.old,
                &replacement.new,
            );
        }
        crate::log_info!(
            "[AutoPaste] session={generation} dispatch={dispatched} kind={} backend={} old_chars={} new_chars={} mutation={}",
            event.kind(),
            target.backend(),
            replacement.as_ref().map_or(0, |r| r.old.chars().count()),
            replacement.as_ref().map_or(0, |r| r.new.chars().count()),
            replacement.is_some()
        );
        if let Some(replacement) = replacement
            && let Err(error) = target.replace(&replacement.old, &replacement.new, &allowed)
        {
            policy.suspend();
            handoff = true;
            settled_target = (0, Instant::now());
            if error.is::<editor::BeforeMutation>() && !matches!(event, Event::Finish) {
                routing.detach();
                defer_event(shared, Some(event));
                crate::log_info!(
                    "[AutoPaste] session={generation} dispatch={dispatched} handoff=retry_unwritten detail={error:#}"
                );
                continue;
            }
            crate::log_info!(
                "[AutoPaste] session={generation} dispatch={dispatched} suspended reason=unverified_mutation detail={error:#}"
            );
            // Consume the attempted source span for routing, without claiming
            // delivery. Never replay an uncertain effect, but keep future speech.
            routing.accept(&event, cut, true);
            routing.detach();
            if matches!(event, Event::Finish) {
                break;
            }
            continue;
        }
        policy.accept(&routed_event);
        routing.accept(
            &event,
            cut,
            matches!(target, Target::Replace(_) | Target::BestEffort(_)),
        );
        crate::log_info!(
            "[AutoPaste] session={generation} dispatch={dispatched} accepted backend={}",
            target.backend()
        );
        if matches!(event, Event::Finish) {
            break;
        }
        if matches!(event, Event::Interim(_)) {
            last_interim = Instant::now();
        }
    }
}

fn permits(state: &Mailbox, event: &Event, current: bool, aborted: bool) -> bool {
    !state.expired
        && (!state.overflow || matches!(event, Event::Finish))
        && !(state.draining && matches!(event, Event::Interim(_)))
        && current
        && (matches!(event, Event::Finish) || !aborted)
}

fn enqueue(state: &mut Mailbox, event: Event) {
    if matches!(event, Event::Interim(_)) && matches!(state.queue.back(), Some(Event::Interim(_))) {
        state.queue.pop_back();
    }
    state.queue.push_back(event);
    enforce_capacity(state);
}

fn defer_event(shared: &Shared, event: Option<Event>) {
    let Some(event) = event else { return };
    let mut state = shared.state.lock().unwrap();
    if matches!(event, Event::Interim(_))
        && (state.draining
            || matches!(
                state.queue.front(),
                Some(Event::Interim(_) | Event::Final(_))
            ))
    {
        return;
    }
    state.queue.push_front(event);
    enforce_capacity(&mut state);
}

fn enforce_capacity(state: &mut Mailbox) {
    let bytes: usize = state
        .queue
        .iter()
        .map(|e| match e {
            Event::Interim(s) | Event::Final(s) => s.len(),
            Event::Finish => 0,
        })
        .sum();
    if state.queue.len() > MAX_EVENTS || bytes > MAX_TEXT_BYTES {
        state.queue.clear();
        state.overflow = true;
        state.accepting = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mailbox_coalesces_only_adjacent_interims_and_preserves_final_order() {
        let mut state = Mailbox::default();
        for event in [
            Event::Interim("old".into()),
            Event::Interim("new".into()),
            Event::Final("one".into()),
            Event::Interim("next".into()),
            Event::Final("two".into()),
        ] {
            enqueue(&mut state, event);
        }
        assert_eq!(
            state.queue.into_iter().collect::<Vec<_>>(),
            vec![
                Event::Interim("new".into()),
                Event::Final("one".into()),
                Event::Interim("next".into()),
                Event::Final("two".into())
            ]
        );
    }

    #[test]
    fn mailbox_overflow_discards_work_instead_of_growing_or_replaying() {
        let mut state = Mailbox::default();
        enqueue(&mut state, Event::Final("x".repeat(MAX_TEXT_BYTES + 1)));
        assert!(state.overflow && state.queue.is_empty() && !state.accepting);
    }

    #[test]
    fn disabled_auto_paste_never_claims_target_or_starts_worker() {
        let writer = StreamingAutoPaste::new(false, Arc::new(AtomicBool::new(false)));
        writer.interim("preview");
        writer.final_text("final");
        writer.finish();
        assert!(writer.shared.is_none());
    }

    #[test]
    fn dispatch_permit_rechecks_drain_abort_expiration_and_generation() {
        let interim = Event::Interim("draft".into());
        let final_text = Event::Final("final".into());
        let mut state = Mailbox::default();
        assert!(permits(&state, &interim, true, false));
        state.draining = true;
        assert!(!permits(&state, &interim, true, false));
        assert!(permits(&state, &final_text, true, false));
        assert!(!permits(&state, &final_text, true, true));
        assert!(permits(&state, &Event::Finish, true, true));
        assert!(!permits(&state, &Event::Finish, false, false));
        state.overflow = true;
        assert!(!permits(&state, &final_text, true, false));
        assert!(permits(&state, &Event::Finish, true, false));
        state.expired = true;
        assert!(!permits(&state, &Event::Finish, true, true));
    }
}
