//! The session microphone owner — split out of `runtime.rs` for the file-size limit.
//! `use super::super::*` reaches the sibling CC modules (`overlay`); audio capture
//! lives in `crate::api::realtime_audio`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::super::overlay;
use super::super::telemetry::{self, Privacy};

pub(super) fn should_upload(sample_count: usize, muted: bool) -> bool {
    sample_count > 0 && !muted
}

pub(super) fn rms(samples: &[i16]) -> f64 {
    if samples.is_empty() {
        0.0
    } else {
        (samples.iter().map(|&s| (s as f64).powi(2)).sum::<f64>() / samples.len() as f64).sqrt()
    }
}

pub(super) struct MicUplinkWindow {
    started: Instant,
    chunks: u64,
    samples: u64,
    voiced_chunks: u64,
    peak_rms: f64,
}

impl MicUplinkWindow {
    pub(super) fn new() -> Self {
        Self {
            started: Instant::now(),
            chunks: 0,
            samples: 0,
            voiced_chunks: 0,
            peak_rms: 0.0,
        }
    }

    pub(super) fn record(&mut self, samples: usize, voiced: bool, rms: f64) {
        self.chunks += 1;
        self.samples += samples as u64;
        self.voiced_chunks += u64::from(voiced);
        self.peak_rms = self.peak_rms.max(rms);
    }

    pub(super) fn flush_if_due(&mut self, muted: bool) {
        if self.started.elapsed() < Duration::from_secs(5) {
            return;
        }
        if self.voiced_chunks > 0 {
            telemetry::human(
                "cc-mic",
                format!(
                    "uplink ok: {} chunks / {} samples, {} voiced, peak rms {:.0}",
                    self.chunks, self.samples, self.voiced_chunks, self.peak_rms
                ),
            );
        }
        telemetry::event(
            "mic_audio_uplink",
            "speech",
            Privacy::Safe,
            serde_json::json!({
                "duration_ms": self.started.elapsed().as_millis(),
                "chunks_sent": self.chunks,
                "samples_sent": self.samples,
                "voiced_chunks": self.voiced_chunks,
                "peak_rms": self.peak_rms,
                "muted": muted,
            }),
        );
        *self = Self::new();
    }
}

/// Owns the microphone for the whole session on a DEDICATED thread: builds the cpal stream, watches
/// for a default-input-device change, and rebuilds on its own. Keeping all cpal/WASAPI calls on this
/// one thread isolates their per-thread COM apartment from the session loop's TLS/UIA churn, so a
/// device switch can't trip RPC_E_CHANGED_MODE. Audio flows to the loop via the shared `buf`.
pub(super) fn mic_thread(buf: Arc<Mutex<Vec<i16>>>, pause: Arc<AtomicBool>, stop: Arc<AtomicBool>) {
    // Build the mic stream, retrying a few times (WASAPI transiently reports "device busy" mid-switch).
    let build = || -> anyhow::Result<cpal::Stream> {
        let mut attempt = 0;
        loop {
            match crate::api::realtime_audio::start_mic_capture(
                buf.clone(),
                stop.clone(),
                pause.clone(),
            ) {
                Ok(s) => return Ok(s),
                Err(error) if attempt < 4 && is_transient_device_error(&error) => {
                    attempt += 1;
                    overlay::push_log(format!("(transient mic error - retrying {attempt}/4)"));
                    telemetry::event(
                        "mic_stream_retry",
                        "speech",
                        Privacy::Safe,
                        serde_json::json!({"attempt": attempt, "error": error.to_string()}),
                    );
                    std::thread::sleep(Duration::from_millis(500));
                }
                Err(error) => return Err(error),
            }
        }
    };
    let mut stream = build()
        .map_err(|error| {
            overlay::push_log(format!("(mic init failed: {error})"));
            telemetry::typed_error(
                "ERR_MIC_INIT_FAILED",
                "speech",
                "microphone initialization failed",
                serde_json::json!({"error": error.to_string()}),
            );
        })
        .ok();
    let mut device = crate::api::realtime_audio::current_input_device_name();
    telemetry::event(
        "mic_stream_state",
        "speech",
        Privacy::Safe,
        serde_json::json!({
            "state": if stream.is_some() { "ready" } else { "unavailable" },
            "device": device,
        }),
    );
    let mut last_check = Instant::now();
    while !stop.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(200));
        if last_check.elapsed() < Duration::from_secs(2) {
            continue;
        }
        last_check = Instant::now();
        if stream.is_none() {
            stream = build()
                .map_err(|error| {
                    overlay::push_log(format!("(mic recovery pending: {error})"));
                })
                .ok();
            if stream.is_some() {
                overlay::push_log("(microphone recovered)".to_string());
            }
        }
        let now = crate::api::realtime_audio::current_input_device_name();
        if now != device {
            overlay::push_log(format!(
                "(audio device changed -> {} - re-initializing mic)",
                now.as_deref().unwrap_or("none")
            ));
            telemetry::event(
                "mic_device_changed",
                "speech",
                Privacy::Safe,
                serde_json::json!({"previous": device, "current": now}),
            );
            device = now;
            drop(stream.take()); // release the OLD device before grabbing the new one
            std::thread::sleep(Duration::from_millis(300)); // let the switch settle
            stream = build()
                .map_err(|e| overlay::push_log(format!("(mic re-init failed: {e})")))
                .ok();
        }
    }
    drop(stream);
    telemetry::event(
        "mic_stream_state",
        "speech",
        Privacy::Safe,
        serde_json::json!({"state": "stopped", "device": device}),
    );
}

fn is_transient_device_error(error: &anyhow::Error) -> bool {
    let detail = error.to_string().to_ascii_lowercase();
    detail.contains("device busy")
        || detail.contains("device invalidated")
        || detail.contains("device not available")
        || detail.contains("os error 997")
}

#[cfg(test)]
mod tests {
    use super::{is_transient_device_error, rms, should_upload};

    #[test]
    fn uplink_keeps_all_nonempty_unmuted_audio() {
        assert!(should_upload(960, false));
        assert!(!should_upload(0, false));
        assert!(!should_upload(960, true));
    }

    #[test]
    fn rms_handles_silence_and_signal() {
        assert_eq!(rms(&[]), 0.0);
        assert_eq!(rms(&[100, -100]), 100.0);
    }

    #[test]
    fn retries_device_churn_but_not_com_apartment_conflicts() {
        assert!(is_transient_device_error(&anyhow::anyhow!(
            "device invalidated"
        )));
        assert!(!is_transient_device_error(&anyhow::anyhow!(
            "Cannot change thread mode after it is set"
        )));
    }
}
