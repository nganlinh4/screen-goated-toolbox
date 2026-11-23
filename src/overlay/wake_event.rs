use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};

use anyhow::{Context, Result, bail};
use windows::Win32::Foundation::{HANDLE, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{CreateEventW, INFINITE, SetEvent};
use windows::Win32::UI::WindowsAndMessaging::{
    MWMO_INPUTAVAILABLE, MsgWaitForMultipleObjectsEx, QS_ALLINPUT,
};
use windows::core::PCWSTR;

/// Coalesced notifications for a thread that also owns a Windows message queue.
pub(crate) struct WakeEvent(OwnedHandle);

impl WakeEvent {
    pub(crate) fn new() -> Result<Self> {
        let handle = unsafe { CreateEventW(None, false, false, PCWSTR::null()) }
            .context("create thread wake event")?;
        Ok(Self(unsafe { OwnedHandle::from_raw_handle(handle.0) }))
    }

    pub(crate) fn signal(&self) {
        // The event remains owned through this call and pending signals survive
        // until the next wait. Multiple notifications may coalesce.
        unsafe {
            let _ = SetEvent(HANDLE(self.0.as_raw_handle()));
        }
    }

    pub(crate) fn wait(&self) -> Result<()> {
        let result = unsafe {
            MsgWaitForMultipleObjectsEx(
                Some(&[HANDLE(self.0.as_raw_handle())]),
                INFINITE,
                QS_ALLINPUT,
                MWMO_INPUTAVAILABLE,
            )
        };
        if result == WAIT_OBJECT_0 || result.0 == WAIT_OBJECT_0.0 + 1 {
            Ok(())
        } else {
            bail!(
                "thread message/event wait failed: {} ({})",
                result.0,
                windows::core::Error::from_thread()
            );
        }
    }

    #[cfg(test)]
    pub(crate) fn signalled_within(&self, milliseconds: u32) -> bool {
        unsafe {
            windows::Win32::System::Threading::WaitForSingleObject(
                HANDLE(self.0.as_raw_handle()),
                milliseconds,
            ) == WAIT_OBJECT_0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, mpsc};
    use std::time::Duration;
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        MSG, PM_NOREMOVE, PM_REMOVE, PeekMessageW, PostThreadMessageW, WM_APP,
    };

    #[test]
    fn notifications_before_wait_are_preserved_and_coalesced() {
        let wake = WakeEvent::new().unwrap();
        wake.signal();
        wake.signal();
        assert!(wake.signalled_within(0));
        assert!(!wake.signalled_within(0));
    }

    #[test]
    fn an_idle_thread_blocks_until_a_notification() {
        let wake = Arc::new(WakeEvent::new().unwrap());
        let waiting = Arc::clone(&wake);
        let (entered, started) = mpsc::channel();
        let (done, finished) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            entered.send(()).unwrap();
            waiting.wait().unwrap();
            done.send(()).unwrap();
        });
        started.recv().unwrap();
        assert!(finished.recv_timeout(Duration::from_millis(40)).is_err());
        wake.signal();
        finished.recv_timeout(Duration::from_secs(1)).unwrap();
        thread.join().unwrap();
    }

    #[test]
    fn an_already_observed_windows_message_still_wakes_the_wait() {
        let thread = std::thread::spawn(|| {
            let wake = WakeEvent::new().unwrap();
            unsafe {
                let mut message = MSG::default();
                let _ = PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE);
                PostThreadMessageW(
                    windows::Win32::System::Threading::GetCurrentThreadId(),
                    WM_APP,
                    WPARAM(0),
                    LPARAM(0),
                )
                .unwrap();
                assert!(PeekMessageW(&mut message, None, WM_APP, WM_APP, PM_NOREMOVE).as_bool());
                wake.wait().unwrap();
                assert!(PeekMessageW(&mut message, None, WM_APP, WM_APP, PM_REMOVE).as_bool());
            }
        });
        thread.join().unwrap();
    }
}
