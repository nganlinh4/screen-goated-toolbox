//! Token-budget admission for endpoints that publish a per-minute ceiling.
//!
//! Providers report the remaining tokens in the current window on every
//! response. When that balance cannot cover even the cheapest request an
//! endpoint could receive, the next attempt is certain to be rejected, so the
//! chain skips it instead of paying for the round trip and then waiting out a
//! cooldown.
//!
//! The rule is deliberately one-sided: a request is skipped only when it cannot
//! possibly succeed. Anything that might fit is attempted, because a wrongly
//! benched endpoint costs far more than the few hundred milliseconds a rejected
//! call costs.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

/// Cheapest image an endpoint can be billed for, measured against the live Groq
/// endpoint on 2026-08-19 across eleven aspect ratios from 1:4 to 3:1.
///
/// Cost there tracks shape rather than size and is not monotonic in the aspect
/// ratio, so it cannot be predicted from the dimensions alone: 1024x512 billed
/// 770 while 1024x341 billed 1026 and 1024x682 billed 1794. The floor is stable
/// though, and a floor is all an admission check needs.
pub(super) const MEASURED_MIN_IMAGE_TOKENS: u32 = 770;

#[derive(Clone, Copy, Debug)]
struct TokenBudget {
    limit: u32,
    remaining: u32,
    reset: Duration,
    observed: Instant,
}

static TOKEN_BUDGETS: LazyLock<Mutex<HashMap<String, TokenBudget>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Stores the token balance a provider reported for one endpoint.
pub(super) fn record(endpoint: &str, limit: u32, remaining: u32, reset: Duration) {
    record_at(endpoint, limit, remaining, reset, Instant::now());
}

fn record_at(endpoint: &str, limit: u32, remaining: u32, reset: Duration, now: Instant) {
    if limit == 0 {
        return;
    }
    if let Ok(mut budgets) = TOKEN_BUDGETS.lock() {
        budgets.insert(
            endpoint.to_string(),
            TokenBudget {
                limit,
                remaining: remaining.min(limit),
                reset,
                observed: now,
            },
        );
    }
}

/// Seconds to wait before `minimum_cost` tokens can be available, or `None`
/// when the endpoint can already be attempted.
pub(super) fn shortfall(endpoint: &str, minimum_cost: u32) -> Option<Duration> {
    shortfall_at(endpoint, minimum_cost, Instant::now())
}

fn shortfall_at(endpoint: &str, minimum_cost: u32, now: Instant) -> Option<Duration> {
    let budget = TOKEN_BUDGETS.lock().ok()?.get(endpoint).copied()?;
    if budget.reset.is_zero() {
        return None;
    }
    let elapsed = now.saturating_duration_since(budget.observed);
    // The window refills continuously, so a stale reading only understates what
    // is available; project it forward before deciding anything.
    let refilled = f64::from(budget.limit) * (elapsed.as_secs_f64() / budget.reset.as_secs_f64());
    let available = (f64::from(budget.remaining) + refilled).min(f64::from(budget.limit));
    if available >= f64::from(minimum_cost) {
        return None;
    }
    let missing = f64::from(minimum_cost) - available;
    let seconds = missing / (f64::from(budget.limit) / budget.reset.as_secs_f64());
    Some(Duration::from_secs_f64(seconds.max(0.0)).min(budget.reset))
}

/// Drops the stored balance for an endpoint. Test-only: in production a stale
/// balance cannot block for ever, because the projection refills it over time.
#[cfg(test)]
fn forget(endpoint: &str) {
    if let Ok(mut budgets) = TOKEN_BUDGETS.lock() {
        budgets.remove(endpoint);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(name: &str) -> String {
        format!("test:{name}")
    }

    #[test]
    fn a_full_window_never_blocks() {
        let endpoint = key("full");
        record(&endpoint, 8_000, 8_000, Duration::from_secs(22));
        assert_eq!(shortfall(&endpoint, 1_282), None);
        forget(&endpoint);
    }

    #[test]
    fn an_exhausted_window_reports_the_wait_and_refills_over_time() {
        let endpoint = key("exhausted");
        let start = Instant::now();
        record_at(&endpoint, 8_000, 0, Duration::from_secs(20), start);

        // Nothing available yet: a 1_282-token request needs about 3.2s of refill.
        let wait = shortfall_at(&endpoint, 1_282, start).expect("should report a wait");
        assert!(
            (wait.as_secs_f64() - 3.205).abs() < 0.05,
            "unexpected wait {wait:?}"
        );

        // Half the window later the balance covers it.
        assert_eq!(
            shortfall_at(&endpoint, 1_282, start + Duration::from_secs(10)),
            None
        );
        forget(&endpoint);
    }

    #[test]
    fn only_a_certain_shortfall_blocks() {
        let endpoint = key("borderline");
        let start = Instant::now();
        // Enough for the cheapest request, not for the dearest: must not block,
        // because the cheap case is the one we are allowed to assume.
        record_at(&endpoint, 8_000, 1_400, Duration::from_secs(20), start);
        assert_eq!(
            shortfall_at(&endpoint, MEASURED_MIN_IMAGE_TOKENS + 512, start),
            None
        );
        forget(&endpoint);
    }

    #[test]
    fn an_unknown_endpoint_or_missing_window_is_never_blocked() {
        assert_eq!(shortfall(&key("never-seen"), 5_000), None);
        let endpoint = key("no-window");
        record(&endpoint, 8_000, 0, Duration::ZERO);
        assert_eq!(shortfall(&endpoint, 5_000), None);
        forget(&endpoint);
    }
}
