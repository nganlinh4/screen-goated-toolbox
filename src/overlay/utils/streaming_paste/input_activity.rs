//! Observe edit-affecting input without retaining keys, coordinates, or text.

use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc,
};
use std::time::Duration;
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::*;

pub(super) const INSERTION_TAG: usize = 0x53475450;
static EPOCH: AtomicU64 = AtomicU64::new(0);

pub(super) fn epoch() -> u64 {
    EPOCH.load(Ordering::SeqCst)
}

pub(super) struct InputWatch {
    thread: u32,
    stopped: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl InputWatch {
    pub(super) fn start() -> anyhow::Result<Self> {
        let (send, ready) = mpsc::channel();
        let stopped = Arc::new(AtomicBool::new(false));
        let worker_stop = stopped.clone();
        let worker = std::thread::Builder::new()
            .name("auto-paste-input-watch".into())
            .spawn(move || {
                let result = unsafe { install_hooks() };
                let Ok(hooks) = result else {
                    let _ = send.send(None);
                    return;
                };
                let id = unsafe { GetCurrentThreadId() };
                let timer = unsafe { SetTimer(None, 0, 50, None) };
                if timer == 0 {
                    let _ = send.send(None);
                    return;
                }
                let _timer = WakeTimer(timer);
                // Ensure the thread message queue exists before publishing its ID.
                let mut message = MSG::default();
                unsafe {
                    let _ = PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE);
                }
                if send.send(Some(id)).is_err() {
                    return;
                }
                unsafe {
                    while !worker_stop.load(Ordering::SeqCst)
                        && GetMessageW(&mut message, None, 0, 0).0 > 0
                    {
                        let _ = TranslateMessage(&message);
                        DispatchMessageW(&message);
                    }
                }
                drop(hooks);
            })?;
        match ready.recv_timeout(Duration::from_secs(2)) {
            Ok(Some(thread)) => Ok(Self {
                thread,
                stopped,
                worker: Some(worker),
            }),
            _ => {
                stopped.store(true, Ordering::SeqCst);
                anyhow::bail!("input activity observation unavailable")
            }
        }
    }
}

impl Drop for InputWatch {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
        let _ = unsafe { PostThreadMessageW(self.thread, WM_QUIT, WPARAM(0), LPARAM(0)) };
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

struct WakeTimer(usize);
impl Drop for WakeTimer {
    fn drop(&mut self) {
        unsafe {
            let _ = KillTimer(None, self.0);
        }
    }
}

struct Hooks(HHOOK, HHOOK);
impl Drop for Hooks {
    fn drop(&mut self) {
        unsafe {
            let _ = UnhookWindowsHookEx(self.0);
            let _ = UnhookWindowsHookEx(self.1);
        }
    }
}

unsafe fn install_hooks() -> anyhow::Result<Hooks> {
    unsafe {
        let module = GetModuleHandleW(None)?;
        let keyboard =
            SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), Some(module.into()), 0)?;
        match SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), Some(module.into()), 0) {
            Ok(mouse) => Ok(Hooks(keyboard, mouse)),
            Err(error) => {
                let _ = UnhookWindowsHookEx(keyboard);
                Err(error.into())
            }
        }
    }
}

unsafe extern "system" fn keyboard_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        if code == HC_ACTION as i32 {
            let event = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
            if affects_ownership(true, wparam.0 as u32, event.dwExtraInfo) {
                EPOCH.fetch_add(1, Ordering::SeqCst);
            }
        }
        CallNextHookEx(None, code, wparam, lparam)
    }
}

unsafe extern "system" fn mouse_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        if code == HC_ACTION as i32 {
            let event = &*(lparam.0 as *const MSLLHOOKSTRUCT);
            if affects_ownership(false, wparam.0 as u32, event.dwExtraInfo) {
                EPOCH.fetch_add(1, Ordering::SeqCst);
            }
        }
        CallNextHookEx(None, code, wparam, lparam)
    }
}

fn affects_ownership(keyboard: bool, message: u32, tag: usize) -> bool {
    // Releasing the shortcut that started capture is not a new edit request.
    tag != INSERTION_TAG
        && if keyboard {
            matches!(message, WM_KEYDOWN | WM_SYSKEYDOWN)
        } else {
            matches!(
                message,
                WM_LBUTTONDOWN
                    | WM_RBUTTONDOWN
                    | WM_MBUTTONDOWN
                    | WM_XBUTTONDOWN
                    | WM_MOUSEWHEEL
                    | WM_MOUSEHWHEEL
            )
        }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pointer_motion_and_own_input_do_not_cancel_dictation() {
        assert!(!affects_ownership(false, WM_MOUSEMOVE, 0));
        assert!(!affects_ownership(true, WM_KEYDOWN, INSERTION_TAG));
        assert!(affects_ownership(true, WM_KEYDOWN, 0));
        assert!(!affects_ownership(true, WM_KEYUP, 123));
        assert!(affects_ownership(true, WM_KEYDOWN, 123));
        assert!(!affects_ownership(false, WM_LBUTTONUP, 0));
        assert!(affects_ownership(false, WM_LBUTTONDOWN, 0));
        assert!(affects_ownership(false, WM_MOUSEWHEEL, 0));
    }
}
