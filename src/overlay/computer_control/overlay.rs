//! Computer Control session lifecycle + the "assistant orb" overlay driver.
//!
//! Owns starting/stopping the background runtime thread and pushes the agent's
//! current activity (state + caption + audio level) to the transparent orb overlay
//! (`super::orb`). The orb IS the on-screen UI; the `set_status`/`push_log` helpers
//! are stderr breadcrumbs for headless/dev runs.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

static CC_ACTIVE: AtomicBool = AtomicBool::new(false);
/// True while the orb is showing a spoken reply (Responding). `set_model_idle` only clears the
/// caption when this is set, so a narration that's immediately followed by a tool call (which moves
/// the orb into an action state) doesn't get yanked back to "idle" mid-task.
static ORB_RESPONDING: AtomicBool = AtomicBool::new(false);
/// The running session's own stop token. Every session gets a fresh one, so a
/// stop requested for an ending session stays set for its stragglers even
/// after the next session starts. The lock also serializes start against stop.
static SESSION_STOP: std::sync::LazyLock<Mutex<Arc<AtomicBool>>> =
    std::sync::LazyLock::new(|| Mutex::new(Arc::new(AtomicBool::new(false))));

/// Why a session could not take a launch or a command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SessionUnavailable {
    /// The previous session is still shutting down.
    Stopping,
    /// No usable API key; the user has been notified.
    Credentials,
}
/// The orb's states. `label()` must match the `STATES` labels in `orb/orb.html`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum OrbState {
    Idle,
    Thinking,
    Look,
    Observe,
    Click,
    Act,
    DoSteps,
    Type,
    Drag,
    Scroll,
    Point,
    Navigate,
    Launch,
    Run,
    Wait,
    Memory,
    Console,
    Responding,
    Done,
    Error,
}

impl OrbState {
    fn label(self) -> &'static str {
        match self {
            OrbState::Idle => "idle",
            OrbState::Thinking => "thinking",
            OrbState::Look => "look",
            OrbState::Observe => "observe",
            OrbState::Click => "click",
            OrbState::Act => "act",
            OrbState::DoSteps => "do_steps",
            OrbState::Type => "type",
            OrbState::Drag => "drag",
            OrbState::Scroll => "scroll",
            OrbState::Point => "point",
            OrbState::Navigate => "navigate",
            OrbState::Launch => "launch",
            OrbState::Run => "run",
            OrbState::Wait => "wait",
            OrbState::Memory => "memory",
            OrbState::Console => "console",
            OrbState::Responding => "responding",
            OrbState::Done => "done",
            OrbState::Error => "error",
        }
    }
}

pub fn is_active() -> bool {
    CC_ACTIVE.load(Ordering::SeqCst)
}

fn session_stop() -> MutexGuard<'static, Arc<AtomicBool>> {
    SESSION_STOP
        .lock()
        .unwrap_or_else(|error| error.into_inner())
}

/// The outcome of asking for a session while one is already marked active.
fn running_session(stop: &AtomicBool) -> Result<(), SessionUnavailable> {
    if stop.load(Ordering::SeqCst) {
        Err(SessionUnavailable::Stopping)
    } else {
        Ok(())
    }
}

pub(crate) fn launch_and_submit(
    text: String,
) -> Result<super::runtime::control::TextCommandDisposition, SessionUnavailable> {
    if is_active() {
        running_session(&session_stop())?;
    }
    let disposition = super::runtime::control::submit_external_text_command(text);
    if disposition != super::runtime::control::TextCommandDisposition::Queued {
        return Ok(disposition);
    }
    if !is_active()
        && let Err(reason) = start_session()
    {
        super::runtime::control::clear_startup_text_command();
        return Err(reason);
    }
    Ok(disposition)
}

/// Start a Computer Control session (no-op if one is already running).
pub fn show_overlay() {
    let _ = start_session();
}

/// Start a session, or report why the running one cannot take new work.
/// `Ok` means a session is live: newly started or already running.
pub(crate) fn start_session() -> Result<(), SessionUnavailable> {
    let mut current_stop = session_stop();
    if CC_ACTIVE.swap(true, Ordering::SeqCst) {
        return running_session(&current_stop);
    }
    if let Err(error) = super::session::load_key() {
        CC_ACTIVE.store(false, Ordering::SeqCst);
        drop(current_stop);
        show_startup_credential_error(&error);
        return Err(SessionUnavailable::Credentials);
    }
    let stop = Arc::new(AtomicBool::new(false));
    *current_stop = stop.clone();
    drop(current_stop);
    super::orb::show_orb();
    set_orb_state(OrbState::Idle, None);
    std::thread::spawn(move || {
        super::runtime::run(stop);
        // Hide before releasing the session, so a session started right after
        // this one never has its freshly shown orb hidden by this thread.
        super::orb::hide_orb();
        CC_ACTIVE.store(false, Ordering::SeqCst);
    });
    Ok(())
}

