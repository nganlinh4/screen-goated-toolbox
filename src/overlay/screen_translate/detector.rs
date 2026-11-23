//! Persistent host client for the streaming Screen Translate worker.

mod client_slot;
pub(super) mod incremental;
mod process;
mod warmup;

pub(super) fn prepare(cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>) {
    incremental::prepare(cancelled);
}

pub(super) fn stop() {
    incremental::stop();
}
