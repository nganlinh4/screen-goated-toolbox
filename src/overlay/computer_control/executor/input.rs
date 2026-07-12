//! Truthful `SendInput` dispatch and per-action telemetry.
//!
//! Win32 returns the number of events it actually inserted. Treating the call as
//! fire-and-forget can report success while no input reached the desktop, or can
//! leave a key/button held when only a prefix of a batch was accepted.

use std::cell::RefCell;
use std::fmt;
use std::thread::sleep;
use std::time::Duration;

use serde_json::{Value, json};
use windows::Win32::Foundation::{GetLastError, SetLastError, WIN32_ERROR};
use windows::Win32::UI::Input::KeyboardAndMouse::{INPUT, SendInput};

const RELEASE_ATTEMPTS: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SendInputError {
    pub requested: u32,
    pub inserted: u32,
    pub last_error: u32,
}

impl SendInputError {
    pub(super) fn status(self) -> &'static str {
        if self.inserted == 0 {
            "input_injection_failed"
        } else {
            "input_injection_partial"
        }
    }
}

impl fmt::Display for SendInputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SendInput inserted {}/{} events (Win32 last error {})",
            self.inserted, self.requested, self.last_error
        )
    }
}

impl std::error::Error for SendInputError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SendReceipt {
    requested: u32,
    inserted: u32,
    last_error: u32,
}

#[derive(Clone, Copy, Debug)]
enum SendPhase {
    Action,
    Cleanup,
}

#[derive(Clone, Copy, Debug, Default)]
struct PhaseTelemetry {
    calls: u64,
    requested: u64,
    inserted: u64,
    last_error: u32,
    failed_calls: u64,
    last_failure: Option<SendInputError>,
}

impl PhaseTelemetry {
    fn record(&mut self, receipt: SendReceipt) {
        self.calls += 1;
        self.requested += u64::from(receipt.requested);
        self.inserted += u64::from(receipt.inserted);
        self.last_error = receipt.last_error;
        if receipt.inserted != receipt.requested {
            self.failed_calls += 1;
            self.last_failure = Some(SendInputError {
                requested: receipt.requested,
                inserted: receipt.inserted,
                last_error: receipt.last_error,
            });
        }
    }

    fn to_json(self) -> Value {
        let mut value = json!({
            "calls": self.calls,
            "requested": self.requested,
            "inserted": self.inserted,
            "last_error": self.last_error,
            "failed_calls": self.failed_calls,
            "fully_inserted": self.failed_calls == 0 && self.inserted == self.requested,
        });
        if let Some(failure) = self.last_failure {
            value["last_failure"] = json!({
                "requested": failure.requested,
                "inserted": failure.inserted,
                "last_error": failure.last_error,
            });
        }
        value
    }
}

#[derive(Clone, Debug, Default)]
struct InputTelemetry {
    action: PhaseTelemetry,
    cleanup: PhaseTelemetry,
    target: Value,
}

impl InputTelemetry {
    fn record(&mut self, phase: SendPhase, receipt: SendReceipt) {
        match phase {
            SendPhase::Action => self.action.record(receipt),
            SendPhase::Cleanup => self.cleanup.record(receipt),
        }
    }

    fn into_json(self) -> Option<Value> {
        if self.action.calls == 0 && self.cleanup.calls == 0 {
            return None;
        }
        let mut value = self.action.to_json();
        value["input_target"] = self.target;
        if self.cleanup.calls > 0 {
            value["cleanup"] = self.cleanup.to_json();
        }
        Some(value)
    }
}

thread_local! {
    static TELEMETRY: RefCell<Option<InputTelemetry>> = const { RefCell::new(None) };
}

pub(super) fn begin_action() {
    TELEMETRY.with(|slot| *slot.borrow_mut() = Some(InputTelemetry::default()));
}

pub(super) fn finish_action() -> Option<Value> {
    TELEMETRY.with(|slot| slot.borrow_mut().take().and_then(InputTelemetry::into_json))
}

