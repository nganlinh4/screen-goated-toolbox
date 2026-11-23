//! Monotonic native-window instance generations.
//!
//! HWND and PID identify a current window, but both can match again after a
//! destroy/recreate cycle. A process-wide WinEvent hook records those lifecycle
//! boundaries. Observations acquire the current generation; input edges must
//! validate the same generation after a hook-thread barrier.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::time::Duration;

use anyhow::{Result, anyhow};
use windows::Win32::Foundation::{
    CloseHandle, HANDLE, HWND, LPARAM, WAIT_OBJECT_0, WAIT_TIMEOUT, WPARAM,
};
use windows::Win32::System::Threading::{
    GetCurrentThreadId, OpenThread, THREAD_SYNCHRONIZE, WaitForSingleObject,
};
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, EVENT_OBJECT_CREATE, EVENT_OBJECT_DESTROY, GetMessageW,
    GetWindowThreadProcessId, IsWindow, MSG, OBJID_WINDOW, PM_NOREMOVE, PeekMessageW,
    PostThreadMessageW, TranslateMessage, WINEVENT_OUTOFCONTEXT, WM_APP,
};

const SYNC_MESSAGE: u32 = WM_APP + 0x53;
const START_TIMEOUT: Duration = Duration::from_secs(1);
const SYNC_TIMEOUT: Duration = Duration::from_millis(500);

mod lifecycle;
use lifecycle::{Lifecycle, Owner, Prepared, ThreadLease, ThreadState};

struct NativeThread(usize);
impl ThreadLease for NativeThread {
    fn state(&self) -> ThreadState {
        match unsafe { WaitForSingleObject(HANDLE(self.0 as *mut _), 0) } {
            WAIT_TIMEOUT => ThreadState::Alive,
            WAIT_OBJECT_0 => ThreadState::Exited,
            _ => ThreadState::Unknown,
        }
    }
}
impl Drop for NativeThread {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(HANDLE(self.0 as *mut _));
        }
    }
}
type TrackerHandle = Arc<Owner<NativeThread>>;

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
    exhausted: bool,
}

impl GenerationRegistry {
    fn allocate(&mut self) -> Result<u64> {
        let Some(next) = self.next.checked_add(1) else {
            self.exhausted = true;
            self.entries.clear();
            return Err(anyhow!("native window generation exhausted"));
        };
        self.next = next;
        Ok(next)
    }

    fn observe(&mut self, hwnd: u64, pid: u64) -> Result<u64> {
        if self.exhausted {
            return Err(anyhow!("native window generation exhausted"));
        }
        if let Some(entry) = self.entries.get(&hwnd)
            && entry.alive
            && entry.pid == pid
            && entry.observed
        {
            return Ok(entry.generation);
        }
        let generation = self.allocate()?;
        self.entries.insert(
            hwnd,
            Entry {
                pid,
                generation,
                alive: true,
                observed: true,
            },
        );
        Ok(generation)
    }

    fn note_create(&mut self, hwnd: u64, pid: u64) {
        if !self.entries.contains_key(&hwnd) {
            return;
        }
        let Ok(generation) = self.allocate() else {
            return;
        };
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

static TRACKER: OnceLock<Mutex<Lifecycle<NativeThread>>> = OnceLock::new();
static ACTIVE_HOOK: AtomicUsize = AtomicUsize::new(0);
static REGISTRY: OnceLock<Mutex<GenerationRegistry>> = OnceLock::new();
static WAITERS: OnceLock<Mutex<HashMap<u64, mpsc::SyncSender<()>>>> = OnceLock::new();
static NEXT_WAITER: AtomicU64 = AtomicU64::new(1);

fn registry() -> &'static Mutex<GenerationRegistry> {
    REGISTRY.get_or_init(|| Mutex::new(GenerationRegistry::default()))
}

fn waiters() -> &'static Mutex<HashMap<u64, mpsc::SyncSender<()>>> {
    WAITERS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn tracker_state() -> &'static Mutex<Lifecycle<NativeThread>> {
    TRACKER.get_or_init(|| Mutex::new(Lifecycle::new()))
}

