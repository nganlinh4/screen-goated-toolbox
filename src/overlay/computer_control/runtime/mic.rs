//! The session microphone owner — split out of `runtime.rs` for the file-size limit.
//! `use super::super::*` reaches the sibling CC modules (`overlay`); audio capture
//! lives in `crate::api::realtime_audio`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::super::overlay;

/// Owns the microphone for the whole session on a DEDICATED thread: builds the cpal stream, watches
/// for a default-input-device change, and rebuilds on its own. Keeping all cpal/WASAPI calls on this
/// one thread isolates their per-thread COM apartment from the session loop's TLS/UIA churn, so a
/// device switch can't trip RPC_E_CHANGED_MODE. Audio flows to the loop via the shared `buf`.
pub(super) fn mic_thread(buf: Arc<Mutex<Vec<i16>>>, pause: Arc<AtomicBool>, stop: Arc<AtomicBool>) {
    // Build the mic stream, retrying a few times (WASAPI transiently reports "device busy" mid-switch).
    let build = || -> anyhow::Result<cpal::Stream> {
        let mut attempt = 0;
        loop {
            match crate::api::realtime_audio::start_mic_capture(buf.clone(), stop.clone(), pause.clone())
            {
                Ok(s) => return Ok(s),
                Err(_) if attempt < 4 => {
                    attempt += 1;
                    overlay::push_log(format!("(audio device busy - retrying {attempt}/4)"));
                    std::thread::sleep(Duration::from_millis(500));
                }
                Err(e) => return Err(e),
            }
        }
    };
    let mut stream = build().map_err(|e| overlay::push_log(format!("(mic init failed: {e})"))).ok();
    let mut device = crate::api::realtime_audio::current_input_device_name();
    let mut last_check = Instant::now();
    while !stop.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(200));
        if last_check.elapsed() < Duration::from_secs(2) {
            continue;
        }
        last_check = Instant::now();
        let now = crate::api::realtime_audio::current_input_device_name();
        if now != device {
            overlay::push_log(format!(
                "(audio device changed -> {} - re-initializing mic)",
                now.as_deref().unwrap_or("none")
            ));
            device = now;
            drop(stream.take()); // release the OLD device before grabbing the new one
            std::thread::sleep(Duration::from_millis(300)); // let the switch settle
            stream = build()
                .map_err(|e| overlay::push_log(format!("(mic re-init failed: {e})")))
                .ok();
        }
    }
    drop(stream);
}
