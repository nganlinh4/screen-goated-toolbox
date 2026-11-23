use std::sync::atomic::{AtomicBool, Ordering};

use super::super::engine::IS_RECORDING;
use windows::Win32::Media::{timeBeginPeriod, timeEndPeriod};

static TIMER_RESOLUTION_ACTIVE: AtomicBool = AtomicBool::new(false);
static COMPATIBILITY_SIDECARS_OWNED: AtomicBool = AtomicBool::new(false);

pub(in crate::overlay::screen_record) fn own_compatibility_sidecars() {
    COMPATIBILITY_SIDECARS_OWNED.store(true, Ordering::SeqCst);
}

fn sidecars_available(owned: &AtomicBool, finished: [&AtomicBool; 3]) -> bool {
    if !owned.load(Ordering::SeqCst) {
        return true;
    }
    if finished.iter().any(|signal| !signal.load(Ordering::SeqCst)) {
        return false;
    }
    owned.store(false, Ordering::SeqCst);
    true
}

pub(in crate::overlay::screen_record) struct RecordingStartClaim {
    armed: bool,
    timer_lease: bool,
}

impl RecordingStartClaim {
    pub(in crate::overlay::screen_record) fn claim(timer_lease: bool) -> Result<Self, String> {
        IS_RECORDING
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| "A recording is already active or starting".to_string())?;

        use super::super::engine::{
            AUDIO_ENCODING_FINISHED, MIC_AUDIO_ENCODING_FINISHED, WEBCAM_ENCODING_FINISHED,
        };
        if !sidecars_available(
            &COMPATIBILITY_SIDECARS_OWNED,
            [
                &**AUDIO_ENCODING_FINISHED,
                &**MIC_AUDIO_ENCODING_FINISHED,
                &**WEBCAM_ENCODING_FINISHED,
            ],
        ) {
            IS_RECORDING.store(false, Ordering::SeqCst);
            return Err(
                "A previous recording is still finishing its audio or webcam capture".to_string(),
            );
        }

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

/// Serializes tests that change the process-wide recording claim.
#[cfg(test)]
pub(super) static CLAIM_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_claim_is_single_flight() {
        let _guard = CLAIM_TEST_LOCK.lock().unwrap();
        release_recording_state();
        let first = RecordingStartClaim::claim(false).unwrap();
        assert!(RecordingStartClaim::claim(false).is_err());
        drop(first);
        assert!(RecordingStartClaim::claim(false).is_ok());
        release_recording_state();
    }

    #[test]
    fn aborted_sidecar_quarantines_shared_signals_until_old_writer_finishes() {
        let owned = AtomicBool::new(true);
        let audio = AtomicBool::new(false);
        let mic = AtomicBool::new(true);
        let webcam = AtomicBool::new(true);
        let finished = [&audio, &mic, &webcam];
        assert!(!sidecars_available(&owned, finished));
        assert!(owned.load(Ordering::SeqCst));
        audio.store(true, Ordering::SeqCst);
        assert!(sidecars_available(&owned, finished));
        assert!(!owned.load(Ordering::SeqCst));
    }

    #[test]
    fn new_recording_cannot_reuse_signals_until_aborted_writer_exits() {
        use super::super::super::engine::{
            AUDIO_ENCODING_FINISHED, MIC_AUDIO_ENCODING_FINISHED, WEBCAM_ENCODING_FINISHED,
        };
        let _guard = CLAIM_TEST_LOCK.lock().unwrap();
        release_recording_state();
        AUDIO_ENCODING_FINISHED.store(false, Ordering::SeqCst);
        MIC_AUDIO_ENCODING_FINISHED.store(true, Ordering::SeqCst);
        WEBCAM_ENCODING_FINISHED.store(true, Ordering::SeqCst);
        own_compatibility_sidecars();
        let (release, released) = std::sync::mpsc::channel();
        let writer = std::thread::spawn(move || {
            released.recv().unwrap();
            AUDIO_ENCODING_FINISHED.store(true, Ordering::SeqCst);
        });
        assert!(RecordingStartClaim::claim(false).is_err());
        release.send(()).unwrap();
        writer.join().unwrap();
        let claim = RecordingStartClaim::claim(false).unwrap();
        drop(claim);
        release_recording_state();
    }
}
