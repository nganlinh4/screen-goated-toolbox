//! Nonblocking fresh-frame requests and the session-owned capture cache.

use std::sync::Condvar;
use std::thread::JoinHandle;

use super::*;

const MAX_FRAME_AGE: Duration = Duration::from_secs(2);
const INPUT_REQUEST_WINDOW: Duration = Duration::from_secs(5);

#[derive(Clone, Copy)]
struct FrameRequest {
    at: Instant,
    trigger: &'static str,
}

pub(super) struct FrameDelivery {
    request: Option<FrameRequest>,
    minimum_capture: Option<Instant>,
    input_started: Option<Instant>,
    last_sent: Instant,
    connection_generation: u32,
}

impl Default for FrameDelivery {
    fn default() -> Self {
        Self {
            request: None,
            minimum_capture: None,
            input_started: None,
            last_sent: Instant::now(),
            connection_generation: 0,
        }
    }
}

impl FrameDelivery {
    pub(super) fn request(&mut self, trigger: &'static str, now: Instant) {
        self.minimum_capture = Some(now);
        self.request = Some(FrameRequest { at: now, trigger });
    }

    pub(super) fn begin_input(&mut self) {
        let now = Instant::now();
        self.input_started = Some(now);
        self.request("user_input", now);
    }

    pub(super) fn confirm_input(&mut self) -> bool {
        let needs_request = self.input_started.is_none();
        if needs_request {
            self.begin_input();
        }
        self.input_started = None;
        needs_request
    }

    pub(super) fn pending(&self) -> bool {
        self.request.is_some()
    }

    pub(super) fn confirm_continuation(&mut self) {
        self.input_started = None;
    }

    pub(super) fn defer_periodic(&mut self) {
        self.last_sent = Instant::now();
        if let Some(request) = &mut self.request {
            request.at = self.last_sent;
            self.minimum_capture = Some(self.last_sent);
        }
    }

    pub(super) fn accepts_action_source(&self) -> bool {
        !self.pending() && self.input_started.is_none()
    }

    pub(super) fn action_settled(&mut self, same_turn: bool) -> bool {
        if same_turn && self.accepts_action_source() {
            return true;
        }
        self.request("post_action_refresh", Instant::now());
        false
    }

    fn observe_connection(&mut self, generation: u32, now: Instant) {
        if self.connection_generation != generation {
            self.connection_generation = generation;
            self.last_sent = now;
            self.minimum_capture = Some(now);
            if let Some(request) = &mut self.request {
                request.at = now;
            }
        }
    }

    fn accepts(&self, captured_at: Instant, now: Instant) -> bool {
        now.saturating_duration_since(captured_at) <= MAX_FRAME_AGE
            && self
                .minimum_capture
                .is_none_or(|minimum| captured_at >= minimum)
    }

    fn trigger(&mut self, state: &Activity, now: Instant) -> Option<&'static str> {
        if !state.uncommitted_audio
            && self
                .input_started
                .is_some_and(|started| now.duration_since(started) >= INPUT_REQUEST_WINDOW)
        {
            self.input_started = None;
        }
        if let Some(request) = self.request {
            if state.active
                || state.uncommitted_audio
                || (self.input_started.is_some()
                    && now.duration_since(request.at) < INPUT_REQUEST_WINDOW)
            {
                return Some(request.trigger);
            }
            self.request = None;
        }
        (!state.action_pending
            && !state.playing
            && !state.awaiting
            && state.engaged
            && now.duration_since(self.last_sent) >= FRAME_INTERVAL)
            .then_some("periodic")
    }
}

pub(super) struct Activity {
    pub(super) active: bool,
    pub(super) action_pending: bool,
    pub(super) playing: bool,
    pub(super) awaiting: bool,
    pub(super) engaged: bool,
    pub(super) recent_voice: bool,
    pub(super) voiced: bool,
    pub(super) uncommitted_audio: bool,
}

impl Activity {
    pub(super) fn current(
        state: &Reader,
        playing: bool,
        voiced: bool,
        last_voice: Instant,
        uncommitted_audio: bool,
    ) -> Self {
        Self {
            active: state.active && !state.terminal_drain,
            action_pending: state.pending.id.is_some(),
            playing,
            awaiting: state.awaiting,
            engaged: (state.active && !state.terminal_drain)
                || (uncommitted_audio && last_voice.elapsed() < Duration::from_secs(3)),
            recent_voice: last_voice.elapsed() < Duration::from_secs(5),
            voiced,
            uncommitted_audio,
        }
    }
}

