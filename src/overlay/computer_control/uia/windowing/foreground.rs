//! Foreground activation on a bounded worker.
//!
//! `AttachThreadInput` and `SetForegroundWindow` synchronize with the input
//! queue of the current foreground thread, and an unresponsive thread can make
//! them wait indefinitely. The attempt therefore runs on its own worker and the
//! caller waits at most `FOREGROUND_BUDGET`. A late attempt can still switch
//! the foreground after the caller gave up, so an expired wait reports an
//! unconfirmed effect, and no second attempt starts until the stranded one has
//! returned.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{RecvTimeoutError, sync_channel};
use std::time::Duration;

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowThreadProcessId, HWND_TOP, IsIconic, SW_RESTORE,
    SWP_ASYNCWINDOWPOS, SWP_NOMOVE, SWP_NOSIZE, SetForegroundWindow, SetWindowPos, ShowWindowAsync,
};

use super::{WindowError, WindowResult};

/// Three activation attempts with two 120 ms pauses normally finish well
/// inside this budget; it matches the placement budget that the shared
/// unconfirmed-effect message reports.
const FOREGROUND_BUDGET: Duration = super::VERIFY_BUDGET;
const OPERATION: &str = "focus";

/// Admits one foreground worker at a time, including one stranded behind an
/// unresponsive window after its caller stopped waiting.
pub(super) struct ForegroundGate {
    busy: AtomicBool,
}

impl ForegroundGate {
    pub(super) const fn new() -> Self {
        Self {
            busy: AtomicBool::new(false),
        }
    }
}

static GATE: ForegroundGate = ForegroundGate::new();

struct Admission(&'static ForegroundGate);

impl Drop for Admission {
    fn drop(&mut self) {
        self.0.busy.store(false, Ordering::Release);
    }
}

/// Raise `hwnd` and report whether it is verifiably the foreground window.
pub(super) fn raise(hwnd: HWND) -> WindowResult<bool> {
    run_bounded(&GATE, hwnd.0 as usize, FOREGROUND_BUDGET, |raw| unsafe {
        force_foreground_hwnd(HWND(raw as *mut _))
    })
}

fn run_bounded(
    gate: &'static ForegroundGate,
    raw_hwnd: usize,
    budget: Duration,
    attempt: impl FnOnce(usize) -> bool + Send + 'static,
) -> WindowResult<bool> {
    if gate
        .busy
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        // The previous request has not returned and may still apply.
        return Err(unconfirmed());
    }
    let admission = Admission(gate);
    let (sender, receiver) = sync_channel(1);
    let worker = crate::overlay::computer_control::session_retention::inherit(move || {
        // Released before the result is visible, and on unwind.
        let admission = admission;
        let raised = attempt(raw_hwnd);
        drop(admission);
        let _ = sender.send(raised);
    });
    if std::thread::Builder::new()
        .name("cc-foreground".to_string())
        .spawn(worker)
        .is_err()
    {
        // The closure (and its admission) was dropped without running.
        return Ok(false);
    }
    match receiver.recv_timeout(budget) {
        Ok(raised) => Ok(raised),
        Err(RecvTimeoutError::Timeout) => Err(unconfirmed()),
        // The attempt panicked after possibly calling into Win32.
        Err(RecvTimeoutError::Disconnected) => Err(unconfirmed()),
    }
}

fn unconfirmed() -> WindowError {
    WindowError::Unconfirmed {
        operation: OPERATION,
        observed: None,
    }
}

/// Force `hwnd` to the foreground using the `AttachThreadInput` trick (a
/// background process otherwise can't legally call `SetForegroundWindow`),
/// restoring it if minimized. Retries briefly and verifies. Restore and
/// z-order requests are posted so a window that is not pumping cannot block
/// the worker; focus inside the window is left to its own activation handling.
unsafe fn force_foreground_hwnd(hwnd: HWND) -> bool {
    unsafe {
        let this_tid = GetCurrentThreadId();
        for attempt in 0..3 {
            if GetForegroundWindow().0 == hwnd.0 {
                return true;
            }
            // Only un-minimize. SW_RESTORE changes a maximized window's placement,
            // while on a minimized window it restores the prior maximized/normal state.
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindowAsync(hwnd, SW_RESTORE);
            }
            let fg_tid = GetWindowThreadProcessId(GetForegroundWindow(), None);
            let attach = fg_tid != 0 && fg_tid != this_tid;
            if attach {
                let _ = AttachThreadInput(this_tid, fg_tid, true);
            }
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOP),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_ASYNCWINDOWPOS,
            );
            let _ = SetForegroundWindow(hwnd);
            if attach {
                let _ = AttachThreadInput(this_tid, fg_tid, false);
            }
            if GetForegroundWindow().0 == hwnd.0 {
                return true;
            }
            if attempt < 2 {
                std::thread::sleep(std::time::Duration::from_millis(120));
            }
        }
        GetForegroundWindow().0 == hwnd.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;
    use std::time::Instant;

    #[test]
    fn a_completed_attempt_returns_its_verified_result() {
        static TEST_GATE: ForegroundGate = ForegroundGate::new();
        assert_eq!(
            run_bounded(&TEST_GATE, 7, Duration::from_secs(5), |raw| raw == 7),
            Ok(true)
        );
        assert_eq!(
            run_bounded(&TEST_GATE, 8, Duration::from_secs(5), |_| false),
            Ok(false)
        );
    }

    #[test]
    fn a_hung_attempt_is_unconfirmed_within_the_budget_and_blocks_a_second_attempt() {
        static TEST_GATE: ForegroundGate = ForegroundGate::new();
        let (release, hung) = channel::<()>();
        let started = Instant::now();
        let result = run_bounded(&TEST_GATE, 1, Duration::from_millis(100), move |_| {
            let _ = hung.recv();
            true
        });
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(matches!(
            result,
            Err(WindowError::Unconfirmed {
                operation: "focus",
                observed: None
            })
        ));

        let (ran, ran_rx) = channel::<()>();
        let second = run_bounded(&TEST_GATE, 1, Duration::from_millis(100), move |_| {
            let _ = ran.send(());
            true
        });
        assert!(matches!(second, Err(WindowError::Unconfirmed { .. })));
        assert!(ran_rx.try_recv().is_err(), "a second worker must not start");

        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while TEST_GATE.busy.load(Ordering::Acquire) {
            assert!(Instant::now() < deadline, "stranded worker never returned");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            run_bounded(&TEST_GATE, 1, Duration::from_secs(5), |_| true),
            Ok(true)
        );
    }

    #[test]
    fn a_panicking_attempt_is_unconfirmed_and_releases_the_gate() {
        static TEST_GATE: ForegroundGate = ForegroundGate::new();
        let result = run_bounded(&TEST_GATE, 1, Duration::from_secs(5), |_| {
            panic!("simulated Win32 failure")
        });
        assert!(matches!(result, Err(WindowError::Unconfirmed { .. })));
        assert!(!TEST_GATE.busy.load(Ordering::Acquire));
    }
}