fn record(phase: SendPhase, receipt: SendReceipt) {
    TELEMETRY.with(|slot| {
        if let Some(telemetry) = slot.borrow_mut().as_mut() {
            telemetry.record(phase, receipt);
        }
    });
}

fn evaluate(receipt: SendReceipt) -> Result<(), SendInputError> {
    if receipt.inserted == receipt.requested {
        Ok(())
    } else {
        Err(SendInputError {
            requested: receipt.requested,
            inserted: receipt.inserted,
            last_error: receipt.last_error,
        })
    }
}

fn inject(inputs: &[INPUT], phase: SendPhase) -> Result<(), SendInputError> {
    if inputs.is_empty() {
        return Ok(());
    }
    let requested = inputs.len() as u32;
    TELEMETRY.with(|slot| {
        if let Some(telemetry) = slot.borrow_mut().as_mut()
            && telemetry.target.is_null()
        {
            telemetry.target = super::super::uia::input_target_snapshot();
        }
    });
    let (inserted, last_error) = unsafe {
        // A successful Win32 call does not promise to clear the thread's prior
        // error. Reset it so the recorded value belongs to this dispatch.
        SetLastError(WIN32_ERROR(0));
        let inserted = SendInput(inputs, std::mem::size_of::<INPUT>() as i32);
        (inserted, GetLastError().0)
    };
    let receipt = SendReceipt {
        requested,
        inserted,
        last_error,
    };
    record(phase, receipt);
    evaluate(receipt)
}

pub(super) fn send(inputs: &[INPUT]) -> Result<(), SendInputError> {
    inject(inputs, SendPhase::Action)
}

/// Best-effort release after a failed/partial dispatch. Repeating key-up or
/// button-up is harmless, so retry the complete release set rather than guessing
/// which prefix Win32 accepted.
pub(super) fn release(inputs: &[INPUT]) -> Result<(), SendInputError> {
    let mut last_failure = None;
    for attempt in 0..RELEASE_ATTEMPTS {
        match inject(inputs, SendPhase::Cleanup) {
            Ok(()) => return Ok(()),
            Err(error) => last_failure = Some(error),
        }
        if attempt + 1 < RELEASE_ATTEMPTS {
            sleep(Duration::from_millis(1));
        }
    }
    Err(last_failure.expect("non-empty releases always produce a result"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_insertion_is_success() {
        assert_eq!(
            evaluate(SendReceipt {
                requested: 4,
                inserted: 4,
                last_error: 0,
            }),
            Ok(())
        );
    }

    #[test]
    fn partial_and_zero_insertion_are_failures_even_without_last_error() {
        let partial = evaluate(SendReceipt {
            requested: 4,
            inserted: 2,
            last_error: 5,
        })
        .unwrap_err();
        assert_eq!(partial.status(), "input_injection_partial");
        assert_eq!(partial.inserted, 2);

        let none = evaluate(SendReceipt {
            requested: 1,
            inserted: 0,
            last_error: 0,
        })
        .unwrap_err();
        assert_eq!(none.status(), "input_injection_failed");
        assert_eq!(none.last_error, 0);
    }

    #[test]
    fn telemetry_keeps_action_and_cleanup_counts_separate() {
        let mut telemetry = InputTelemetry::default();
        telemetry.record(
            SendPhase::Action,
            SendReceipt {
                requested: 3,
                inserted: 1,
                last_error: 5,
            },
        );
        telemetry.record(
            SendPhase::Cleanup,
            SendReceipt {
                requested: 2,
                inserted: 2,
                last_error: 0,
            },
        );

        let value = telemetry.into_json().unwrap();
        assert_eq!(value["requested"], 3);
        assert_eq!(value["inserted"], 1);
        assert_eq!(value["last_error"], 5);
        assert_eq!(value["cleanup"]["requested"], 2);
        assert_eq!(value["cleanup"]["inserted"], 2);
    }
}
