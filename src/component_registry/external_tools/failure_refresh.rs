//! Catalog refresh after a downloader failure, rate limited so that repeated
//! failures caused by the input (a half-typed URL) do not hammer the signed
//! catalog endpoint.

use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use anyhow::Result;

use super::{ExternalTool, delivery_optional};

const MIN_REFRESH_INTERVAL: Duration = Duration::from_secs(5 * 60);

static LAST_REFRESH: Mutex<Option<Instant>> = Mutex::new(None);

/// Returns the tools whose delivery changed, or nothing when no refresh ran.
pub(crate) fn refresh_downloader_after_failure(include_deno: bool) -> Result<Vec<ExternalTool>> {
    let candidates = [ExternalTool::YtDlp, ExternalTool::Deno]
        .into_iter()
        .filter(|tool| *tool != ExternalTool::Deno || include_deno)
        .filter(|tool| {
            super::super::update_catalog::policy(tool.id())
                .is_some_and(|(mode, _, _)| mode == "typed-failure")
        })
        .collect::<Vec<_>>();
    if candidates.is_empty() || !claim_refresh(&LAST_REFRESH, Instant::now()) {
        return Ok(Vec::new());
    }
    let before = candidates
        .iter()
        .map(|tool| {
            (
                *tool,
                delivery_optional(*tool).map(|delivery| (delivery.version, delivery.sha256)),
            )
        })
        .collect::<Vec<_>>();
    super::super::update_catalog::refresh_now()?;
    Ok(before
        .into_iter()
        .filter_map(|(tool, previous)| {
            let current =
                delivery_optional(tool).map(|delivery| (delivery.version, delivery.sha256));
            (previous.is_some() && current.is_some() && previous != current).then_some(tool)
        })
        .collect())
}

fn claim_refresh(last: &Mutex<Option<Instant>>, now: Instant) -> bool {
    let mut last = last.lock().unwrap_or_else(PoisonError::into_inner);
    if last.is_some_and(|at| now.saturating_duration_since(at) < MIN_REFRESH_INTERVAL) {
        return false;
    }
    *last = Some(now);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_refreshes_are_rate_limited() {
        let last = Mutex::new(None);
        let start = Instant::now();
        assert!(claim_refresh(&last, start));
        assert!(!claim_refresh(&last, start + Duration::from_secs(1)));
        assert!(!claim_refresh(
            &last,
            start + MIN_REFRESH_INTERVAL - Duration::from_secs(1)
        ));
        assert!(claim_refresh(&last, start + MIN_REFRESH_INTERVAL));
    }
}
