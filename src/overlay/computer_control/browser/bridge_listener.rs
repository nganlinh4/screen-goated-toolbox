//! Retryable listener acquisition for the extension bridge.

use std::net::TcpListener;
use std::time::Duration;

use serde_json::json;

const RETRY_INITIAL: Duration = Duration::from_millis(100);
const RETRY_MAX: Duration = Duration::from_secs(5);

pub(super) fn bind_with_retry(addr: &str) -> TcpListener {
    let mut failures = 0_u32;
    loop {
        match TcpListener::bind(addr) {
            Ok(listener) => return listener,
            Err(error) => {
                let delay = retry_delay(failures);
                if failures == 0 || failures.is_power_of_two() {
                    super::super::telemetry::typed_error(
                        "ERR_BROWSER_BRIDGE_BIND",
                        "browser_bridge",
                        "browser bridge bind failed; retrying",
                        json!({
                            "addr": addr,
                            "error": error.to_string(),
                            "failure_count": failures + 1,
                            "retry_ms": delay.as_millis(),
                        }),
                    );
                }
                std::thread::sleep(delay);
                failures = failures.saturating_add(1);
            }
        }
    }
}

fn retry_delay(failures: u32) -> Duration {
    let factor = 1_u32 << failures.min(6);
    RETRY_INITIAL.saturating_mul(factor).min(RETRY_MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bind_retry_backoff_is_monotonic_and_bounded() {
        let delays: Vec<_> = (0..20).map(retry_delay).collect();
        assert_eq!(delays[0], RETRY_INITIAL);
        assert!(delays.windows(2).all(|pair| pair[0] <= pair[1]));
        assert!(delays.iter().all(|delay| *delay <= RETRY_MAX));
        assert_eq!(delays.last(), Some(&RETRY_MAX));
    }
}