fn tracker() -> Result<TrackerHandle> {
    tracker_state()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .acquire(
            START_TIMEOUT,
            start_tracker,
            || set_active_hook(0),
            set_active_hook,
        )
}

fn set_active_hook(hook: usize) {
    let mut state = registry().lock().unwrap_or_else(|e| e.into_inner());
    ACTIVE_HOOK.store(0, Ordering::SeqCst);
    state.entries.clear();
    ACTIVE_HOOK.store(hook, Ordering::SeqCst);
}

fn start_tracker() -> Result<mpsc::Receiver<Option<Prepared<NativeThread>>>> {
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("sgt-window-tracker".into())
        .spawn(move || unsafe {
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
            if hook.0.is_null() {
                let _ = ready_tx.send(None);
                return;
            }
            let Ok(thread) = OpenThread(THREAD_SYNCHRONIZE, false, thread_id) else {
                let _ = UnhookWinEvent(hook);
                let _ = ready_tx.send(None);
                return;
            };
            let (activate, activation) = mpsc::channel();
            let ready = Prepared {
                thread: NativeThread(thread.0 as usize),
                thread_id,
                hook: hook.0 as usize,
                activate,
            };
            if ready_tx.send(Some(ready)).is_err()
                || activation.recv_timeout(Duration::from_secs(2)).is_err()
            {
                let _ = UnhookWinEvent(hook);
                return;
            }
            while GetMessageW(&mut message, None, 0, 0).0 > 0 {
                if message.message == SYNC_MESSAGE {
                    signal_waiter(message.wParam.0 as u64);
                } else {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
            let _ = UnhookWinEvent(hook);
        })?;
    Ok(ready_rx)
}

fn sync_events() -> Result<TrackerHandle> {
    let handle = tracker()?;
    let token = NEXT_WAITER
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            value.checked_add(1)
        })
        .map_err(|_| anyhow!("native window tracker barrier generation exhausted"))?;
    let token_param = usize::try_from(token)
        .map_err(|_| anyhow!("native window tracker barrier is not representable"))?;
    let (tx, rx) = mpsc::sync_channel(1);
    waiters().lock().unwrap().insert(token, tx);
    if let Err(error) = unsafe {
        PostThreadMessageW(
            handle.thread_id,
            SYNC_MESSAGE,
            WPARAM(token_param),
            LPARAM(0),
        )
    } {
        waiters().lock().unwrap().remove(&token);
        return Err(error.into());
    }
    rx.recv_timeout(SYNC_TIMEOUT).map_err(|_| {
        waiters().lock().unwrap().remove(&token);
        anyhow!("native window lifecycle tracker did not reach the validation barrier")
    })?;
    with_owner(&handle, |_| Ok(()))?;
    Ok(handle)
}

fn with_owner<T>(
    owner: &TrackerHandle,
    apply: impl FnOnce(&mut GenerationRegistry) -> Result<T>,
) -> Result<T> {
    let lifecycle = tracker_state().lock().unwrap_or_else(|e| e.into_inner());
    if !lifecycle.owns(owner) {
        return Err(anyhow!("native window tracker was retired"));
    }
    let mut state = registry().lock().unwrap_or_else(|e| e.into_inner());
    if ACTIVE_HOOK.load(Ordering::SeqCst) != owner.hook
        || owner.thread.state() != ThreadState::Alive
    {
        return Err(anyhow!("native window tracker changed"));
    }
    let result = apply(&mut state)?;
    if owner.thread.state() != ThreadState::Alive {
        return Err(anyhow!("native window tracker exited during validation"));
    }
    Ok(result)
}

fn signal_waiter(token: u64) {
    if let Some(waiter) = waiters().lock().unwrap().remove(&token) {
        let _ = waiter.send(());
    }
}

