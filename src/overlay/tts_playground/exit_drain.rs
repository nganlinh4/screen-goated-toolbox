//! Drain edits already posted by the page before an application exit.

use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, mpsc};
use std::time::{Duration, Instant};

use serde_json::Value;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{GetWindowThreadProcessId, PostMessageW};

use super::{WINDOW_LIFECYCLE, WM_APP_EXIT_DRAIN, WM_APP_EXIT_THAW};

const TIMEOUT: Duration = Duration::from_secs(3);
const MAX_REQUEST: usize = if usize::BITS > 53 {
    ((1u64 << 53) - 1) as usize
} else {
    usize::MAX
};
static NEXT_REQUEST: AtomicUsize = AtomicUsize::new(1);
static PENDING: Mutex<Option<Pending>> = Mutex::new(None);

struct Pending {
    hwnd: isize,
    generation: usize,
    document: usize,
    request: usize,
    sender: mpsc::SyncSender<()>,
}

/// Owns the closed admission and reverses the page freeze if exit is abandoned.
pub(crate) struct ExitDrainGuard {
    target: Option<(isize, usize, usize, usize)>,
}

impl Drop for ExitDrainGuard {
    fn drop(&mut self) {
        if let Some((hwnd, generation, document, request)) = self.target {
            let _admission = super::REMOVAL_ADMISSION
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let current = WINDOW_LIFECYCLE
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .is_ready_document(hwnd, generation, document);
            if current {
                let _ = unsafe {
                    PostMessageW(
                        Some(HWND(hwnd as *mut std::ffi::c_void)),
                        WM_APP_EXIT_THAW,
                        WPARAM(request),
                        LPARAM(generation as isize),
                    )
                };
            }
        }
        WINDOW_LIFECYCLE
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .end_exit();
    }
}

pub(super) fn drain() -> io::Result<ExitDrainGuard> {
    {
        let mut lifecycle = WINDOW_LIFECYCLE
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if !lifecycle.begin_exit() {
            return Err(io::Error::other("TTS Playground is changing state"));
        }
    }
    let mut guard = ExitDrainGuard { target: None };
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let observation = WINDOW_LIFECYCLE
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .exit_observation();
        match observation {
            Ok(None) => return Ok(guard),
            Ok(Some((hwnd, generation, document))) => {
                let window = HWND(hwnd as *mut std::ffi::c_void);
                if unsafe { GetWindowThreadProcessId(window, None) == GetCurrentThreadId() } {
                    return Err(io::Error::other(
                        "cannot drain TTS edits on its own window thread",
                    ));
                }
                let request = NEXT_REQUEST
                    .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| {
                        (id < MAX_REQUEST).then_some(id + 1)
                    })
                    .map_err(|_| io::Error::other("TTS exit request identity exhausted"))?;
                let (sender, receiver) = mpsc::sync_channel(1);
                {
                    let mut pending = PENDING.lock().unwrap_or_else(|error| error.into_inner());
                    if pending.is_some() {
                        return Err(io::Error::other("TTS exit drain already active"));
                    }
                    *pending = Some(Pending {
                        hwnd,
                        generation,
                        document,
                        request,
                        sender,
                    });
                }
                guard.target = Some((hwnd, generation, document, request));
                let posted = {
                    let _admission = super::REMOVAL_ADMISSION
                        .lock()
                        .unwrap_or_else(|error| error.into_inner());
                    let current = WINDOW_LIFECYCLE
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .is_ready_document(hwnd, generation, document);
                    if !current {
                        clear_pending(request);
                        return Err(io::Error::other("TTS window changed before edit drain"));
                    }
                    unsafe {
                        PostMessageW(
                            Some(window),
                            WM_APP_EXIT_DRAIN,
                            WPARAM(request),
                            LPARAM(generation as isize),
                        )
                    }
                };
                if let Err(error) = posted {
                    clear_pending(request);
                    return Err(io::Error::other(format!(
                        "could not request TTS edit drain: {error}"
                    )));
                }
                let result = wait_for_ack(
                    &receiver,
                    deadline.saturating_duration_since(Instant::now()),
                );
                clear_pending(request);
                return result.map(|()| guard);
            }
            Err(()) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            Err(()) => return Err(io::Error::other("TTS window did not settle before exit")),
        }
    }
}

