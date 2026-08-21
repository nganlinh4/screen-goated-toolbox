use std::time::{Duration, Instant};

use anyhow::{Result, bail};

const OWNER_STOP_TIMEOUT: Duration = Duration::from_secs(30);
const OWNER_STOP_INTERVAL: Duration = Duration::from_millis(20);

/// Stops every host feature that can retain an audio model or runtime, then
/// keeps Screen Recorder from relaunching until the caller finishes removal.
pub(crate) fn stop_audio_owners() -> Result<impl Drop> {
    let recorder_guard = crate::overlay::screen_record::stop_for_component_removal()?;
    crate::overlay::tts_playground::stop_for_component_removal()?;
    crate::overlay::recording::compositor_cancel();
    crate::overlay::stop_realtime_overlay();
    crate::api::tts::TTS_MANAGER.stop();

    let deadline = Instant::now() + OWNER_STOP_TIMEOUT;
    while crate::overlay::is_recording_overlay_active()
        || crate::overlay::is_realtime_overlay_active()
    {
        if Instant::now() >= deadline {
            bail!("audio feature did not stop before component removal timed out");
        }
        std::thread::sleep(OWNER_STOP_INTERVAL);
    }
    Ok(recorder_guard)
}
