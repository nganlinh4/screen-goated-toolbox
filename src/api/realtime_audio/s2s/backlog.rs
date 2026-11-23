//! Playback backlog belongs to one direct-speech run, even across overlay restarts.

use std::sync::{
    Arc, LazyLock, Mutex, Weak,
    atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct BacklogSnapshot {
    pub(crate) segments: u32,
    pub(crate) audio_ms: u32,
    pub(crate) delay_ms: u32,
    pub(crate) ready_ms: u32,
}

#[derive(Default)]
pub(super) struct S2sBacklogRun {
    state: Mutex<BacklogSnapshot>,
}

impl S2sBacklogRun {
    pub(super) fn lease(self: &Arc<Self>, audio_ms: usize) -> Arc<S2sBacklogLease> {
        let audio_ms = u32::try_from(audio_ms).unwrap_or(u32::MAX);
        let mut state = self.state.lock().unwrap();
        state.segments = state.segments.saturating_add(1);
        state.audio_ms = state.audio_ms.saturating_add(audio_ms);
        Arc::new(S2sBacklogLease {
            run: self.clone(),
            audio_ms,
        })
    }

    pub(super) fn set_delay(&self, ms: u32) {
        self.state.lock().unwrap().delay_ms = ms;
    }

    pub(super) fn set_ready(&self, ms: u32) {
        self.state.lock().unwrap().ready_ms = ms;
    }

    fn snapshot(&self) -> BacklogSnapshot {
        *self.state.lock().unwrap()
    }
}

pub(super) struct S2sBacklogLease {
    run: Arc<S2sBacklogRun>,
    audio_ms: u32,
}

impl Drop for S2sBacklogLease {
    fn drop(&mut self) {
        let mut state = self.run.state.lock().unwrap();
        state.segments = state.segments.saturating_sub(1);
        state.audio_ms = state.audio_ms.saturating_sub(self.audio_ms);
        if state.segments == 0 {
            state.delay_ms = 0;
            state.ready_ms = 0;
        }
    }
}

#[derive(Default)]
struct BacklogRegistry {
    current: Mutex<Option<(u64, u64, Weak<S2sBacklogRun>)>>,
}

impl BacklogRegistry {
    fn install(&self, session_id: u64, engine_revision: u64, run: &Arc<S2sBacklogRun>) {
        *self.current.lock().unwrap() = Some((session_id, engine_revision, Arc::downgrade(run)));
    }

    fn snapshot(&self, session_id: u64, engine_revision: u64) -> BacklogSnapshot {
        let current = self.current.lock().unwrap();
        let run = current.as_ref().and_then(|(owner, revision, run)| {
            if *owner == session_id && *revision == engine_revision {
                run.upgrade()
            } else {
                None
            }
        });
        run.map(|run| run.snapshot()).unwrap_or_default()
    }
}

static BACKLOG: LazyLock<BacklogRegistry> = LazyLock::new(BacklogRegistry::default);

pub(super) fn begin_run(
    session_id: u64,
    stop: &AtomicBool,
    engine_revision: u64,
) -> Arc<S2sBacklogRun> {
    let run = Arc::new(S2sBacklogRun::default());
    crate::overlay::realtime_webview::state::publish_if_engine_current(
        session_id,
        stop,
        engine_revision,
        || {
            BACKLOG.install(session_id, engine_revision, &run);
        },
    );
    run
}

pub(crate) fn current_backlog() -> BacklogSnapshot {
    let session_id =
        crate::overlay::realtime_webview::state::REALTIME_SESSION_ID.load(Ordering::SeqCst);
    let engine_revision = crate::overlay::realtime_webview::state::TRANSCRIPTION_ENGINE_REVISION
        .load(Ordering::SeqCst);
    let snapshot = BACKLOG.snapshot(session_id, engine_revision);
    if crate::overlay::realtime_webview::state::REALTIME_SESSION_ID.load(Ordering::SeqCst)
        == session_id
        && crate::overlay::realtime_webview::state::engine_is_current(engine_revision)
    {
        snapshot
    } else {
        BacklogSnapshot::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_leases_and_updates_cannot_change_new_run_metrics() {
        let registry = BacklogRegistry::default();
        let old = Arc::new(S2sBacklogRun::default());
        registry.install(1, 1, &old);
        let old_lease = old.lease(1_000);
        old.set_ready(700);
        let new = Arc::new(S2sBacklogRun::default());
        registry.install(2, 2, &new);
        let new_lease = new.lease(2_000);
        new.set_ready(1_500);
        new.set_delay(300);
        old.set_ready(900);
        old.set_delay(800);
        drop(old_lease);
        assert_eq!(
            registry.snapshot(2, 2),
            BacklogSnapshot {
                segments: 1,
                audio_ms: 2_000,
                ready_ms: 1_500,
                delay_ms: 300
            }
        );
        assert_eq!(registry.snapshot(1, 1), BacklogSnapshot::default());
        drop(new_lease);
        assert_eq!(registry.snapshot(2, 2), BacklogSnapshot::default());
    }

    #[test]
    fn replacement_within_same_session_owns_its_own_counters() {
        let registry = BacklogRegistry::default();
        let old = Arc::new(S2sBacklogRun::default());
        registry.install(7, 1, &old);
        let old_lease = old.lease(800);
        let new = Arc::new(S2sBacklogRun::default());
        assert_eq!(registry.snapshot(7, 2), BacklogSnapshot::default());
        registry.install(7, 2, &new);
        let new_lease = new.lease(1_200);
        new.set_delay(150);
        old.set_delay(900);
        drop(old_lease);
        assert_eq!(
            registry.snapshot(7, 2),
            BacklogSnapshot {
                segments: 1,
                audio_ms: 1_200,
                delay_ms: 150,
                ready_ms: 0,
            }
        );
        drop(new_lease);
        assert_eq!(registry.snapshot(7, 2), BacklogSnapshot::default());
    }
}
