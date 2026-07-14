//! Monotonic native-window instance generations.
//!
//! HWND and PID identify a current window, but both can match again after a
//! destroy/recreate cycle. A process-wide WinEvent hook records those lifecycle
//! boundaries. Observations acquire the current generation; input edges must
//! validate the same generation after a hook-thread barrier.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock, mpsc};
use std::time::Duration;

use anyhow::{Result, anyhow};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, EVENT_OBJECT_CREATE, EVENT_OBJECT_DESTROY, GetMessageW,
    GetWindowThreadProcessId, IsWindow, MSG, OBJID_WINDOW, PM_NOREMOVE, PeekMessageW,
    PostThreadMessageW, TranslateMessage, WINEVENT_OUTOFCONTEXT, WM_APP,
};

const SYNC_MESSAGE: u32 = WM_APP + 0x53;
const START_TIMEOUT: Duration = Duration::from_secs(1);
const SYNC_TIMEOUT: Duration = Duration::from_millis(500);

#[derive(Clone, Copy)]
struct TrackerHandle {
    thread_id: u32,
}

#[derive(Clone, Copy, Debug)]
struct Entry {
    pid: u64,
    generation: u64,
    alive: bool,
    observed: bool,
}

#[derive(Default)]
struct GenerationRegistry {
    next: u64,
    entries: HashMap<u64, Entry>,
}

impl GenerationRegistry {
    fn allocate(&mut self) -> u64 {
        self.next = self.next.saturating_add(1).max(1);
        self.next
    }

    fn observe(&mut self, hwnd: u64, pid: u64) -> u64 {
        if let Some(entry) = self.entries.get(&hwnd)
            && entry.alive
            && entry.pid == pid
            && entry.observed
        {
            return entry.generation;
        }
        let generation = self.allocate();
        self.entries.insert(
            hwnd,
            Entry {
                pid,
                generation,
                alive: true,
                observed: true,
            },
        );
        generation
    }

    fn note_create(&mut self, hwnd: u64, pid: u64) {
        if !self.entries.contains_key(&hwnd) {
            return;
        }
        let generation = self.allocate();
        self.entries.insert(
            hwnd,
            Entry {
                pid,
                generation,
                alive: true,
                observed: false,
            },
        );
    }

    fn note_destroy(&mut self, hwnd: u64) {
        if let Some(entry) = self.entries.get_mut(&hwnd) {
            entry.alive = false;
        }
    }

    fn current(&self, hwnd: u64, pid: u64) -> Option<u64> {
        self.entries
            .get(&hwnd)
            .filter(|entry| entry.alive && entry.observed && entry.pid == pid)
            .map(|entry| entry.generation)
    }

    fn validates(&self, hwnd: u64, pid: u64, generation: u64) -> bool {
        self.current(hwnd, pid) == Some(generation)
    }
}

static TRACKER: OnceLock<Option<TrackerHandle>> = OnceLock::new();
static REGISTRY: OnceLock<Mutex<GenerationRegistry>> = OnceLock::new();
static WAITERS: OnceLock<Mutex<HashMap<u64, mpsc::SyncSender<()>>>> = OnceLock::new();
static NEXT_WAITER: AtomicU64 = AtomicU64::new(1);

fn registry() -> &'static Mutex<GenerationRegistry> {
    REGISTRY.get_or_init(|| Mutex::new(GenerationRegistry::default()))
}

fn waiters() -> &'static Mutex<HashMap<u64, mpsc::SyncSender<()>>> {
    WAITERS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn tracker() -> Result<TrackerHandle> {
    TRACKER
        .get_or_init(start_tracker)
        .as_ref()
        .copied()
        .ok_or_else(|| anyhow!("native window lifecycle tracker is unavailable"))
}

fn start_tracker() -> Option<TrackerHandle> {
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    std::thread::spawn(move || unsafe {
        let thread_id = GetCurrentThreadId();
        let mut message = MSG::default();
        let _ = PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE);
        let hook = SetWinEventHook(
            EVENT_OBJECT_CREATE,
            EVENT_OBJECT_DESTROY,
            None,
            Some(window_event),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );
        let ready = !hook.0.is_null();
        let _ = ready_tx.send(ready.then_some(TrackerHandle { thread_id }));
        if !ready {
            return;
        }
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            if message.message == SYNC_MESSAGE {
                signal_waiter(message.wParam.0 as u64);
            } else {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        let _ = UnhookWinEvent(hook);
    });
    ready_rx.recv_timeout(START_TIMEOUT).ok().flatten()
}

