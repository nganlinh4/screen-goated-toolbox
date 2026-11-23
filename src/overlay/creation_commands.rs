use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use serde::Deserialize;
use serde_json::{Value, json};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

pub(crate) mod lane;
use crate::overlay::creation_error::{CreationError, ErrorCode};
use lane::Lane;

const MAX_PENDING: usize = 64;
const MAX_REQUEST_BYTES: usize = 1024 * 1024;
const MAX_REPLY_BYTES: usize = 8 * 1024 * 1024;
const ERROR_RESERVE: usize = 2048;
type Handler = fn(&str, &Value, &AtomicBool) -> Result<Value, CreationError>;

#[derive(Deserialize)]
pub(crate) struct Envelope {
    #[serde(default)]
    pub(crate) id: String,
    pub(crate) cmd: String,
    #[serde(default)]
    pub(crate) args: Value,
}

struct Permit(Arc<AtomicUsize>);

impl Drop for Permit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

struct Target {
    active: AtomicBool,
    pending: Arc<AtomicUsize>,
    replies: Mutex<Vec<(Permit, String)>>,
    handler: Handler,
    message: u32,
    max_request_bytes: usize,
}

impl Target {
    fn new(handler: Handler, message: u32, max_request_bytes: usize) -> Self {
        Self {
            active: AtomicBool::new(true),
            pending: Arc::new(AtomicUsize::new(0)),
            replies: Mutex::new(Vec::new()),
            handler,
            message,
            max_request_bytes: max_request_bytes.min(MAX_REQUEST_BYTES),
        }
    }

    fn reserve(&self, request_bytes: usize) -> Result<Permit, CreationError> {
        let replies = self
            .replies
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let count = self.pending.load(Ordering::Acquire);
        let stored = replies.iter().map(|(_, value)| value.len()).sum::<usize>();
        if !self.active.load(Ordering::Acquire) || request_bytes > self.max_request_bytes {
            return Err("Creation request queue is unavailable.".into());
        }
        if count >= MAX_PENDING
            || stored + (count + 1 - replies.len()) * ERROR_RESERVE > MAX_REPLY_BYTES
        {
            return Err(queue_full());
        }
        self.pending.fetch_add(1, Ordering::AcqRel);
        Ok(Permit(self.pending.clone()))
    }

    fn close(&self) {
        let mut replies = self
            .replies
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        self.active.store(false, Ordering::Release);
        replies.clear();
    }

    fn complete(&self, permit: Permit, mut script: String, fallback: String) -> bool {
        let mut replies = self
            .replies
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if !self.active.load(Ordering::Acquire) {
            return false;
        }
        let remaining = self.pending.load(Ordering::Acquire) - replies.len() - 1;
        let available = MAX_REPLY_BYTES
            - replies.iter().map(|(_, value)| value.len()).sum::<usize>()
            - remaining * ERROR_RESERVE;
        if script.len() > available {
            script = fallback;
        }
        replies.push((permit, script));
        true
    }

    fn take_replies(&self) -> Vec<String> {
        let mut replies = self
            .replies
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        // Release permits under the same lock used to reserve reply storage.
        std::mem::take(&mut *replies)
            .into_iter()
            .map(|(_, script)| script)
            .collect()
    }
}

struct Request {
    hwnd: crate::win_types::SendHwnd,
    target: Arc<Target>,
    permit: Permit,
    envelope: Envelope,
}

struct Entry {
    target: Arc<Target>,
    lane: Lane<Request>,
}

impl Entry {
    fn new(target: Target, process: impl Fn(Request) + Send + 'static) -> anyhow::Result<Self> {
        Ok(Self {
            target: Arc::new(target),
            lane: Lane::new("sgt-creation-commands", MAX_PENDING, process)?,
        })
    }

    fn submit(&self, hwnd: HWND, envelope: Envelope, bytes: usize) -> Result<(), CreationError> {
        if envelope.id.len() > 256 {
            return Err("Creation request identifier is too long.".into());
        }
        let permit = self.target.reserve(bytes)?;
        self.lane
            .submit(Request {
                hwnd: crate::win_types::SendHwnd(hwnd),
                target: self.target.clone(),
                permit,
                envelope,
            })
            .map_err(|_| queue_full())
    }
}

fn queue_full() -> CreationError {
    CreationError::new(ErrorCode::QueueFull, "Creation request queue is full.")
}

static TARGETS: LazyLock<Mutex<HashMap<isize, Arc<Entry>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub(crate) fn register(hwnd: HWND, message: u32, max_request_bytes: usize, handler: Handler) {
    unregister(hwnd);
    let entry = match Entry::new(Target::new(handler, message, max_request_bytes), process) {
        Ok(entry) => Arc::new(entry),
        Err(error) => {
            crate::log_info!("[Creation] Background commands unavailable: {error}");
            return;
        }
    };
    let previous = TARGETS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .insert(hwnd.0 as isize, entry);
    if let Some(previous) = previous {
        previous.target.close();
    }
}

pub(crate) fn unregister(hwnd: HWND) {
    let entry = TARGETS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .remove(&(hwnd.0 as isize));
    if let Some(entry) = entry {
        entry.target.close();
    }
}

pub(crate) fn request(hwnd: HWND, envelope: Envelope, bytes: usize) -> Result<(), CreationError> {
    let entry = TARGETS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .get(&(hwnd.0 as isize))
        .cloned()
        .ok_or_else(|| "Creation window is no longer available.".to_string())?;
    entry.submit(hwnd, envelope, bytes)
}

fn process(request: Request) {
    let handler = request.target.handler;
    let message = request.target.message;
    process_with(request, handler, |hwnd| unsafe {
        let _ = PostMessageW(Some(hwnd), message, WPARAM(0), LPARAM(0));
    });
}

fn process_with(
    request: Request,
    dispatch: impl FnOnce(&str, &Value, &AtomicBool) -> Result<Value, CreationError>,
    notify: impl FnOnce(HWND),
) {
    if !request.target.active.load(Ordering::Acquire) {
        return;
    }
    let result = dispatch(
        &request.envelope.cmd,
        &request.envelope.args,
        &request.target.active,
    );
    if let Err(error) = &result {
        crate::log_info!("[Creation] {} failed: {error}", request.envelope.cmd);
    }
    let script = reply_script(&request.envelope.id, result);
    let fallback = reply_script(
        &request.envelope.id,
        Err("Creation reply storage is full.".into()),
    );
    if request.target.complete(request.permit, script, fallback) {
        notify(request.hwnd.0);
    }
}

pub(crate) fn take_replies(hwnd: HWND) -> Vec<String> {
    let entry = TARGETS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .get(&(hwnd.0 as isize))
        .cloned();
    entry
        .map(|entry| entry.target.take_replies())
        .unwrap_or_default()
}

/// The reply exposes only the stable code; callers log the detailed cause.
pub(crate) fn reply_script(id: &str, result: Result<Value, CreationError>) -> String {
    let payload = match result {
        Ok(value) => json!({ "id": id, "result": value }),
        Err(error) => json!({ "id": id, "error": error.code().as_str() }),
    };
    format!("window.dispatchEvent(new CustomEvent('ipc-reply', {{ detail: {payload} }}));")
}

#[cfg(test)]
mod tests;
