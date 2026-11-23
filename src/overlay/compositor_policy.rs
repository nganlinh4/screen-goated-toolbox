//! Restart and GPU-fallback policy shared by every compositor supervisor
//! (result scenes, status overlay and realtime overlay), so a fix to backoff
//! or software-rendering fallback reaches all three.

use std::ffi::{OsStr, OsString};
use std::sync::LazyLock;
use std::time::Instant;

pub(crate) const SOFTWARE_RENDERING_ARGUMENT: &str = "--disable-gpu";
const INITIAL_RESTART_DELAY_MS: u64 = 250;
const MAX_RESTART_DELAY_MS: u64 = 30_000;
const GPU_FAILURE_WINDOW_MS: u64 = 60_000;
const GPU_FAILURE_THRESHOLD: u32 = 3;

static MONOTONIC_EPOCH: LazyLock<Instant> = LazyLock::new(Instant::now);

/// Monotonic milliseconds for supervisor deadlines; immune to clock changes.
pub(crate) fn now_ms() -> u64 {
    MONOTONIC_EPOCH.elapsed().as_millis() as u64
}

/// Exponential restart backoff for the `failure`th consecutive failure.
pub(crate) fn restart_delay_ms(failure: u32) -> u64 {
    let shift = failure.saturating_sub(1).min(7);
    INITIAL_RESTART_DELAY_MS
        .saturating_mul(1_u64 << shift)
        .min(MAX_RESTART_DELAY_MS)
}

/// Counts a GPU process failure. Returns the new count and whether the
/// renderer must fall back to software rendering.
pub(crate) fn gpu_failure_state(previous_at: u64, previous_count: u32, now: u64) -> (u32, bool) {
    let count = if now.saturating_sub(previous_at) <= GPU_FAILURE_WINDOW_MS {
        previous_count.saturating_add(1)
    } else {
        1
    };
    (count, count >= GPU_FAILURE_THRESHOLD)
}

pub(crate) fn has_software_rendering_argument(arguments: &OsStr) -> bool {
    arguments
        .to_string_lossy()
        .split_whitespace()
        .any(|argument| argument == SOFTWARE_RENDERING_ARGUMENT)
}

/// Inherited WebView2 browser arguments plus the software-rendering switch.
pub(crate) fn browser_arguments_with_software_fallback(existing: Option<OsString>) -> OsString {
    let mut arguments = existing.unwrap_or_default();
    if !has_software_rendering_argument(&arguments) {
        if !arguments.is_empty() {
            arguments.push(" ");
        }
        arguments.push(SOFTWARE_RENDERING_ARGUMENT);
    }
    arguments
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restart_backoff_is_exponential_and_bounded() {
        assert_eq!(restart_delay_ms(0), 250);
        assert_eq!(restart_delay_ms(1), 250);
        assert_eq!(restart_delay_ms(2), 500);
        assert_eq!(restart_delay_ms(3), 1_000);
        assert_eq!(restart_delay_ms(8), 30_000);
        assert_eq!(restart_delay_ms(u32::MAX), 30_000);
    }

    #[test]
    fn repeated_gpu_failures_request_software_rendering() {
        assert_eq!(gpu_failure_state(1_000, 1, 2_000), (2, false));
        assert_eq!(gpu_failure_state(2_000, 2, 3_000), (3, true));
        assert_eq!(gpu_failure_state(1_000, 3, 70_000), (1, false));
    }

    #[test]
    fn software_fallback_is_added_once_and_keeps_existing_arguments() {
        let arguments = browser_arguments_with_software_fallback(Some("--foo".into()));
        assert_eq!(arguments, OsString::from("--foo --disable-gpu"));
        assert_eq!(
            browser_arguments_with_software_fallback(Some(arguments.clone())),
            arguments
        );
        assert_eq!(
            browser_arguments_with_software_fallback(None),
            OsString::from(SOFTWARE_RENDERING_ARGUMENT)
        );
        assert!(!has_software_rendering_argument(OsStr::new(
            "--disable-gpu-compositing"
        )));
    }
}