fn sync_events() -> Result<()> {
    let handle = tracker()?;
    let token = NEXT_WAITER.fetch_add(1, Ordering::Relaxed).max(1);
    let (tx, rx) = mpsc::sync_channel(1);
    waiters().lock().unwrap().insert(token, tx);
    if let Err(error) = unsafe {
        PostThreadMessageW(
            handle.thread_id,
            SYNC_MESSAGE,
            WPARAM(token as usize),
            LPARAM(0),
        )
    } {
        waiters().lock().unwrap().remove(&token);
        return Err(error.into());
    }
    rx.recv_timeout(SYNC_TIMEOUT).map_err(|_| {
        waiters().lock().unwrap().remove(&token);
        anyhow!("native window lifecycle tracker did not reach the validation barrier")
    })
}

fn signal_waiter(token: u64) {
    if let Some(waiter) = waiters().lock().unwrap().remove(&token) {
        let _ = waiter.send(());
    }
}

unsafe extern "system" fn window_event(
    _hook: windows::Win32::UI::Accessibility::HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    object_id: i32,
    child_id: i32,
    _event_thread: u32,
    _event_time: u32,
) {
    if hwnd.0.is_null() || object_id != OBJID_WINDOW.0 || child_id != 0 {
        return;
    }
    let raw = hwnd.0 as usize as u64;
    let mut state = registry().lock().unwrap();
    if event == EVENT_OBJECT_DESTROY {
        state.note_destroy(raw);
    } else if event == EVENT_OBJECT_CREATE {
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        if pid != 0 {
            state.note_create(raw, u64::from(pid));
        }
    }
}

fn verify_window(hwnd: u64, expected_pid: u64) -> Result<()> {
    let handle = HWND(hwnd as usize as *mut _);
    let mut actual_pid = 0u32;
    if unsafe { IsWindow(Some(handle)).as_bool() } {
        unsafe { GetWindowThreadProcessId(handle, Some(&mut actual_pid)) };
    }
    if u64::from(actual_pid) != expected_pid {
        anyhow::bail!(
            "native window identity is stale; expected HWND/PID {hwnd}/{expected_pid}, got PID {actual_pid}"
        );
    }
    Ok(())
}

pub(super) fn observe(hwnd: u64, pid: u64) -> Result<u64> {
    sync_events()?;
    verify_window(hwnd, pid)?;
    Ok(registry().lock().unwrap().observe(hwnd, pid))
}

pub(super) fn current(hwnd: u64, pid: u64) -> Result<u64> {
    sync_events()?;
    verify_window(hwnd, pid)?;
    registry()
        .lock()
        .unwrap()
        .current(hwnd, pid)
        .ok_or_else(|| anyhow!("native window has no current observed generation"))
}

pub(super) fn known(hwnd: u64, pid: u64) -> Option<u64> {
    registry().lock().unwrap().current(hwnd, pid)
}

pub(super) fn validate(hwnd: u64, pid: u64, generation: u64) -> Result<()> {
    sync_events()?;
    verify_window(hwnd, pid)?;
    if !registry().lock().unwrap().validates(hwnd, pid, generation) {
        anyhow::bail!("native window was recreated after the action's observation");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_observation_keeps_one_live_generation() {
        let mut registry = GenerationRegistry::default();
        let first = registry.observe(9, 4);
        assert_eq!(registry.observe(9, 4), first);
        assert!(registry.validates(9, 4, first));
    }

    #[test]
    fn same_hwnd_and_pid_after_recreation_rejects_old_generation() {
        let mut registry = GenerationRegistry::default();
        let old = registry.observe(9, 4);
        registry.note_destroy(9);
        registry.note_create(9, 4);
        assert!(!registry.validates(9, 4, old));
        assert_eq!(registry.current(9, 4), None);
        let fresh = registry.observe(9, 4);
        assert_ne!(fresh, old);
        assert!(registry.validates(9, 4, fresh));
    }

    #[test]
    fn pid_change_cannot_inherit_a_generation() {
        let mut registry = GenerationRegistry::default();
        let old = registry.observe(9, 4);
        let fresh = registry.observe(9, 7);
        assert_ne!(fresh, old);
        assert!(!registry.validates(9, 4, old));
        assert!(registry.validates(9, 7, fresh));
    }

    #[test]
    fn live_tracker_round_trips_an_existing_foreground_window() {
        let hwnd = unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() };
        if hwnd.0.is_null() {
            return;
        }
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        if pid == 0 {
            return;
        }
        let raw = hwnd.0 as usize as u64;
        let pid = u64::from(pid);
        let generation = observe(raw, pid).unwrap();
        assert_eq!(current(raw, pid).unwrap(), generation);
        validate(raw, pid, generation).unwrap();
    }
}
