//! Production scripted-run completion contract.

use std::time::Duration;

use super::reader::Reader;

pub(super) fn idle_settle() -> Duration {
    Duration::from_secs(env_seconds("CC_SCRIPTED_IDLE_SETTLE_SECS", 2))
}

pub(super) fn deadline() -> Duration {
    Duration::from_secs(env_seconds("CC_SCRIPTED_DEADLINE_SECS", 300))
}

fn env_seconds(name: &str, fallback: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(fallback)
}

pub(super) fn has_accepted_completion(state: &Reader) -> bool {
    state.terminal_drain && state.terminal_accepted && state.turn_summary_emitted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_is_not_success_without_the_terminal_latch() {
        let idle = Reader {
            turn_summary_emitted: true,
            ..Reader::default()
        };
        assert!(!has_accepted_completion(&idle));

        let accepted = Reader {
            terminal_drain: true,
            terminal_accepted: true,
            turn_summary_emitted: true,
            ..Reader::default()
        };
        assert!(has_accepted_completion(&accepted));

        let failed = Reader {
            terminal_drain: true,
            terminal_accepted: false,
            turn_summary_emitted: true,
            ..Reader::default()
        };
        assert!(!has_accepted_completion(&failed));
    }
}