fn wait_for_ack(receiver: &mpsc::Receiver<()>, timeout: Duration) -> io::Result<()> {
    receiver.recv_timeout(timeout).map_err(|error| match error {
        mpsc::RecvTimeoutError::Timeout => io::Error::other("TTS edit drain timed out"),
        mpsc::RecvTimeoutError::Disconnected => {
            io::Error::other("TTS edit drain lost its acknowledgement")
        }
    })
}

fn clear_pending(request: usize) {
    let mut pending = PENDING.lock().unwrap_or_else(|error| error.into_inner());
    if pending
        .as_ref()
        .is_some_and(|entry| entry.request == request)
    {
        *pending = None;
    }
}

pub(super) fn acknowledge(hwnd: HWND, args: &Value) {
    let Some(request) = args
        .get("request")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
    else {
        return;
    };
    let mut pending = PENDING.lock().unwrap_or_else(|error| error.into_inner());
    if pending.as_ref().is_some_and(|entry| {
        entry.request == request
            && entry.hwnd == hwnd.0 as isize
            && WINDOW_LIFECYCLE
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .is_ready_document(entry.hwnd, entry.generation, entry.document)
    }) && let Some(entry) = pending.take()
    {
        let _ = entry.sender.try_send(());
    }
}

/// Events that can change the draft while an exit barrier holds the page.
const EDIT_EVENTS: &str = r#"['beforeinput', 'input', 'keydown', 'keypress', 'paste', 'cut', 'drop', 'compositionstart']"#;

fn barrier_script(request: usize) -> String {
    format!(
        r#"(() => {{
        const id = {request};
        const begin = () => {{
            if ((window.__sgtTtsExitCancelled || 0) >= id) return;
            if (!document.body) {{ document.addEventListener('DOMContentLoaded', begin, {{ once: true }}); return; }}
            if (typeof window.__TTS_SET_STATE__ !== 'function' || typeof window.__TTS_PATCH_STATE__ !== 'function') {{
                window.setTimeout(begin, 25);
                return;
            }}
            if (!window.ipc?.postMessage) return;
            const active = window.__sgtTtsExitBarrier;
            if (active && active.id > id) return;
            const saved = {{ id, inert: active ? active.inert : document.body.inert, blocker: active?.blocker }};
            if ('inert' in document.body) {{
                document.body.inert = true;
            }} else if (!saved.blocker) {{
                // Runtimes without inert freeze editing with a capturing
                // blocker instead of refusing exit.
                saved.blocker = event => {{ event.preventDefault(); event.stopImmediatePropagation(); }};
                for (const type of {EDIT_EVENTS}) window.addEventListener(type, saved.blocker, true);
            }}
            window.__sgtTtsExitBarrier = saved;
            if (document.activeElement instanceof HTMLElement) document.activeElement.blur();
            window.ipc.postMessage(JSON.stringify({{ cmd: 'tts_exit_drain_ack', args: {{ request: id }} }}));
        }};
        begin();
    }})();"#
    )
}

fn thaw_script(request: usize) -> String {
    format!(
        r#"(() => {{
        const id = {request};
        window.__sgtTtsExitCancelled = Math.max(window.__sgtTtsExitCancelled || 0, id);
        const saved = window.__sgtTtsExitBarrier;
        if (saved?.id === id && document.body) {{
            if (saved.blocker) for (const type of {EDIT_EVENTS}) window.removeEventListener(type, saved.blocker, true);
            if ('inert' in document.body) document.body.inert = saved.inert;
            delete window.__sgtTtsExitBarrier;
        }}
    }})();"#
    )
}

fn run_script(hwnd: HWND, generation: usize, script: &str) {
    if !WINDOW_LIFECYCLE
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .is_ready_document(hwnd.0 as isize, generation, 1)
    {
        return;
    }
    super::WEBVIEW.with(|slot| {
        if let Some(webview) = slot.borrow().as_ref() {
            let _ = webview.evaluate_script(script);
        }
    });
}

pub(super) fn run_barrier(hwnd: HWND, request: usize, generation: usize) {
    run_script(hwnd, generation, &barrier_script(request));
}

pub(super) fn run_thaw(hwnd: HWND, request: usize, generation: usize) {
    run_script(hwnd, generation, &thaw_script(request));
}

#[cfg(test)]
mod tests;