pub(super) fn show_startup_credential_error(error: &anyhow::Error) -> bool {
    let Some(code) = super::session::api_key_error_code(error) else {
        return false;
    };
    let language = crate::APP
        .lock()
        .map(|app| app.config.ui_language.clone())
        .unwrap_or_else(|_| "en".to_string());
    crate::overlay::utils::show_api_key_error_notification(code, &language);
    true
}

/// Signal the running session to stop (the runtime thread clears CC_ACTIVE).
pub fn stop_overlay() {
    super::runtime::control::clear_startup_text_command();
    super::runtime::control::request_stop(&session_stop());
    super::orb::hide_orb();
}

// --- orb activity drivers (called from the runtime/reader thread) ---

pub(super) fn set_orb_state(state: OrbState, caption: Option<&str>) {
    ORB_RESPONDING.store(matches!(state, OrbState::Responding), Ordering::SeqCst);
    // When the agent rests (not mid-task), glide the orb back to the user's spot; during active work
    // it stays wherever it last dodged so it isn't constantly hopping corners between clicks.
    if matches!(state, OrbState::Idle | OrbState::Done | OrbState::Error) {
        super::orb::restore_home();
    }
    let mut js = format!("window.cc&&window.cc.setState('{}');", state.label());
    if let Some(c) = caption {
        js.push_str(&format!(
            "window.cc&&window.cc.setCaption(`{}`);",
            js_escape(c)
        ));
    }
    super::orb::post_orb_script(js);
}

pub(super) fn set_orb_audio(level: f32) {
    // throttle: the mic loop ticks far faster than the orb needs (~12/s is plenty)
    static LAST: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);
    if let Ok(mut g) = LAST.lock() {
        let now = std::time::Instant::now();
        if g.is_some_and(|t| now.duration_since(t).as_millis() < 80) {
            return;
        }
        *g = Some(now);
    }
    super::orb::post_orb_script(format!(
        "window.cc&&window.cc.setAudio({:.3});",
        level.clamp(0.0, 1.0)
    ));
}

fn set_orb_icon(name: &str) {
    super::orb::post_orb_script(format!("window.cc&&window.cc.setIcon('{name}');"));
}

/// Map a brain/executor tool name (see `uia_task::brain::dispatch`) to its orb state.
pub(super) fn set_orb_tool(name: &str, args: &serde_json::Value) {
    let state = match name {
        "observe" => OrbState::Observe,
        "look"
        | "map_targets"
        | "see_whole_screen"
        | "browser_read_page"
        | "browser_extract_page"
        | "artifact_info"
        | "extract_artifact"
        | "read_text_file"
        | "save_artifact"
        | "list_windows"
        | "read_clipboard"
        | "system_query"
        | "browser_network"
        | "browser_status"
        | "browser_tabs" => OrbState::Look,
        "search_memory" | "open_memory" => OrbState::Memory,
        "browser_console" => OrbState::Console,
        "act" => OrbState::Act,
        "click" | "click_at" | "click_target" | "click_mark" | "click_here" => OrbState::Click,
        "do_steps" => OrbState::DoSteps,
        "drag" | "drag_target" => OrbState::Drag,
        "scroll" => OrbState::Scroll,
        "point" | "point_at" => OrbState::Point,
        "type_text"
        | "key_combination"
        | "browser_eval"
        | "paste_artifact"
        | "edit_text_file"
        | "edit_text_file_structure" => OrbState::Type,
        "open_url" | "browser_navigate" | "browser_open_tab" | "browser_switch_tab"
        | "focus_window" => OrbState::Navigate,
        "launch_app" | "browser_setup" => OrbState::Launch,
        "run_command" => OrbState::Run,
        "wait" => OrbState::Wait,
        "done" => OrbState::Done,
        _ => OrbState::Thinking,
    };
    set_orb_state(state, None);
    // Scroll's glyph is DIRECTIONAL (not a loop): show the arrow matching the actual direction.
    if matches!(state, OrbState::Scroll) {
        set_orb_icon(match args.get("direction").and_then(|v| v.as_str()) {
            Some("up") => "keyboard_double_arrow_up",
            Some("left") => "keyboard_double_arrow_left",
            Some("right") => "keyboard_double_arrow_right",
            _ => "keyboard_double_arrow_down",
        });
    }
}

/// Convenience for the goal-reached transition.
pub(super) fn set_orb_done() {
    set_orb_state(OrbState::Done, None);
}

/// Escape a caption for safe interpolation into a JS template literal.
fn js_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('`', "\\`")
        .replace("${", "\\${")
        .replace('\r', "")
}

