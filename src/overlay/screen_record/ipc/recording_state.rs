use std::sync::atomic::{AtomicBool, Ordering};

use super::super::engine::IS_RECORDING;
use windows::Win32::Media::{timeBeginPeriod, timeEndPeriod};

static TIMER_RESOLUTION_ACTIVE: AtomicBool = AtomicBool::new(false);

pub(in crate::overlay::screen_record) struct RecordingStartClaim {
    armed: bool,
    timer_lease: bool,
}

impl RecordingStartClaim {
    pub(in crate::overlay::screen_record) fn claim(timer_lease: bool) -> Result<Self, String> {
        IS_RECORDING
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| "A recording is already active or starting".to_string())?;

        if timer_lease {
            if TIMER_RESOLUTION_ACTIVE
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
            {
                IS_RECORDING.store(false, Ordering::SeqCst);
                return Err("The recording timer lease is already active".to_string());
            }
            if unsafe { timeBeginPeriod(1) } != 0 {
                TIMER_RESOLUTION_ACTIVE.store(false, Ordering::SeqCst);
                IS_RECORDING.store(false, Ordering::SeqCst);
                return Err("Windows refused the requested recording timer resolution".to_string());
            }
        }

        Ok(Self {
            armed: true,
            timer_lease,
        })
    }

    pub(in crate::overlay::screen_record) fn commit(mut self) {
        self.armed = false;
    }
}

impl Drop for RecordingStartClaim {
    fn drop(&mut self) {
        if self.armed {
            IS_RECORDING.store(false, Ordering::SeqCst);
            if self.timer_lease {
                end_timer_resolution();
            }
        }
    }
}

pub(super) fn end_timer_resolution() {
    if TIMER_RESOLUTION_ACTIVE.swap(false, Ordering::SeqCst) {
        unsafe {
            timeEndPeriod(1);
        }
    }
}

pub(in crate::overlay::screen_record) fn release_recording_state() {
    end_timer_resolution();
    IS_RECORDING.store(false, Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_claim_is_single_flight() {
        release_recording_state();
        let first = RecordingStartClaim::claim(false).unwrap();
        assert!(RecordingStartClaim::claim(false).is_err());
        drop(first);
        assert!(RecordingStartClaim::claim(false).is_ok());
        release_recording_state();
    }
}
