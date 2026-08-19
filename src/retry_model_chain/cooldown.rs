//! Per-model cooldown and circuit-breaker state for the retry chain.
//!
//! A model that rate-limits, times out repeatedly, runs out of credit, or is
//! withdrawn by its provider is benched here so later chain steps stop paying
//! for a failure that is already known.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

#[cfg(not(feature = "recorder-worker"))]
pub(super) const MODEL_RATE_LIMIT_COOLDOWN: Duration = Duration::from_secs(300);
#[cfg(not(feature = "recorder-worker"))]
pub(super) const MODEL_TIMEOUT_COOLDOWN: Duration = Duration::from_secs(30 * 60);
#[cfg(not(feature = "recorder-worker"))]
pub(super) const MODEL_UNAVAILABLE_COOLDOWN: Duration = Duration::from_secs(6 * 60 * 60);
#[cfg(not(feature = "recorder-worker"))]
pub(super) const MODEL_BILLING_COOLDOWN: Duration = Duration::from_secs(6 * 60 * 60);
#[cfg(not(feature = "recorder-worker"))]
pub(super) const MODEL_TIMEOUT_FAILURE_THRESHOLD: u8 = 2;

#[cfg(not(feature = "recorder-worker"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ModelCooldownKind {
    RateLimit,
    Timeout,
    Unavailable,
    Billing,
}

#[cfg(not(feature = "recorder-worker"))]
impl ModelCooldownKind {
    fn duration(self) -> Duration {
        match self {
            Self::RateLimit => MODEL_RATE_LIMIT_COOLDOWN,
            Self::Timeout => MODEL_TIMEOUT_COOLDOWN,
            Self::Unavailable => MODEL_UNAVAILABLE_COOLDOWN,
            Self::Billing => MODEL_BILLING_COOLDOWN,
        }
    }

    fn reason(self) -> &'static str {
        match self {
            Self::RateLimit => "MODEL_RATE_LIMIT_COOLDOWN",
            Self::Timeout => "MODEL_TIMEOUT_COOLDOWN",
            Self::Unavailable => "MODEL_UNAVAILABLE_COOLDOWN",
            Self::Billing => "MODEL_BILLING_COOLDOWN",
        }
    }
}

#[cfg(not(feature = "recorder-worker"))]
#[derive(Clone, Copy, Debug)]
pub(super) enum ModelCircuitState {
    Monitoring {
        timeout_failures: u8,
    },
    Open {
        kind: ModelCooldownKind,
        until: Instant,
    },
    HalfOpen {
        kind: ModelCooldownKind,
    },
}

#[cfg(not(feature = "recorder-worker"))]
static MODEL_CIRCUITS: LazyLock<Mutex<HashMap<String, ModelCircuitState>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[cfg(feature = "recorder-worker")]
static MODEL_COOLDOWNS: LazyLock<Mutex<HashMap<String, Instant>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Longest provider-reported wait we will honour before falling back to the
/// fixed cooldown, so a malformed hint cannot bench a model indefinitely.
#[cfg(not(feature = "recorder-worker"))]
const MAX_REPORTED_COOLDOWN: Duration = Duration::from_secs(6 * 60 * 60);
#[cfg(not(feature = "recorder-worker"))]
const MIN_REPORTED_COOLDOWN: Duration = Duration::from_secs(5);

/// Reads the wait a provider reports alongside a rate-limit rejection.
///
/// Groq answers a 429 with `retry-after` and a message ending in "Please try
/// again in 22.012s"; Gemini has no rate-limit headers at all but ends its
/// RESOURCE_EXHAUSTED body with "Please retry in 32.814072061s". Token and
/// request windows differ by orders of magnitude: a token-per-minute window reopens in seconds while an exhausted
/// daily quota takes hours. Honouring the reported figure keeps a fast model
/// available instead of benching it for the fixed five minutes, and still backs
/// off properly when the provider really is done for the day.
#[cfg(not(feature = "recorder-worker"))]
pub(super) fn reported_cooldown(error: &str) -> Option<Duration> {
    let lower = error.to_ascii_lowercase();
    // "please retry in 32.8s" is Gemini's wording; the others are Groq/OpenAI style.
    let tail = [
        "try again in ",
        "retry in ",
        "retry after ",
        "retry-after: ",
    ]
    .into_iter()
    .find_map(|marker| lower.split_once(marker).map(|(_, rest)| rest.to_string()))?;
    let seconds = parse_duration_seconds(tail.trim())?;
    Some(Duration::from_secs_f64(seconds).clamp(MIN_REPORTED_COOLDOWN, MAX_REPORTED_COOLDOWN))
}

