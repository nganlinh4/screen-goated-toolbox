//! Structured telemetry for Computer Control.
//!
//! The live CC harness used to print ad-hoc `[cc] ...` lines from many threads.
//! This module keeps the readable breadcrumbs while also writing JSONL records
//! with stable session/turn/step identifiers and monotonic timing.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

mod privacy;

static START: OnceLock<Instant> = OnceLock::new();
static TURN_ID: AtomicU64 = AtomicU64::new(0);
static STEP_ID: AtomicU64 = AtomicU64::new(0);
static UTTERANCE_ID: AtomicU64 = AtomicU64::new(0);
static FRAME_ID: AtomicU64 = AtomicU64::new(0);
static ARTIFACT_ID: AtomicU64 = AtomicU64::new(0);
static SESSION_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static SESSION: OnceLock<Mutex<SessionState>> = OnceLock::new();
static WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ActionTrace {
    pub action_id: u64,
    pub turn_id: u64,
}

struct SessionState {
    id: String,
    trace_dir: PathBuf,
    claimed: bool,
    start_recorded: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Privacy {
    Safe,
    /// Non-transcript content that is useful inside the per-session trace but
    /// must never be copied into the rolling global diagnostics file.
    Sensitive,
    /// User-visible user/assistant transcript content. Like `Sensitive`, this
    /// remains in the per-session trace only.
    UserText,
}

impl Privacy {
    fn as_str(self) -> &'static str {
        match self {
            Privacy::Safe => "safe",
            Privacy::Sensitive => "sensitive",
            Privacy::UserText => "user_text",
        }
    }

    fn may_write_global(self) -> bool {
        matches!(self, Privacy::Safe)
    }
}

pub(super) fn session_id() -> String {
    session_state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .id
        .clone()
}

/// Stable artifact directory for this process's Computer Control session.
/// `CC_TRACE_DIR` names the root; the session id is always appended so two
/// launches can never overwrite one another's evidence.
pub(super) fn trace_dir() -> PathBuf {
    session_state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .trace_dir
        .clone()
}

/// Claim a fresh trace context for a new `Brain`. A lazily-created, unclaimed
/// context is reused so the initial snapshot cannot land in a different folder.
pub(super) fn begin_session() {
    let mut state = session_state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let replacing_previous = state.claimed;
    if replacing_previous {
        *state = new_session_state();
    }
    state.claimed = true;
    drop(state);
    if replacing_previous {
        TURN_ID.store(0, Ordering::SeqCst);
        STEP_ID.store(0, Ordering::SeqCst);
        UTTERANCE_ID.store(0, Ordering::SeqCst);
        FRAME_ID.store(0, Ordering::SeqCst);
        ARTIFACT_ID.store(0, Ordering::SeqCst);
    }
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

pub(super) fn record_session_start(fields: Value) {
    let should_record = {
        let mut state = session_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let should_record = !state.start_recorded;
        state.start_recorded = true;
        should_record
    };
    if should_record {
        event("session_start", "runtime", Privacy::UserText, fields);
    }
}

pub(super) fn record_model_setup(setup: &Value, stage: &str) {
    let declarations = setup
        .pointer("/setup/tools")
        .and_then(Value::as_array)
        .and_then(|tools| {
            tools
                .iter()
                .find_map(|tool| tool.get("functionDeclarations"))
        })
        .and_then(Value::as_array);
    let function_count = declarations.map_or(0, Vec::len);
    let dynamic_function_count = declarations.map_or(0, |items| {
        items
            .iter()
            .filter(|item| {
                item.get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|name| name.starts_with("mcp__"))
            })
            .count()
    });
    let built_in_function_count = function_count.saturating_sub(dynamic_function_count);
    let declaration_bytes = declarations
        .and_then(|items| serde_json::to_vec(items).ok())
        .map_or(0, |bytes| bytes.len());
    let instruction_bytes = setup
        .pointer("/setup/systemInstruction")
        .and_then(|value| serde_json::to_vec(value).ok())
        .map_or(0, |bytes| bytes.len());
    let encoded = serde_json::to_vec(setup).unwrap_or_default();
    event(
        "model_setup",
        "runtime",
        Privacy::Safe,
        json!({
            "stage": stage,
            "model": setup.pointer("/setup/model"),
            "function_count": function_count,
            "built_in_function_count": built_in_function_count,
            "dynamic_function_count": dynamic_function_count,
            "declaration_bytes": declaration_bytes,
            "instruction_bytes": instruction_bytes,
            "setup_bytes": encoded.len(),
            "setup_fingerprint": format!("fnv1a64:{:016x}", fnv1a64(&encoded)),
            "thinking_level": setup.pointer("/setup/generationConfig/thinkingConfig/thinkingLevel"),
            "max_output_tokens": setup.pointer("/setup/generationConfig/maxOutputTokens"),
            "search_enabled": setup.pointer("/setup/tools").and_then(Value::as_array)
                .is_some_and(|tools| tools.iter().any(|tool| tool.get("googleSearch").is_some())),
        }),
    );
}

