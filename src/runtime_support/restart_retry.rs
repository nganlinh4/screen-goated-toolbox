use crate::gui::app::RestartPrepareDenied;

/// Retry only transient admission failures. All other failures require the
/// user to resolve the condition and restart manually.
pub(super) fn prepare_with_bounded_retry<T>(
    mut prepare: impl FnMut() -> Result<T, RestartPrepareDenied>,
    mut explicit_exit_pending: impl FnMut() -> bool,
    attempts: usize,
    mut wait: impl FnMut(),
) -> Result<T, RestartPrepareDenied> {
    for attempt in 0..attempts {
        let result = if explicit_exit_pending() {
            Err(RestartPrepareDenied::Busy)
        } else {
            match prepare() {
                Ok(prepared) if explicit_exit_pending() => {
                    drop(prepared);
                    Err(RestartPrepareDenied::Busy)
                }
                result => result,
            }
        };
        match result {
            Err(RestartPrepareDenied::Busy) if attempt + 1 < attempts => wait(),
            result => return result,
        }
    }
    Err(RestartPrepareDenied::Busy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn busy_then_available_prepares_after_waiting() {
        let calls = Cell::new(0);
        let waits = Cell::new(0);
        let result = prepare_with_bounded_retry(
            || {
                calls.set(calls.get() + 1);
                if calls.get() < 3 {
                    Err(RestartPrepareDenied::Busy)
                } else {
                    Ok("prepared")
                }
            },
            || false,
            5,
            || waits.set(waits.get() + 1),
        );
        assert_eq!(result, Ok("prepared"));
        assert_eq!((calls.get(), waits.get()), (3, 2));
    }

    #[test]
    fn persistent_busy_stops_at_limit_and_conflict_does_not_retry() {
        let calls = Cell::new(0);
        let waits = Cell::new(0);
        let result: Result<(), _> = prepare_with_bounded_retry(
            || {
                calls.set(calls.get() + 1);
                Err(RestartPrepareDenied::Busy)
            },
            || false,
            3,
            || waits.set(waits.get() + 1),
        );
        assert_eq!(result, Err(RestartPrepareDenied::Busy));
        assert_eq!((calls.get(), waits.get()), (3, 2));

        let result: Result<(), _> = prepare_with_bounded_retry(
            || Err(RestartPrepareDenied::Blocked),
            || false,
            3,
            || panic!("a conflict or failure must not be retried"),
        );
        assert_eq!(result, Err(RestartPrepareDenied::Blocked));
    }

    #[test]
    fn explicit_exit_takes_priority_before_and_after_preparation() {
        struct Guard<'a>(&'a Cell<usize>);
        impl Drop for Guard<'_> {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }

        let checks = Cell::new(0);
        let calls = Cell::new(0);
        let drops = Cell::new(0);
        let waits = Cell::new(0);
        let result = prepare_with_bounded_retry(
            || {
                calls.set(calls.get() + 1);
                Ok(Guard(&drops))
            },
            || {
                checks.set(checks.get() + 1);
                // First attempt is already pending; another request arrives
                // while the second attempt is acquiring its guard.
                matches!(checks.get(), 1 | 3)
            },
            3,
            || waits.set(waits.get() + 1),
        );
        assert!(result.is_ok());
        assert_eq!(
            (checks.get(), calls.get(), waits.get(), drops.get()),
            (5, 2, 2, 1)
        );
        drop(result);
        assert_eq!(drops.get(), 2);
    }
}
