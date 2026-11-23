//! Session-local cooldown for the user's realtime translation provider.
//!
//! A failed request is served by the alternate provider for that request only.
//! Repeated failures put the configured provider on a growing cooldown, during
//! which ticks go straight to the alternate instead of waiting on a provider
//! that is rate-limited or offline. When the cooldown ends the configured
//! provider is tried again. Nothing here is persisted: the user's selection in
//! the config is the only source of truth, and changing it clears the cooldown.

use std::time::{Duration, Instant};

const FIRST_COOLDOWN: Duration = Duration::from_secs(5);
const MAX_COOLDOWN: Duration = Duration::from_secs(60);

#[derive(Default)]
pub(super) struct ProviderCooldown {
    model: String,
    consecutive_failures: u32,
    until: Option<Instant>,
}

impl ProviderCooldown {
    /// The provider this tick should call first for the configured model.
    pub(super) fn provider_for<'a>(&self, configured: &'a str, now: Instant) -> &'a str {
        if self.is_cooling(configured, now) {
            alternate_provider(configured)
        } else {
            configured
        }
    }

    /// The configured provider answered (whether or not the answer was still
    /// current), so it is healthy again.
    pub(super) fn record_success(&mut self, configured: &str) {
        self.reset(configured);
    }

    /// The configured provider returned no translation.
    pub(super) fn record_failure(&mut self, configured: &str, now: Instant) {
        if self.model != configured {
            self.reset(configured);
        }
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        self.until = Some(now + cooldown_after(self.consecutive_failures));
    }

    fn is_cooling(&self, configured: &str, now: Instant) -> bool {
        self.model == configured && self.until.is_some_and(|until| now < until)
    }

    fn reset(&mut self, configured: &str) {
        configured.clone_into(&mut self.model);
        self.consecutive_failures = 0;
        self.until = None;
    }
}

/// The other realtime translation provider.
pub(super) fn alternate_provider(model: &str) -> &'static str {
    if model == crate::model_config::REALTIME_TRANSLATION_MODEL_GTX {
        crate::model_config::REALTIME_TRANSLATION_MODEL_LLM
    } else {
        crate::model_config::REALTIME_TRANSLATION_MODEL_GTX
    }
}

/// The first failure only reroutes that request; the cooldown starts with the
/// second consecutive failure and doubles up to a minute.
fn cooldown_after(consecutive_failures: u32) -> Duration {
    if consecutive_failures < 2 {
        return Duration::ZERO;
    }
    let doublings = (consecutive_failures - 2).min(8);
    FIRST_COOLDOWN
        .saturating_mul(1 << doublings)
        .min(MAX_COOLDOWN)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_config::{REALTIME_TRANSLATION_MODEL_GTX, REALTIME_TRANSLATION_MODEL_LLM};

    #[test]
    fn a_single_failure_keeps_the_configured_provider_first() {
        let now = Instant::now();
        let mut cooldown = ProviderCooldown::default();
        cooldown.record_failure(REALTIME_TRANSLATION_MODEL_LLM, now);
        assert_eq!(
            cooldown.provider_for(REALTIME_TRANSLATION_MODEL_LLM, now),
            REALTIME_TRANSLATION_MODEL_LLM
        );
    }

    #[test]
    fn repeated_failures_route_to_the_alternate_then_retry_after_the_backoff() {
        let start = Instant::now();
        let mut cooldown = ProviderCooldown::default();
        cooldown.record_failure(REALTIME_TRANSLATION_MODEL_LLM, start);
        cooldown.record_failure(REALTIME_TRANSLATION_MODEL_LLM, start);
        assert_eq!(
            cooldown.provider_for(REALTIME_TRANSLATION_MODEL_LLM, start),
            REALTIME_TRANSLATION_MODEL_GTX
        );
        assert_eq!(
            cooldown.provider_for(REALTIME_TRANSLATION_MODEL_LLM, start + FIRST_COOLDOWN),
            REALTIME_TRANSLATION_MODEL_LLM,
            "the configured provider is retried once the cooldown ends"
        );

        cooldown.record_failure(REALTIME_TRANSLATION_MODEL_LLM, start);
        assert_eq!(
            cooldown.provider_for(
                REALTIME_TRANSLATION_MODEL_LLM,
                start + FIRST_COOLDOWN + Duration::from_secs(1)
            ),
            REALTIME_TRANSLATION_MODEL_GTX,
            "a further failure doubles the cooldown"
        );

        cooldown.record_success(REALTIME_TRANSLATION_MODEL_LLM);
        assert_eq!(
            cooldown.provider_for(REALTIME_TRANSLATION_MODEL_LLM, start),
            REALTIME_TRANSLATION_MODEL_LLM
        );
    }

    #[test]
    fn the_cooldown_is_bounded_and_belongs_to_the_configured_model() {
        let start = Instant::now();
        let mut cooldown = ProviderCooldown::default();
        for _ in 0..100 {
            cooldown.record_failure(REALTIME_TRANSLATION_MODEL_GTX, start);
        }
        assert_eq!(
            cooldown.provider_for(REALTIME_TRANSLATION_MODEL_GTX, start + MAX_COOLDOWN),
            REALTIME_TRANSLATION_MODEL_GTX
        );
        assert_eq!(
            cooldown.provider_for(REALTIME_TRANSLATION_MODEL_GTX, start),
            REALTIME_TRANSLATION_MODEL_LLM
        );
        assert_eq!(
            cooldown.provider_for(REALTIME_TRANSLATION_MODEL_LLM, start),
            REALTIME_TRANSLATION_MODEL_LLM,
            "a new user selection is not affected by the old provider's cooldown"
        );
    }
}
