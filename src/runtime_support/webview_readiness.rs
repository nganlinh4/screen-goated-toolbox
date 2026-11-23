//! Require the runtime to become observable before automatic restart.

const MAX_PROBES: usize = 21;

#[derive(Debug, Eq, PartialEq)]
pub(super) enum Failure {
    InstallerFailed,
    NotDetected,
}

pub(super) fn verify(
    installer_succeeded: bool,
    mut available: impl FnMut() -> bool,
    mut wait: impl FnMut(),
) -> Result<(), Failure> {
    for attempt in 0..MAX_PROBES {
        if available() {
            return Ok(());
        }
        if attempt + 1 < MAX_PROBES {
            wait();
        }
    }
    Err(if installer_succeeded {
        Failure::NotDetected
    } else {
        Failure::InstallerFailed
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn success_without_runtime_is_bounded_and_not_ready() {
        let probes = Cell::new(0);
        let waits = Cell::new(0);
        assert_eq!(
            verify(
                true,
                || {
                    probes.set(probes.get() + 1);
                    false
                },
                || waits.set(waits.get() + 1)
            ),
            Err(Failure::NotDetected)
        );
        assert_eq!(probes.get(), MAX_PROBES);
        assert_eq!(waits.get(), MAX_PROBES - 1);
    }

    #[test]
    fn delayed_runtime_is_accepted_without_unneeded_waits() {
        let probes = Cell::new(0);
        let waits = Cell::new(0);
        assert_eq!(
            verify(
                true,
                || {
                    probes.set(probes.get() + 1);
                    probes.get() == 4
                },
                || waits.set(waits.get() + 1)
            ),
            Ok(())
        );
        assert_eq!(probes.get(), 4);
        assert_eq!(waits.get(), 3);
    }

    #[test]
    fn nonzero_exit_only_succeeds_when_runtime_is_observed() {
        assert_eq!(
            verify(false, || true, || panic!("already available")),
            Ok(())
        );
        assert_eq!(
            verify(false, || false, || {}),
            Err(Failure::InstallerFailed)
        );
    }
}
