use super::*;

#[test]
fn restart_backoff_is_exponential_and_bounded() {
    assert_eq!(restart_delay_ms(1), 250);
    assert_eq!(restart_delay_ms(2), 500);
    assert_eq!(restart_delay_ms(3), 1_000);
    assert_eq!(restart_delay_ms(8), 30_000);
    assert_eq!(restart_delay_ms(u32::MAX), 30_000);
}

#[test]
fn heartbeat_timeout_uses_saturating_elapsed_time() {
    assert!(!heartbeat_is_stale(5_000, 0, 5_000));
    assert!(heartbeat_is_stale(5_001, 0, 5_000));
    assert!(!heartbeat_is_stale(1, 2, 5_000));
}

#[test]
fn repeated_gpu_failures_activate_fallback_only_inside_the_window() {
    assert_eq!(gpu_failure_state(0, 0, 100), (1, false));
    assert_eq!(gpu_failure_state(100, 1, 200), (2, false));
    assert_eq!(gpu_failure_state(200, 2, 300), (3, true));
    assert_eq!(gpu_failure_state(200, 2, 60_201), (1, false));
}

#[test]
fn software_fallback_preserves_existing_webview_arguments() {
    assert_eq!(
        browser_arguments_with_software_fallback(Some(OsString::from("--foo"))),
        OsString::from("--foo --disable-gpu")
    );
    assert_eq!(
        browser_arguments_with_software_fallback(None),
        OsString::from("--disable-gpu")
    );
    assert_eq!(
        browser_arguments_with_software_fallback(Some(OsString::from("--disable-gpu"))),
        OsString::from("--disable-gpu")
    );
}
