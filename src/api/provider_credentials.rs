//! The single provider-credential resolver. Every feature uses the key saved
//! in Settings; process environment variables never override it, so rotating
//! a key in Settings takes effect everywhere and a globally exported key (for
//! example from another CLI tool) cannot silently bill a different project.

#[cfg(test)]
use std::cell::RefCell;

#[cfg(test)]
thread_local! {
    static OVERRIDE: RefCell<Option<(String, String)>> = const { RefCell::new(None) };
}

/// Returns the saved credential for the provider named by `credential_name`.
/// Test harnesses may substitute a value for one name with [`with_override`].
pub(crate) fn resolve(credential_name: &str, saved: &str) -> String {
    #[cfg(test)]
    if let Some(value) = OVERRIDE.with_borrow(|entry| {
        entry
            .as_ref()
            .filter(|(name, _)| name == credential_name)
            .map(|(_, value)| value.clone())
    }) {
        return value;
    }
    #[cfg(not(test))]
    let _ = credential_name;
    saved.trim().to_string()
}

#[cfg(test)]
pub(crate) fn with_override<T>(
    credential_name: &str,
    value: &str,
    operation: impl FnOnce() -> T,
) -> T {
    struct Restore(Option<(String, String)>);
    impl Drop for Restore {
        fn drop(&mut self) {
            OVERRIDE.with(|slot| {
                slot.replace(self.0.take());
            });
        }
    }

    let previous =
        OVERRIDE.with(|slot| slot.replace(Some((credential_name.to_string(), value.to_string()))));
    let _restore = Restore(previous);
    operation()
}

#[cfg(test)]
mod tests {
    use super::{resolve, with_override};

    #[test]
    fn process_environment_never_overrides_the_saved_key() {
        // PATH is always present in the process environment.
        assert!(std::env::var_os("PATH").is_some());
        assert_eq!(resolve("PATH", " saved "), "saved");
        assert_eq!(resolve("PATH", ""), "");
    }

    #[test]
    fn scoped_override_wins_and_is_restored() {
        let before = resolve("SGT_TEST_CREDENTIAL", "saved");
        let during = with_override("SGT_TEST_CREDENTIAL", "rotated", || {
            resolve("SGT_TEST_CREDENTIAL", "saved")
        });
        assert_eq!(during, "rotated");
        assert_eq!(resolve("SGT_TEST_CREDENTIAL", "saved"), before);
    }
}
