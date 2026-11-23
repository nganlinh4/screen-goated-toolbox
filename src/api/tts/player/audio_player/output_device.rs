//! Render endpoint selection and stream rebuild pacing for the TTS player.

use std::time::Duration;
use windows::Win32::Media::Audio::*;

/// Opens the configured render endpoint, or the Windows default when nothing
/// is configured or the configured endpoint is missing or not active. Returns
/// the device and whether the stream follows the default endpoint.
pub(super) unsafe fn resolve_render_device(
    enumerator: &IMMDeviceEnumerator,
    configured_id: Option<&str>,
) -> anyhow::Result<(IMMDevice, bool)> {
    if let Some(id) = configured_id {
        let configured = unsafe { enumerator.GetDevice(&windows::core::HSTRING::from(id)) }
            .ok()
            .filter(|device| unsafe { device.GetState() }.ok() == Some(DEVICE_STATE_ACTIVE));
        if let Some(device) = configured {
            return Ok((device, false));
        }
        crate::log_info!(
            "[TTS WASAPI] configured output endpoint is unavailable; using the default endpoint"
        );
    }
    let device = unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole) }?;
    Ok((device, true))
}

pub(super) fn default_render_endpoint_id(enumerator: &IMMDeviceEnumerator) -> Option<String> {
    let device = unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole) }.ok()?;
    unsafe { crate::api::audio::endpoint_id::read(&device) }
        .ok()
        .flatten()
}

const RETRY_BASE: Duration = Duration::from_millis(750);
const RETRY_MAX: Duration = Duration::from_secs(15);
/// A stream that ran this long was healthy, so its failure restarts pacing.
const HEALTHY_STREAM: Duration = Duration::from_secs(10);

/// Exponential rebuild delay with log-once for repeated identical failures.
#[derive(Default)]
pub(super) struct StreamRetry {
    failures: u32,
    last_error: Option<String>,
}

impl StreamRetry {
    /// Records one failed stream and returns (delay before rebuild, log it).
    pub fn record_failure(&mut self, error: &str, ran_for: Duration) -> (Duration, bool) {
        if ran_for >= HEALTHY_STREAM {
            self.failures = 0;
        }
        let repeated = self.failures > 0 && self.last_error.as_deref() == Some(error);
        let delay = RETRY_BASE
            .saturating_mul(1u32 << self.failures.min(5))
            .min(RETRY_MAX);
        self.failures = self.failures.saturating_add(1);
        self.last_error = Some(error.to_string());
        (delay, !repeated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_stream_failures_back_off_and_log_once() {
        let mut retry = StreamRetry::default();
        let mut delays = Vec::new();
        let mut logged = Vec::new();
        for _ in 0..8 {
            let (delay, log) = retry.record_failure("no endpoint", Duration::ZERO);
            delays.push(delay.as_millis());
            logged.push(log);
        }
        assert_eq!(
            delays,
            [750, 1_500, 3_000, 6_000, 12_000, 15_000, 15_000, 15_000]
        );
        assert_eq!(logged.iter().filter(|log| **log).count(), 1);
        let (_, log) = retry.record_failure("different", Duration::ZERO);
        assert!(log, "a new error is logged");
    }

    #[test]
    fn healthy_stream_restarts_pacing() {
        let mut retry = StreamRetry::default();
        for _ in 0..4 {
            retry.record_failure("default changed", Duration::ZERO);
        }
        let (delay, _) = retry.record_failure("default changed", HEALTHY_STREAM);
        assert_eq!(delay, RETRY_BASE);
    }

    #[test]
    fn missing_configured_endpoint_falls_back_to_default() {
        use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};
        assert!(wasapi::initialize_mta().is_ok());
        let enumerator: IMMDeviceEnumerator =
            unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }.unwrap();
        let missing = "{0.0.0.00000000}.{00000000-0000-0000-0000-000000000000}";
        let default_present = default_render_endpoint_id(&enumerator).is_some();
        match unsafe { resolve_render_device(&enumerator, Some(missing)) } {
            Ok((_, follows_default)) => assert!(follows_default),
            Err(error) => assert!(
                !default_present,
                "a missing configured endpoint must use the default: {error}"
            ),
        }
        wasapi::deinitialize();
    }
}
