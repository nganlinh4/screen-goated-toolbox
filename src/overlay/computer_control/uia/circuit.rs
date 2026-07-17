//! Per-surface timeout circuit for slow or wedged accessibility providers.

use std::error::Error;
use std::fmt;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

const COOLDOWN: Duration = Duration::from_secs(30);
static STATE: OnceLock<Mutex<CircuitState>> = OnceLock::new();

#[derive(Default)]
struct CircuitState {
    blocked: Option<(String, Instant)>,
}

impl CircuitState {
    fn remaining(&mut self, surface: &str, now: Instant) -> Option<Duration> {
        let Some((blocked_surface, until)) = &self.blocked else {
            return None;
        };
        if blocked_surface != surface || *until <= now {
            self.blocked = None;
            return None;
        }
        until.checked_duration_since(now)
    }

    fn timeout(&mut self, surface: &str, now: Instant) {
        self.blocked = Some((surface.to_string(), now + COOLDOWN));
    }

    fn success(&mut self, surface: &str) {
        if self
            .blocked
            .as_ref()
            .is_some_and(|(blocked_surface, _)| blocked_surface == surface)
        {
            self.blocked = None;
        }
    }
}

fn state() -> std::sync::MutexGuard<'static, CircuitState> {
    STATE
        .get_or_init(|| Mutex::new(CircuitState::default()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub(super) fn surface_key(target: Option<&str>) -> String {
    if let Some(target) = target {
        return format!("target:{target}");
    }
    unsafe {
        let hwnd = GetForegroundWindow();
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        format!("foreground:{}:{pid}", hwnd.0 as usize)
    }
}

pub(super) fn remaining(surface: &str) -> Option<Duration> {
    state().remaining(surface, Instant::now())
}

pub(super) fn record_timeout(surface: &str) {
    state().timeout(surface, Instant::now());
}

pub(super) fn record_success(surface: &str) {
    state().success(surface);
}

#[derive(Debug)]
struct EnumerationTimeout;

impl fmt::Display for EnumerationTimeout {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UIA enumeration timed out")
    }
}

impl Error for EnumerationTimeout {}

pub(super) fn timeout_error() -> anyhow::Error {
    anyhow::Error::new(EnumerationTimeout)
}

pub(super) fn is_timeout(error: &anyhow::Error) -> bool {
    error.is::<EnumerationTimeout>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_blocks_only_the_same_surface_until_expiry() {
        let now = Instant::now();
        let mut circuit = CircuitState::default();
        circuit.timeout("one", now);
        assert_eq!(circuit.remaining("one", now), Some(COOLDOWN));
        assert_eq!(circuit.remaining("two", now), None);
        assert_eq!(circuit.remaining("one", now), None);

        circuit.timeout("one", now);
        assert_eq!(circuit.remaining("one", now + COOLDOWN), None);
    }

    #[test]
    fn success_closes_a_matching_circuit() {
        let now = Instant::now();
        let mut circuit = CircuitState::default();
        circuit.timeout("one", now);
        circuit.success("one");
        assert_eq!(circuit.remaining("one", now), None);
    }
}