/// Parses `22.012s`, `1m30s`, `2m`, `500ms`, or a bare `22` into seconds.
#[cfg(not(feature = "recorder-worker"))]
pub(super) fn parse_duration_seconds(text: &str) -> Option<f64> {
    let bytes = text.as_bytes();
    let mut total = 0.0;
    let mut number = String::new();
    let mut matched = false;
    let mut index = 0;
    while index < bytes.len() {
        let ch = bytes[index] as char;
        match ch {
            '0'..='9' | '.' => number.push(ch),
            'h' | 'm' | 's' if !number.is_empty() => {
                let value: f64 = number.parse().ok()?;
                // `ms` is milliseconds; only a bare `m` means minutes.
                let is_millis = ch == 'm' && bytes.get(index + 1).is_some_and(|next| *next == b's');
                total += match ch {
                    'h' => value * 3600.0,
                    'm' if !is_millis => value * 60.0,
                    'm' => value / 1000.0,
                    _ => value,
                };
                if is_millis {
                    index += 1;
                }
                number.clear();
                matched = true;
            }
            _ => break,
        }
        index += 1;
    }
    if !number.is_empty() && !matched {
        total = number.parse().ok()?;
        matched = true;
    }
    matched.then_some(total).filter(|value| *value > 0.0)
}

#[cfg(not(feature = "recorder-worker"))]
pub(super) fn rate_limit_error(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("http 429")
        || lower.contains("status code 429")
        || lower.contains("rate limit")
        || lower.contains("too many requests")
        || lower.contains("quota exceeded")
}

#[cfg(not(feature = "recorder-worker"))]
pub(super) fn timeout_error(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("timeout") || lower.contains("timed out") || lower.contains("deadline exceeded")
}

#[cfg(not(feature = "recorder-worker"))]
pub(super) fn billing_error(error: &str) -> bool {
    crate::overlay::utils::is_billing_exhausted_error(error)
}

#[cfg(not(feature = "recorder-worker"))]
pub(super) fn unavailable_model_error(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    let is_404 = lower.contains("http 404")
        || lower.contains("status code 404")
        || lower.contains("error 404");
    if !is_404 || !(lower.contains("model") || lower.contains("deployment")) {
        return false;
    }
    lower.contains("unavailable")
        || lower.contains("archived")
        || lower.contains("not found")
        || lower.contains("no such model")
        || lower.contains("does not exist")
        || lower.contains("doesn't exist")
        || lower.contains("decommissioned")
        || lower.contains("deprecated")
        || lower.contains("has been removed")
        || lower.contains("was removed")
}

#[cfg(not(feature = "recorder-worker"))]
pub fn record_model_failure(model_id: &str, error: &str) {
    record_model_failure_at(model_id, error, Instant::now());
}

#[cfg(not(feature = "recorder-worker"))]
pub(super) fn record_model_failure_at(model_id: &str, error: &str, now: Instant) {
    let is_rate_limit = rate_limit_error(error);
    let is_timeout = timeout_error(error);
    let is_unavailable = unavailable_model_error(error);
    let is_billing = billing_error(error);
    // A provider that tells us when it reopens is more accurate than any constant.
    let reported = is_rate_limit.then(|| reported_cooldown(error)).flatten();
    let cooldown_for = |kind: ModelCooldownKind| match kind {
        ModelCooldownKind::RateLimit => reported.unwrap_or_else(|| kind.duration()),
        other => other.duration(),
    };
    if !is_rate_limit && !is_timeout && !is_unavailable && !is_billing {
        if let Ok(mut circuits) = MODEL_CIRCUITS.lock()
            && let Some(ModelCircuitState::HalfOpen { kind }) = circuits.get(model_id).copied()
        {
            circuits.insert(
                model_id.to_string(),
                ModelCircuitState::Open {
                    kind,
                    until: now + kind.duration(),
                },
            );
        }
        return;
    }

    let Ok(mut circuits) = MODEL_CIRCUITS.lock() else {
        return;
    };
    let state = circuits
        .entry(model_id.to_string())
        .or_insert(ModelCircuitState::Monitoring {
            timeout_failures: 0,
        });

    if matches!(*state, ModelCircuitState::Open { until, .. } if until <= now) {
        *state = ModelCircuitState::Monitoring {
            timeout_failures: 0,
        };
    }

    match *state {
        ModelCircuitState::Open { until, .. } if until > now => {}
        ModelCircuitState::HalfOpen { kind } => {
            let kind = if is_billing {
                ModelCooldownKind::Billing
            } else if is_rate_limit {
                ModelCooldownKind::RateLimit
            } else if is_timeout {
                ModelCooldownKind::Timeout
            } else if is_unavailable {
                ModelCooldownKind::Unavailable
            } else {
                kind
            };
            *state = ModelCircuitState::Open {
                kind,
                until: now + cooldown_for(kind),
            };
        }
        ModelCircuitState::Monitoring { .. } if is_billing => {
            let kind = ModelCooldownKind::Billing;
            *state = ModelCircuitState::Open {
                kind,
                until: now + kind.duration(),
            };
        }
        ModelCircuitState::Monitoring { .. } if is_rate_limit => {
            let kind = ModelCooldownKind::RateLimit;
            *state = ModelCircuitState::Open {
                kind,
                until: now + cooldown_for(kind),
            };
        }
        ModelCircuitState::Monitoring { .. } if is_unavailable => {
            let kind = ModelCooldownKind::Unavailable;
            *state = ModelCircuitState::Open {
                kind,
                until: now + kind.duration(),
            };
        }
        ModelCircuitState::Monitoring { timeout_failures } if is_timeout => {
            let timeout_failures = timeout_failures.saturating_add(1);
            if timeout_failures >= MODEL_TIMEOUT_FAILURE_THRESHOLD {
                let kind = ModelCooldownKind::Timeout;
                *state = ModelCircuitState::Open {
                    kind,
                    until: now + kind.duration(),
                };
            } else {
                *state = ModelCircuitState::Monitoring { timeout_failures };
            }
        }
        _ => {}
    }
}

