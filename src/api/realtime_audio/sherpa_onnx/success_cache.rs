use std::sync::{Mutex, OnceLock};

/// Serialize initialization while caching only a successful value.
///
/// `OnceLock<Result<..>>` permanently poisons a process after a transient
/// dependency or installation failure. This helper deliberately leaves the
/// slot empty on error so a later request can retry after prerequisites change.
pub(super) fn get_or_try_init<'a, T, E>(
    slot: &'a OnceLock<T>,
    init_lock: &Mutex<()>,
    init: impl FnOnce() -> Result<T, E>,
) -> Result<&'a T, E> {
    if let Some(value) = slot.get() {
        return Ok(value);
    }

    let _guard = init_lock
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(value) = slot.get() {
        return Ok(value);
    }

    let value = init()?;
    let _ = slot.set(value);
    Ok(slot
        .get()
        .expect("serialized success cache must contain initialized value"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn failed_initialization_can_retry_but_success_is_cached() {
        let slot = OnceLock::new();
        let lock = Mutex::new(());
        let attempts = AtomicUsize::new(0);

        let first = get_or_try_init(&slot, &lock, || {
            attempts.fetch_add(1, Ordering::SeqCst);
            Err::<usize, _>("not ready")
        });
        assert_eq!(first, Err("not ready"));

        let second = get_or_try_init(&slot, &lock, || {
            attempts.fetch_add(1, Ordering::SeqCst);
            Ok::<_, &str>(42)
        })
        .expect("second attempt should recover");
        assert_eq!(*second, 42);

        let third = get_or_try_init(&slot, &lock, || -> Result<usize, &str> {
            panic!("cached success must not initialize again")
        })
        .expect("cached success");
        assert_eq!(*third, 42);
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    }
}
