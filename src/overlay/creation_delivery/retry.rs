use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

static RUNNING: AtomicBool = AtomicBool::new(false);
static NOT_BEFORE_MS: AtomicU64 = AtomicU64::new(0);
static PRODUCTS: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

pub(crate) fn schedule_reconciliation(product: &str) {
    if super::validate_product(product).is_err() {
        return;
    }
    let has_pending = super::DELIVERY_LOCK
        .lock()
        .map_err(|_| ())
        .and_then(|_guard| {
            super::load_store(&super::journal_path())
                .map(|store| {
                    store.entries.iter().any(|entry| entry.product == product)
                        || store
                            .cancellations
                            .iter()
                            .any(|entry| entry.product == product)
                })
                .map_err(|_| ())
        })
        .unwrap_or(true);
    if !has_pending {
        if let Ok(mut products) = PRODUCTS.lock() {
            products.remove(product);
        }
        return;
    }
    if let Ok(mut products) = PRODUCTS.lock() {
        products.insert(product.to_string());
    } else {
        return;
    }
    start_owner();
}

fn start_owner() {
    if RUNNING
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    std::thread::spawn(|| {
        let wait_ms = retry_start_delay_ms(now_ms(), NOT_BEFORE_MS.load(Ordering::Acquire));
        if wait_ms > 0 {
            std::thread::sleep(Duration::from_millis(wait_ms));
        }
        for delay in [100_u64, 500, 2_000, 5_000] {
            std::thread::sleep(Duration::from_millis(delay));
            let products = PRODUCTS
                .lock()
                .map(|items| items.iter().cloned().collect::<Vec<_>>())
                .unwrap_or_default();
            for product in products {
                if super::reconcile_product(&product).is_ok_and(|pending| pending.is_empty())
                    && let Ok(mut items) = PRODUCTS.lock()
                {
                    items.remove(&product);
                }
            }
            if PRODUCTS.lock().is_ok_and(|items| items.is_empty()) {
                break;
            }
        }
        NOT_BEFORE_MS.store(now_ms().saturating_add(30_000), Ordering::Release);
        RUNNING.store(false, Ordering::Release);
        if retry_owner_should_restart(
            PRODUCTS
                .lock()
                .map(|items| !items.is_empty())
                .unwrap_or(false),
        ) {
            start_owner();
        }
    });
}

pub(super) fn retry_start_delay_ms(now: u64, not_before: u64) -> u64 {
    not_before.saturating_sub(now)
}

fn retry_owner_should_restart(has_pending: bool) -> bool {
    has_pending
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_work_always_gets_another_bounded_retry_cycle() {
        assert!(retry_owner_should_restart(true));
        assert!(!retry_owner_should_restart(false));
        assert_eq!(retry_start_delay_ms(10_000, 35_000), 25_000);
        assert_eq!(retry_start_delay_ms(35_000, 10_000), 0);
    }
}
