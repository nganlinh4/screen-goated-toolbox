use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

#[derive(Default)]
pub(super) struct DeviceFailure {
    committed: AtomicU32,
    in_flight: AtomicU32,
    applied_reset: AtomicU64,
    generation: AtomicU64,
}

impl DeviceFailure {
    pub(super) fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    pub(super) fn set_padding(&self, frames: u32) {
        self.committed.store(frames, Ordering::Release);
    }

    pub(super) fn begin_submission(&self, frames: u32) {
        self.in_flight.store(frames, Ordering::Release);
    }

    pub(super) fn finish_submission(&self, padding: u32) {
        self.committed.store(padding, Ordering::Release);
        self.in_flight.store(0, Ordering::Release);
    }

    pub(super) fn reset_applied(&self, generation: u64) {
        self.committed.store(0, Ordering::Release);
        self.in_flight.store(0, Ordering::Release);
        self.applied_reset.store(generation, Ordering::Release);
    }

    /// Called while holding the playback deck lock, before exposing empty padding.
    pub(super) fn finish_stream(&self, failed: bool, requested_reset: u64) {
        let lost = self.committed.load(Ordering::Acquire) > 0
            || self.in_flight.load(Ordering::Acquire) > 0;
        if failed && lost && self.applied_reset.load(Ordering::Acquire) == requested_reset {
            self.generation.fetch_add(1, Ordering::AcqRel);
        }
        self.committed.store(0, Ordering::Release);
        self.in_flight.store(0, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_reports_lost_committed_and_in_flight_audio() {
        let state = DeviceFailure::default();
        state.set_padding(480);
        state.finish_stream(true, 0);
        assert_eq!(state.generation(), 1);
        state.begin_submission(100);
        state.finish_stream(true, 0);
        assert_eq!(state.generation(), 2);
        state.finish_stream(true, 0);
        assert_eq!(state.generation(), 2);
    }

    #[test]
    fn intentional_reset_does_not_fail_the_next_owner() {
        let state = DeviceFailure::default();
        state.set_padding(480);
        state.finish_stream(true, 1);
        assert_eq!(state.generation(), 0);
        state.reset_applied(1);
        state.set_padding(200);
        state.finish_stream(true, 1);
        assert_eq!(state.generation(), 1);
    }
}
