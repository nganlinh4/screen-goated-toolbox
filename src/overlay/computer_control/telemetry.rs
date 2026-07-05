//! Structured telemetry for Computer Control.
//!
//! The live CC harness used to print ad-hoc `[cc] ...` lines from many threads.
//! This module keeps the readable breadcrumbs while also writing JSONL records
//! with stable session/turn/step identifiers and monotonic timing.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

static START: OnceLock<Instant> = OnceLock::new();
static SESSION_ID: OnceLock<String> = OnceLock::new();
static TURN_ID: AtomicU64 = AtomicU64::new(0);
static STEP_ID: AtomicU64 = AtomicU64::new(0);
static UTTERANCE_ID: AtomicU64 = AtomicU64::new(0);
static FRAME_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy)]
pub(super) enum Privacy {
    Safe,
    UserText,
}

impl Privacy {
    fn as_str(self) -> &'static str {
        match self {
            Privacy::Safe => "safe",
            Privacy::UserText => "user_text",
        }
    }
}

pub(super) fn session_id() -> &'static str {
    SESSION_ID.get_or_init(|| {
        let now = unix_ms();
        let pid = std::process::id();
        format!("cc-{now}-{pid}")
    })
}

pub(super) fn start_turn(reason: &str) -> u64 {
    let id = TURN_ID.fetch_add(1, Ordering::SeqCst) + 1;
    event(
        "turn_start",
        "runtime",
        Privacy::Safe,
        json!({"turn_id": id, "reason": reason}),
    );
    id
}

pub(super) fn current_turn() -> u64 {
    TURN_ID.load(Ordering::SeqCst)
}

pub(super) fn next_step(tool: &str) -> u64 {
    let id = STEP_ID.fetch_add(1, Ordering::SeqCst) + 1;
    event(
        "step_start",
        "tool",
        Privacy::Safe,
        json!({"step_id": id, "tool": tool, "turn_id": current_turn()}),
    );
    id
}

pub(super) fn next_utterance(reason: &str) -> u64 {
    let id = UTTERANCE_ID.fetch_add(1, Ordering::SeqCst) + 1;
    event(
        "utterance_start",
        "speech",
        Privacy::Safe,
        json!({"utterance_id": id, "reason": reason, "turn_id": current_turn()}),
    );
    id
}

pub(super) fn next_frame(reason: &str) -> u64 {
    let id = FRAME_ID.fetch_add(1, Ordering::SeqCst) + 1;
    event(
        "frame",
        "capture",
        Privacy::Safe,
        json!({"frame_id": id, "reason": reason, "turn_id": current_turn()}),
    );
    id
}

pub(super) fn human(component: &str, line: impl AsRef<str>) {
    crate::log_info!("[{component}] {}", line.as_ref());
}

pub(super) fn event(event: &str, component: &str, privacy: Privacy, fields: Value) {
    let record = json!({
        "ts_ms": unix_ms(),
        "mono_ms": mono_ms(),
        "session_id": session_id(),
        "turn_id": current_turn(),
        "event": event,
        "component": component,
        "privacy": privacy.as_str(),
        "fields": fields,
    });
    write_jsonl(&record);
}

pub(super) fn tool_result(
    tool: &str,
    step: usize,
    duration_ms: u128,
    ok: Option<bool>,
    fields: Value,
) {
    event(
        "tool_result",
        "tool",
        Privacy::Safe,
        json!({
            "tool": tool,
            "step": step,
            "duration_ms": duration_ms,
            "ok": ok,
            "fields": fields,
        }),
    );
}

pub(super) fn typed_error(code: &str, component: &str, message: &str, fields: Value) {
    event(
        "typed_error",
        component,
        Privacy::Safe,
        json!({
            "code": code,
            "message": message,
            "fields": fields,
        }),
    );
}

fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

fn mono_ms() -> u128 {
    START.get_or_init(Instant::now).elapsed().as_millis()
}

fn write_jsonl(record: &Value) {
    let mut path = crate::paths::app_sgt_dir();
    path.push("logs");
    let _ = std::fs::create_dir_all(&path);
    path.push("cc-events.jsonl");
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        use std::io::Write;
        let _ = writeln!(file, "{record}");
    }
}