#[cfg(not(feature = "recorder-worker"))]
pub fn record_model_success(model_id: &str) {
    if let Ok(mut circuits) = MODEL_CIRCUITS.lock() {
        circuits.remove(model_id);
    }
}

#[cfg(not(feature = "recorder-worker"))]
pub fn release_model_probe(model_id: &str) {
    if let Ok(mut circuits) = MODEL_CIRCUITS.lock()
        && let Some(ModelCircuitState::HalfOpen { kind }) = circuits.get(model_id).copied()
    {
        circuits.insert(
            model_id.to_string(),
            ModelCircuitState::Open {
                kind,
                until: Instant::now(),
            },
        );
    }
}

#[cfg(not(feature = "recorder-worker"))]
pub fn claim_model_attempt(model_id: &str) -> Option<String> {
    claim_model_attempt_at(model_id, Instant::now())
}

#[cfg(not(feature = "recorder-worker"))]
pub(super) fn claim_model_attempt_at(model_id: &str, now: Instant) -> Option<String> {
    let mut circuits = MODEL_CIRCUITS.lock().ok()?;
    let state = circuits.get_mut(model_id)?;

    match *state {
        ModelCircuitState::Monitoring { .. } => None,
        ModelCircuitState::Open { kind, until } if until > now => Some(format!(
            "{}:{model_id}:{}s",
            kind.reason(),
            until.saturating_duration_since(now).as_secs().max(1)
        )),
        ModelCircuitState::Open { kind, .. } => {
            *state = ModelCircuitState::HalfOpen { kind };
            None
        }
        ModelCircuitState::HalfOpen { .. } => {
            Some(format!("MODEL_COOLDOWN_PROBE_IN_FLIGHT:{model_id}"))
        }
    }
}

#[cfg(not(feature = "recorder-worker"))]
pub(super) fn model_cooldown_skip_reason(model_id: &str) -> Option<String> {
    model_cooldown_skip_reason_at(model_id, Instant::now())
}

#[cfg(feature = "recorder-worker")]
pub(super) fn model_cooldown_remaining(model_id: &str) -> Option<Duration> {
    let now = Instant::now();
    let mut cooldowns = MODEL_COOLDOWNS.lock().ok()?;
    cooldowns.retain(|_, until| *until > now);
    cooldowns
        .get(model_id)
        .map(|until| until.saturating_duration_since(now))
}

#[cfg(not(feature = "recorder-worker"))]
pub(super) fn model_cooldown_skip_reason_at(model_id: &str, now: Instant) -> Option<String> {
    let mut circuits = MODEL_CIRCUITS.lock().ok()?;
    let state = circuits.get_mut(model_id)?;

    match *state {
        ModelCircuitState::Monitoring { .. } => None,
        ModelCircuitState::Open { kind, until } if until > now => Some(format!(
            "{}:{model_id}:{}s",
            kind.reason(),
            until.saturating_duration_since(now).as_secs().max(1)
        )),
        ModelCircuitState::Open { .. } => None,
        ModelCircuitState::HalfOpen { .. } => {
            Some(format!("MODEL_COOLDOWN_PROBE_IN_FLIGHT:{model_id}"))
        }
    }
}