struct CacheState {
    frame: Option<Arc<uia_task::SnapshotFrame>>,
    enabled: bool,
    request: Option<Instant>,
}

struct Shared {
    state: Mutex<CacheState>,
    wake: Condvar,
}

pub(super) struct FrameCache {
    shared: Arc<Shared>,
}

impl FrameCache {
    pub(super) fn start(
        initial: Option<uia_task::SnapshotFrame>,
        target: Option<String>,
        stop: Arc<AtomicBool>,
    ) -> (Self, JoinHandle<()>) {
        let cache = Self::new(initial);
        let shared = Arc::clone(&cache.shared);
        let worker = std::thread::spawn(super::super::session_retention::inherit(move || {
            capture_loop(
                shared,
                stop,
                || uia_task::snapshot(target.as_deref()),
                target.as_deref(),
            );
        }));
        (cache, worker)
    }

    fn new(initial: Option<uia_task::SnapshotFrame>) -> Self {
        Self {
            shared: Arc::new(Shared {
                state: Mutex::new(CacheState {
                    frame: initial.map(Arc::new),
                    enabled: true,
                    request: None,
                }),
                wake: Condvar::new(),
            }),
        }
    }

    pub(super) fn send_due(
        &self,
        socket: &mut Sock,
        state: &mut Reader,
        activity: Activity,
    ) -> anyhow::Result<()> {
        self.deliver_due(state, activity, |frame, trigger| {
            frames::send_snapshot(socket, frame, trigger)
        })
    }

    fn deliver_due(
        &self,
        state: &mut Reader,
        activity: Activity,
        deliver: impl FnOnce(&uia_task::SnapshotFrame, &str) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        let now = Instant::now();
        state
            .frame_delivery
            .observe_connection(state.connection_generation, now);
        let trigger = state.frame_delivery.trigger(&activity, now);
        let request = state.frame_delivery.request.map(|request| request.at);
        let enabled = state.frame_delivery.pending()
            || frames::capture_cache_needed(
                activity.active,
                false,
                activity.recent_voice,
                activity.action_pending,
                activity.voiced,
            );
        let frame = {
            let mut cache = self.shared.state.lock().unwrap();
            let wake = enabled != cache.enabled || (request.is_some() && request != cache.request);
            cache.enabled = enabled;
            cache.request = request;
            if wake {
                self.shared.wake.notify_one();
            }
            trigger
                .and_then(|_| cache.frame.as_ref())
                .filter(|frame| state.frame_delivery.accepts(frame.captured_at, now))
                .cloned()
        };
        if let (Some(frame), Some(trigger)) = (frame, trigger) {
            deliver(&frame, trigger)?;
            state.frame_delivery.request = None;
            state.frame_delivery.last_sent = Instant::now();
            state.memory.snapshot(
                &frame,
                activity.active,
                activity.uncommitted_audio && !activity.playing,
            );
            state.source_frame = Some(frame.source.clone());
        }
        Ok(())
    }
}

fn capture_loop(
    shared: Arc<Shared>,
    stop: Arc<AtomicBool>,
    mut capture: impl FnMut() -> anyhow::Result<uia_task::SnapshotFrame>,
    target: Option<&str>,
) {
    let mut failure_gate = frames::CaptureFailureGate::new();
    let mut cache = shared.state.lock().unwrap();
    while !stop.load(Ordering::SeqCst) {
        if !cache.enabled {
            failure_gate.on_success();
            cache = shared
                .wake
                .wait_timeout(cache, Duration::from_millis(120))
                .unwrap()
                .0;
            continue;
        }
        let request = cache.request;
        drop(cache);
        let result = capture().and_then(|frame| {
            anyhow::ensure!(
                frame.captured_at.elapsed() <= MAX_FRAME_AGE,
                "captured frame exceeded its delivery age bound"
            );
            Ok(frame)
        });
        if stop.load(Ordering::SeqCst) {
            break;
        }
        let frame = match result {
            Ok(frame) => {
                failure_gate.on_success();
                Some(Arc::new(frame))
            }
            Err(error) => {
                if failure_gate.on_failure(Instant::now()) {
                    frames::capture_failed("background_cache", target, &error);
                }
                None
            }
        };
        cache = shared.state.lock().unwrap();
        if let Some(frame) = frame {
            cache.frame = Some(frame);
        }
        let deadline = Instant::now() + CAPTURE_CACHE_INTERVAL;
        while cache.enabled && cache.request == request && !stop.load(Ordering::SeqCst) {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            cache = shared.wake.wait_timeout(cache, remaining).unwrap().0;
        }
    }
}

#[cfg(test)]
mod tests;