unsafe extern "system" fn window_event(
    hook: windows::Win32::UI::Accessibility::HWINEVENTHOOK,
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
    let mut state = registry().lock().unwrap_or_else(|e| e.into_inner());
    let active_hook = ACTIVE_HOOK.load(Ordering::SeqCst);
    if active_hook == 0 || active_hook != hook.0 as usize {
        return;
    }
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
    let owner = sync_events()?;
    verify_window(hwnd, pid)?;
    with_owner(&owner, |state| state.observe(hwnd, pid))
}

pub(super) fn current(hwnd: u64, pid: u64) -> Result<u64> {
    let owner = sync_events()?;
    verify_window(hwnd, pid)?;
    with_owner(&owner, |state| {
        state
            .current(hwnd, pid)
            .ok_or_else(|| anyhow!("native window has no current observed generation"))
    })
}

pub(super) fn known(hwnd: u64, pid: u64) -> Option<u64> {
    let lifecycle = tracker_state().lock().unwrap_or_else(|e| e.into_inner());
    let owner = lifecycle.current.as_ref()?;
    if !lifecycle.owns(owner) {
        return None;
    }
    let state = registry().lock().unwrap_or_else(|e| e.into_inner());
    (ACTIVE_HOOK.load(Ordering::SeqCst) == owner.hook)
        .then(|| state.current(hwnd, pid))
        .flatten()
}

pub(super) fn validate(hwnd: u64, pid: u64, generation: u64) -> Result<()> {
    let owner = sync_events()?;
    verify_window(hwnd, pid)?;
    with_owner(&owner, |state| {
        if !state.validates(hwnd, pid, generation) {
            anyhow::bail!("native window was recreated after the action's observation");
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_observation_keeps_one_live_generation() {
        let mut registry = GenerationRegistry::default();
        let first = registry.observe(9, 4).unwrap();
        assert_eq!(registry.observe(9, 4).unwrap(), first);
        assert!(registry.validates(9, 4, first));
    }

    #[test]
    fn same_hwnd_and_pid_after_recreation_rejects_old_generation() {
        let mut registry = GenerationRegistry::default();
        let old = registry.observe(9, 4).unwrap();
        registry.note_destroy(9);
        registry.note_create(9, 4);
        assert!(!registry.validates(9, 4, old));
        assert_eq!(registry.current(9, 4), None);
        let fresh = registry.observe(9, 4).unwrap();
        assert_ne!(fresh, old);
        assert!(registry.validates(9, 4, fresh));
    }

    #[test]
    fn pid_change_cannot_inherit_a_generation() {
        let mut registry = GenerationRegistry::default();
        let old = registry.observe(9, 4).unwrap();
        let fresh = registry.observe(9, 7).unwrap();
        assert_ne!(fresh, old);
        assert!(!registry.validates(9, 4, old));
        assert!(registry.validates(9, 7, fresh));
    }

    #[test]
    fn hook_gap_invalidates_prior_observations_without_reusing_generation() {
        let mut registry = GenerationRegistry::default();
        let old = registry.observe(9, 4).unwrap();
        registry.entries.clear();
        assert!(!registry.validates(9, 4, old));
        let fresh = registry.observe(9, 4).unwrap();
        assert_ne!(fresh, old);
    }

    #[test]
    fn exhausted_window_generations_fail_closed() {
        let mut registry = GenerationRegistry {
            next: u64::MAX,
            ..GenerationRegistry::default()
        };
        assert!(registry.observe(9, 4).is_err());
        assert!(registry.observe(10, 4).is_err());
        assert!(registry.entries.is_empty());
    }

    #[test]
    #[ignore = "requires an interactive foreground window"]
    fn live_tracker_round_trips_an_existing_foreground_window() {
        let hwnd = unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() };
        assert!(
            !hwnd.0.is_null(),
            "an interactive foreground window is required"
        );
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        assert_ne!(pid, 0, "the foreground window must have a process");
        let raw = hwnd.0 as usize as u64;
        let pid = u64::from(pid);
        let generation = observe(raw, pid).unwrap();
        assert_eq!(current(raw, pid).unwrap(), generation);
        validate(raw, pid, generation).unwrap();
    }
}
