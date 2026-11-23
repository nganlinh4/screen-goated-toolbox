//! Fault injection is available only when an opted-in debug host forwards its capability.
use std::sync::atomic::{AtomicBool, Ordering};

static FAIL_CAPTURE_SETUP: AtomicBool = AtomicBool::new(false);

fn enabled() -> bool {
    std::env::var("SGT_RECORDER_DIAGNOSTICS").as_deref() == Ok("1")
}

pub(super) fn fail_capture_setup_if_armed() -> Result<(), String> {
    if enabled() && FAIL_CAPTURE_SETUP.swap(false, Ordering::SeqCst) {
        Err("Diagnostic capture setup failure after GPU initialization".into())
    } else {
        Ok(())
    }
}

pub(super) fn command(command: &str) -> Result<serde_json::Value, String> {
    if !enabled() {
        return Err("Recorder diagnostic capability is not enabled".into());
    }
    match command {
        "diagnostic_arm_capture_setup_failure" => {
            if super::engine::IS_RECORDING.load(Ordering::SeqCst) {
                return Err("Capture is already active".into());
            }
            FAIL_CAPTURE_SETUP.store(true, Ordering::SeqCst);
            Ok(serde_json::Value::Null)
        }
        "diagnostic_gpu_device_loss" => super::gpu_export::diagnostic_device_loss(),
        "diagnostic_snapshot" => Ok(serde_json::json!({
            "recording": super::engine::IS_RECORDING.load(Ordering::SeqCst),
            "encoder": super::engine::ENCODER_ACTIVE.load(Ordering::SeqCst),
            "audioFinished": super::engine::AUDIO_ENCODING_FINISHED.load(Ordering::SeqCst),
            "micFinished": super::engine::MIC_AUDIO_ENCODING_FINISHED.load(Ordering::SeqCst),
            "webcamFinished": super::engine::WEBCAM_ENCODING_FINISHED.load(Ordering::SeqCst),
            "stopAudio": super::engine::SHOULD_STOP_AUDIO.load(Ordering::SeqCst),
            "gpu": super::gpu_export::diagnostic_context(),
        })),
        _ => Err("Unknown recorder diagnostic command".into()),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn diagnostics_are_not_enabled_by_default() {
        assert!(!super::enabled());
        assert!(super::command("diagnostic_gpu_device_loss").is_err());
    }
}
