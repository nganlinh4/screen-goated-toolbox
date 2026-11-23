use super::*;

// Restart backoff and GPU fallback are covered once in compositor_policy.

#[test]
fn heartbeat_timeout_uses_saturating_elapsed_time() {
    assert!(!heartbeat_is_stale(5_000, 0, 5_000));
    assert!(heartbeat_is_stale(5_001, 0, 5_000));
    assert!(!heartbeat_is_stale(1, 2, 5_000));
}
