#[cfg(test)]
use std::cell::RefCell;

#[cfg(test)]
thread_local! {
    static OVERRIDE: RefCell<Option<(String, String)>> = const { RefCell::new(None) };
}

pub(crate) fn resolve(env_name: &str, saved: &str) -> String {
    #[cfg(test)]
    if let Some(value) = OVERRIDE.with_borrow(|entry| {
        entry
            .as_ref()
            .filter(|(name, _)| name == env_name)
            .map(|(_, value)| value.clone())
    }) {
        return value;
    }
    resolve_value(std::env::var(env_name).ok(), saved)
}

#[cfg(test)]
pub(crate) fn with_override<T>(env_name: &str, value: &str, operation: impl FnOnce() -> T) -> T {
    struct Restore(Option<(String, String)>);
    impl Drop for Restore {
        fn drop(&mut self) {
            OVERRIDE.with(|slot| {
                slot.replace(self.0.take());
            });
        }
    }

    let previous =
        OVERRIDE.with(|slot| slot.replace(Some((env_name.to_string(), value.to_string()))));
    let _restore = Restore(previous);
    operation()
}

fn resolve_value(environment: Option<String>, saved: &str) -> String {
    environment
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| saved.to_string())
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::{resolve, resolve_value, with_override};

    #[test]
    fn nonempty_environment_value_has_priority() {
        assert_eq!(
            resolve_value(Some(" environment ".to_string()), "saved"),
            "environment"
        );
    }

    #[test]
    fn missing_or_blank_environment_value_uses_saved_value() {
        assert_eq!(resolve_value(None, " saved "), "saved");
        assert_eq!(resolve_value(Some("   ".to_string()), " saved "), "saved");
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
