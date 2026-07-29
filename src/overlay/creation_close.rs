use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{IsWindow, PostMessageW, WM_CLOSE};

#[derive(Default)]
struct CloseRetryState {
    attempts: HashMap<usize, u32>,
    pending: HashMap<usize, usize>,
    next_token: usize,
}

static RETRIES: LazyLock<Mutex<CloseRetryState>> =
    LazyLock::new(|| Mutex::new(CloseRetryState::default()));
static CLOSING_PRODUCTS: LazyLock<Mutex<HashSet<&'static str>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

pub(crate) fn begin_product(product: &'static str) {
    CLOSING_PRODUCTS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .insert(product);
}

pub(crate) fn reset_product(product: &'static str) {
    CLOSING_PRODUCTS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .remove(product);
}

pub(crate) fn ensure_accepting(product: &str) -> Result<(), String> {
    if is_closing(product) {
        return Err("This creation window is closing.".to_string());
    }
    Ok(())
}

pub(crate) fn is_closing(product: &str) -> bool {
    CLOSING_PRODUCTS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .contains(product)
}

pub(crate) fn schedule(hwnd: HWND) {
    if hwnd.is_invalid() {
        return;
    }
    let key = hwnd.0 as usize;
    let (delay, token) = {
        let mut state = RETRIES.lock().unwrap_or_else(|error| error.into_inner());
        if state.pending.contains_key(&key) {
            return;
        }
        let attempt = state.attempts.entry(key).or_default();
        *attempt = attempt.saturating_add(1);
        let delay = retry_delay(*attempt);
        state.next_token = state.next_token.wrapping_add(1).max(1);
        let token = state.next_token;
        state.pending.insert(key, token);
        (delay, token)
    };
    std::thread::spawn(move || {
        std::thread::sleep(delay);
        let hwnd = HWND(key as *mut c_void);
        unsafe {
            if IsWindow(Some(hwnd)).as_bool() {
                if PostMessageW(
                    Some(hwnd),
                    WM_CLOSE,
                    windows::Win32::Foundation::WPARAM(token),
                    Default::default(),
                )
                .is_err()
                {
                    discard_retry(key, token);
                }
            } else {
                discard_retry(key, token);
            }
        }
    });
}

pub(crate) fn accept_retry(hwnd: HWND, token: usize) -> bool {
    let key = hwnd.0 as usize;
    let mut state = RETRIES.lock().unwrap_or_else(|error| error.into_inner());
    consume_retry(&mut state, key, token)
}

pub(crate) fn clear(hwnd: HWND) {
    let key = hwnd.0 as usize;
    let mut state = RETRIES.lock().unwrap_or_else(|error| error.into_inner());
    state.pending.remove(&key);
    state.attempts.remove(&key);
}

fn discard_retry(key: usize, token: usize) {
    let mut state = RETRIES.lock().unwrap_or_else(|error| error.into_inner());
    let _ = consume_retry(&mut state, key, token);
}

fn consume_retry(state: &mut CloseRetryState, key: usize, token: usize) -> bool {
    if state.pending.get(&key).copied() != Some(token) {
        return false;
    }
    state.pending.remove(&key);
    true
}

fn retry_delay(attempt: u32) -> Duration {
    let exponent = attempt.saturating_sub(1).min(5);
    Duration::from_millis((100_u64 << exponent).min(2_000))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_retry_uses_bounded_backoff() {
        assert_eq!(retry_delay(1), Duration::from_millis(100));
        assert_eq!(retry_delay(2), Duration::from_millis(200));
        assert_eq!(retry_delay(99), Duration::from_millis(2_000));
    }

    #[test]
    fn stale_retry_token_cannot_close_a_reused_window_handle() {
        let mut state = CloseRetryState::default();
        state.pending.insert(42, 200);
        assert!(!consume_retry(&mut state, 42, 100));
        assert_eq!(state.pending.get(&42), Some(&200));
        assert!(consume_retry(&mut state, 42, 200));
        assert!(!consume_retry(&mut state, 42, 200));
    }
}