// --- runtime-facing helpers: stderr diagnostics + folded orb driving ---

/// Human-readable status → stderr, plus the error orb state for failure/barge-in
/// statuses. ("ready" is NOT mapped here — it also fires between tool steps; the
/// resting Idle state is set explicitly at connect/reconnect.)
pub(super) fn set_status(status: impl Into<String>) {
    let status = status.into();
    super::telemetry::human("cc", format!("status: {status}"));
    super::telemetry::event(
        "status",
        "overlay",
        super::telemetry::Privacy::Safe,
        serde_json::json!({"status": status.clone()}),
    );
    if status.starts_with("error")
        || status.starts_with("rate limited")
        || status.starts_with("halting")
    {
        set_orb_state(OrbState::Error, None);
    }
}

/// The agent is connected and at rest - the calm Idle default. There is no separate
/// "listening" state: the Idle orb expresses hearing the user purely through its volume
/// reaction (`set_orb_audio`), since the Live model does its own speech detection.
pub(super) fn set_orb_resting() {
    set_orb_state(OrbState::Idle, None);
}

/// Mic active/quiet — stderr breadcrumb only; the live voice LEVEL (what drives the orb's
/// volume reaction) is pushed separately via `set_orb_audio`.
pub(super) fn set_listening(on: bool) {
    static LAST: AtomicBool = AtomicBool::new(false);
    if LAST.swap(on, Ordering::SeqCst) != on {
        super::telemetry::human("cc", format!("listening: {on}"));
        super::telemetry::event(
            "listening",
            "overlay",
            super::telemetry::Privacy::Safe,
            serde_json::json!({"on": on}),
        );
    }
}

/// The user just spoke → the model is thinking; show the command as the caption.
pub(super) fn set_user_text(text: impl Into<String>) {
    let text = text.into();
    set_orb_state(OrbState::Thinking, Some(&text));
}

/// The model is speaking → responding; show the reply as the caption.
pub(super) fn set_model_text(text: impl Into<String>) {
    let text = text.into();
    if std::env::var("CC_TELEMETRY_VERBOSE").is_ok() {
        super::telemetry::event(
            "assistant_transcript_delta",
            "speech",
            super::telemetry::Privacy::UserText,
            serde_json::json!({"text_preview": text.chars().take(240).collect::<String>(), "char_count": text.chars().count()}),
        );
    }
    set_orb_state(OrbState::Responding, Some(&text));
}

/// The reply AUDIO finished playing — drop the caption and rest the orb. Driven by the runtime's
/// speech-end detection: the caption must outlive the transcript (which completes many seconds
/// before the audio does), so the text never vanishes mid-sentence.
pub(super) fn set_model_idle() {
    // Only act if the orb is STILL showing the reply — if a tool call already moved it into an
    // action state, leave it alone (don't flash to "idle" mid-task).
    if !ORB_RESPONDING.swap(false, Ordering::SeqCst) {
        return;
    }
    set_orb_state(OrbState::Idle, None);
    super::orb::post_orb_script("window.cc&&window.cc.setCaption('');".to_string());
}

/// Remove model text that was structurally rejected before it became an
/// accepted response. Unlike `set_model_idle`, this also clears a caption after
/// a tool state replaced Responding.
pub(super) fn clear_model_caption() {
    ORB_RESPONDING.store(false, Ordering::SeqCst);
    super::orb::post_orb_script("window.cc&&window.cc.setCaption('');".to_string());
}

pub(super) fn push_log(line: impl Into<String>) {
    let line = line.into();
    super::telemetry::human("cc", &line);
    super::telemetry::event(
        "log",
        "runtime",
        super::telemetry::Privacy::Sensitive,
        serde_json::json!({"line": line.clone()}),
    );
}

/// Run `body` while a session is marked active and stopping, then restore an
/// idle lifecycle. Exercises the real start/submit paths without a runtime.
#[cfg(test)]
pub(super) fn with_stopping_session<T>(body: impl FnOnce() -> T) -> T {
    let stop = Arc::new(AtomicBool::new(true));
    *session_stop() = stop;
    assert!(
        !CC_ACTIVE.swap(true, Ordering::SeqCst),
        "no real session in tests"
    );
    let result = body();
    CC_ACTIVE.store(false, Ordering::SeqCst);
    *session_stop() = Arc::new(AtomicBool::new(false));
    result
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;

    #[test]
    fn an_active_session_reports_stopping_until_it_has_exited() {
        let stop = AtomicBool::new(false);
        assert_eq!(running_session(&stop), Ok(()));
        stop.store(true, Ordering::SeqCst);
        assert_eq!(running_session(&stop), Err(SessionUnavailable::Stopping));
    }
}