pub(super) fn next_step(tool: &str) -> ActionTrace {
    let id = STEP_ID.fetch_add(1, Ordering::SeqCst) + 1;
    let trace = ActionTrace {
        action_id: id,
        turn_id: current_turn(),
    };
    event_for_action(
        "step_start",
        "tool",
        Privacy::Safe,
        trace,
        json!({"step_id": id, "action_id": id, "tool": tool}),
    );
    trace
}

/// Bind execution to the turn/action created by [`next_step`]. The action
/// worker is on another thread, so a thread-local or `current_turn()` lookup is
/// not sufficient: barge-in may advance the global turn while input is still in
/// flight. The newest pending action is the owner.
pub(super) fn claim_action(dispatch_tool: &str) -> ActionTrace {
    let id = STEP_ID.fetch_add(1, Ordering::SeqCst) + 1;
    let trace = ActionTrace {
        action_id: id,
        turn_id: current_turn(),
    };
    event_for_action(
        "action_dispatch",
        "tool",
        Privacy::Safe,
        trace,
        json!({
            "requested_tool": dispatch_tool,
            "dispatch_tool": dispatch_tool,
            "source": "dispatch_fallback",
        }),
    );
    trace
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
    next_frame_for(reason, None)
}

pub(super) fn next_frame_for(reason: &str, action: Option<ActionTrace>) -> u64 {
    let id = FRAME_ID.fetch_add(1, Ordering::SeqCst) + 1;
    event_with_context(
        "frame",
        "capture",
        Privacy::Safe,
        action,
        json!({"frame_id": id, "reason": reason}),
    );
    id
}

pub(super) fn next_artifact_id() -> u64 {
    ARTIFACT_ID.fetch_add(1, Ordering::SeqCst) + 1
}

pub(super) struct FrameReady<'a> {
    pub turn_id: u64,
    pub frame_id: u64,
    pub reason: &'a str,
    pub capture_ms: u128,
    pub encode_ms: u128,
    pub byte_count: usize,
    pub target: Option<&'a str>,
    pub view: [i32; 4],
    pub window_title: &'a str,
    pub artifact_path: Option<&'a str>,
}

pub(super) fn frame_ready(frame: FrameReady<'_>) {
    event_for_turn(
        "frame_ready",
        "capture",
        Privacy::Safe,
        frame.turn_id,
        json!({
            "frame_id": frame.frame_id,
            "reason": frame.reason,
            "capture_ms": frame.capture_ms,
            "encode_ms": frame.encode_ms,
            "byte_count": frame.byte_count,
            "target": frame.target,
            "view": frame.view,
            "window_title": frame.window_title,
            "artifact_path": frame.artifact_path,
            "turn_id": frame.turn_id,
        }),
    );
}

pub(super) fn event_for_turn(
    event: &str,
    component: &str,
    privacy: Privacy,
    turn_id: u64,
    fields: Value,
) {
    let fields = privacy::sanitize_safe_fields(privacy, fields);
    let record = json!({
        "session_id": session_id(),
        "turn_id": turn_id,
        "action_id": Value::Null,
        "event": event,
        "component": component,
        "privacy": privacy.as_str(),
        "fields": fields,
    });
    write_jsonl(record, privacy);
}

pub(super) fn human(component: &str, line: impl AsRef<str>) {
    crate::log_info!("[{component}] {}", line.as_ref());
}

pub(super) fn event(event: &str, component: &str, privacy: Privacy, fields: Value) {
    let inferred = fields
        .get("action_id")
        .or_else(|| fields.get("step_id"))
        .and_then(Value::as_u64)
        .map(|action_id| ActionTrace {
            action_id,
            turn_id: current_turn(),
        });
    event_with_context(event, component, privacy, inferred, fields);
}

pub(super) fn event_for_action(
    event: &str,
    component: &str,
    privacy: Privacy,
    action: ActionTrace,
    fields: Value,
) {
    event_with_context(event, component, privacy, Some(action), fields);
}

fn event_with_context(
    event: &str,
    component: &str,
    privacy: Privacy,
    action: Option<ActionTrace>,
    fields: Value,
) {
    let fields = privacy::sanitize_safe_fields(privacy, fields);
    let record = json!({
        "session_id": session_id(),
        "turn_id": action.map_or_else(current_turn, |trace| trace.turn_id),
        "action_id": action.map(|trace| trace.action_id),
        "event": event,
        "component": component,
        "privacy": privacy.as_str(),
        "fields": fields,
    });
    write_jsonl(record, privacy);
}

/// Content-free JSON diagnostics for values that may contain commands,
/// clipboard text, page content, paths, URLs, or provider output.
pub(super) fn value_metadata(value: &Value) -> Value {
    privacy::value_metadata(value)
}

pub(super) fn tool_result(
    action: ActionTrace,
    tool: &str,
    step: usize,
    duration_ms: u128,
    ok: Option<bool>,
    fields: Value,
) {
    event_for_action(
        "tool_result",
        "tool",
        Privacy::Sensitive,
        action,
        json!({
            "tool": tool,
            "step": step,
            "duration_ms": duration_ms,
            "ok": ok,
            "fields": fields,
        }),
    );
}

