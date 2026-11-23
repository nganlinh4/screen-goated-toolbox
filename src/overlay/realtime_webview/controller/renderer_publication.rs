//! Admit deferred renderer actions only for their original session and engine.
use super::*;

pub(super) fn publish_if_renderer_engine_current(
    session: u64,
    stop: &std::sync::atomic::AtomicBool,
    engine: u64,
    generation: u64,
    effect: impl FnOnce(),
) -> bool {
    let mut applied = false;
    publish_if_engine_current(session, stop, engine, || {
        if super::super::supervisor::is_current_generation(generation) {
            effect();
            applied = true;
        }
    });
    applied
}