/// Add immutable correlation metadata to an artifact-side JSONL record. Common
/// fields are inserted last so a caller cannot accidentally spoof the ids.
pub(super) fn artifact_record(
    record_type: &str,
    record_id: u64,
    action: Option<ActionTrace>,
    fields: Value,
) -> Value {
    let mut object = match fields {
        Value::Object(object) => object,
        value => serde_json::Map::from_iter([("value".to_string(), value)]),
    };
    object.insert("ts_ms".to_string(), json!(unix_ms()));
    object.insert("mono_ms".to_string(), json!(mono_ms()));
    object.insert("session_id".to_string(), json!(session_id()));
    object.insert(
        "turn_id".to_string(),
        json!(action.map_or_else(current_turn, |trace| trace.turn_id)),
    );
    object.insert(
        "action_id".to_string(),
        json!(action.map(|trace| trace.action_id)),
    );
    object.insert("record_type".to_string(), json!(record_type));
    object.insert("record_id".to_string(), json!(record_id));
    Value::Object(object)
}

pub(super) fn artifact_write_failed(
    kind: &str,
    path: &Path,
    action: Option<ActionTrace>,
    error: &dyn std::fmt::Display,
) {
    let path = artifact_path(path);
    human(
        "cc-telemetry",
        format!("failed to write {kind} artifact (details kept in session trace)"),
    );
    event_with_context(
        "artifact_write_failed",
        "artifact",
        Privacy::Safe,
        action,
        json!({"kind": kind, "artifact_path": path, "error": error.to_string()}),
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

fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}

fn mono_ms() -> u128 {
    START.get_or_init(Instant::now).elapsed().as_millis()
}

fn write_jsonl(mut record: Value, privacy: Privacy) {
    let _guard = WRITE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    // Sampling before this lock lets a preempted producer write an older
    // timestamp after a newer record. Stamp at the serialization boundary so
    // JSONL order and monotonic time describe the same event order.
    stamp_for_serialized_write(&mut record, unix_ms(), mono_ms());
    let mut global = crate::paths::app_sgt_dir();
    global.push("logs");
    global.push("cc-events.jsonl");
    let session = trace_dir().join("events.jsonl");
    if privacy.may_write_global()
        && let Err(error) = append_line(&global, &record)
    {
        report_telemetry_failure(&global, &error);
    }
    if let Err(error) = append_line(&session, &record) {
        report_telemetry_failure(&session, &error);
    }
}

fn stamp_for_serialized_write(record: &mut Value, ts_ms: u128, mono_ms: u128) {
    if let Value::Object(object) = record {
        object.insert("ts_ms".to_string(), json!(ts_ms));
        object.insert("mono_ms".to_string(), json!(mono_ms));
    }
}

fn append_line(path: &Path, record: &Value) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(file, "{record}")
}

fn session_state() -> &'static Mutex<SessionState> {
    SESSION.get_or_init(|| Mutex::new(new_session_state()))
}

fn new_session_state() -> SessionState {
    let root = std::env::var_os("CC_TRACE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::paths::app_local_data_dir().join("cc-trace"));
    let sequence = SESSION_SEQUENCE.fetch_add(1, Ordering::SeqCst) + 1;
    let id = format!("cc-{}-{}-{sequence}", unix_ms(), std::process::id());
    SessionState {
        trace_dir: root.join(&id),
        id,
        claimed: false,
        start_recorded: false,
    }
}

fn artifact_path(path: &Path) -> String {
    let trace_dir = trace_dir();
    path.strip_prefix(&trace_dir)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

fn report_telemetry_failure(path: &Path, error: &std::io::Error) {
    let destination = if path
        .file_name()
        .is_some_and(|name| name == "cc-events.jsonl")
    {
        "global"
    } else {
        "session"
    };
    let message = format!(
        "[cc-telemetry] failed to append {destination} trace: {:?}",
        error.kind()
    );
    eprintln!("{message}");
    crate::debug_log::log_debug(&message);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifact_record_keeps_originating_action_context() {
        let trace = ActionTrace {
            action_id: 17,
            turn_id: 4,
        };
        let record = artifact_record(
            "click",
            9,
            Some(trace),
            json!({"kind": "target", "turn_id": 999, "action_id": 999}),
        );

        assert_eq!(record["session_id"], session_id());
        assert_eq!(record["turn_id"], 4);
        assert_eq!(record["action_id"], 17);
        assert_eq!(record["record_id"], 9);
        assert_eq!(record["kind"], "target");
    }

    #[test]
    fn only_content_free_events_may_reach_the_global_trace() {
        assert!(Privacy::Safe.may_write_global());
        assert!(!Privacy::Sensitive.may_write_global());
        assert!(!Privacy::UserText.may_write_global());
    }

    #[test]
    fn serialized_write_stamp_replaces_a_producer_sample() {
        let mut record = json!({"event": "test", "ts_ms": 900, "mono_ms": 800});

        stamp_for_serialized_write(&mut record, 101, 42);

        assert_eq!(record["ts_ms"], 101);
        assert_eq!(record["mono_ms"], 42);
    }
}
