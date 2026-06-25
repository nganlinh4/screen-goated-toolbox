//! Computer Control session lifecycle + the "assistant orb" overlay driver.
//!
//! Owns starting/stopping the background runtime thread and pushes the agent's
//! current activity (state + caption + audio level) to the transparent orb overlay
//! (`super::orb`). The orb IS the on-screen UI; the `set_status`/`push_log` helpers
//! are stderr breadcrumbs for headless/dev runs.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

static CC_ACTIVE: AtomicBool = AtomicBool::new(false);
/// True while the orb is showing a spoken reply (Responding). `set_model_idle` only clears the
/// caption when this is set, so a narration that's immediately followed by a tool call (which moves
/// the orb into an action state) doesn't get yanked back to "listening" mid-task.
static ORB_RESPONDING: AtomicBool = AtomicBool::new(false);
static CC_STOP: std::sync::LazyLock<Arc<AtomicBool>> =
    std::sync::LazyLock::new(|| Arc::new(AtomicBool::new(false)));

/// The orb's states. `label()` must match the `STATES` labels in `orb/orb.html`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum OrbState {
    Idle,
    Listening,
    Thinking,
    Look,
    Click,
    Type,
    Drag,
    Scroll,
    Point,
    Navigate,
    Launch,
    Run,
    Responding,
    Done,
    Error,
}

impl OrbState {
    fn label(self) -> &'static str {
        match self {
            OrbState::Idle => "idle",
            OrbState::Listening => "listening",
            OrbState::Thinking => "thinking",
            OrbState::Look => "look",
            OrbState::Click => "click",
            OrbState::Type => "type",
            OrbState::Drag => "drag",
            OrbState::Scroll => "scroll",
            OrbState::Point => "point",
            OrbState::Navigate => "navigate",
            OrbState::Launch => "launch",
            OrbState::Run => "run",
            OrbState::Responding => "responding",
            OrbState::Done => "done",
            OrbState::Error => "error",
        }
    }

    /// Whether the orb is draggable in this state. Action states are click-through
    /// so the agent's synthetic clicks are never intercepted by the orb.
    fn interactive(self) -> bool {
        !matches!(
            self,
            OrbState::Look
                | OrbState::Click
                | OrbState::Type
                | OrbState::Drag
                | OrbState::Scroll
                | OrbState::Point
                | OrbState::Navigate
                | OrbState::Launch
                | OrbState::Run
        )
    }
}

pub fn is_active() -> bool {
    CC_ACTIVE.load(Ordering::SeqCst)
}

/// Start a Computer Control session (no-op if one is already running).
pub fn show_overlay() {
    if CC_ACTIVE.swap(true, Ordering::SeqCst) {
        return;
    }
    CC_STOP.store(false, Ordering::SeqCst);
    super::orb::ensure_started();
    super::orb::show_orb();
    set_orb_state(OrbState::Idle, None);
    let stop = CC_STOP.clone();
    std::thread::spawn(move || {
        super::runtime::run(stop);
        CC_ACTIVE.store(false, Ordering::SeqCst);
        super::orb::hide_orb();
    });
}

/// Signal the running session to stop (the runtime thread clears CC_ACTIVE).
pub fn stop_overlay() {
    CC_STOP.store(true, Ordering::SeqCst);
    super::orb::hide_orb();
}

// --- orb activity drivers (called from the runtime/reader thread) ---

pub(super) fn set_orb_state(state: OrbState, caption: Option<&str>) {
    super::orb::set_interactive(state.interactive());
    ORB_RESPONDING.store(matches!(state, OrbState::Responding), Ordering::SeqCst);
    // When the agent rests (not mid-task), glide the orb back to the user's spot; during active work
    // it stays wherever it last dodged so it isn't constantly hopping corners between clicks.
    if matches!(
        state,
        OrbState::Idle | OrbState::Listening | OrbState::Done | OrbState::Error
    ) {
        super::orb::restore_home();
    }
    let mut js = format!("window.cc&&window.cc.setState('{}');", state.label());
    if let Some(c) = caption {
        js.push_str(&format!("window.cc&&window.cc.setCaption(`{}`);", js_escape(c)));
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

/// Map a brain/executor tool name (see `uia_task::brain::dispatch`) to its orb state.
pub(super) fn set_orb_tool(name: &str) {
    let state = match name {
        "look" | "map_targets" | "see_whole_screen" | "list_windows" | "read_clipboard"
        | "search_memory" | "open_memory" | "browser_read_page" | "browser_query"
        | "browser_network" | "browser_status" | "browser_tabs" => OrbState::Look,
        "click" | "click_at" | "click_element" | "click_target" | "click_mark" | "click_here"
        | "double_click" | "browser_click" => OrbState::Click,
        "drag" | "drag_target" => OrbState::Drag,
        "scroll" => OrbState::Scroll,
        "point" | "point_at" => OrbState::Point,
        "type_text" | "key_combination" | "browser_fill" | "browser_eval" => OrbState::Type,
        "open_url" | "browser_navigate" | "browser_open_tab" | "browser_switch_tab"
        | "focus_window" => OrbState::Navigate,
        "launch_app" | "browser_setup" => OrbState::Launch,
        "run_command" => OrbState::Run,
        "done" => OrbState::Done,
        _ => OrbState::Thinking,
    };
    set_orb_state(state, None);
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
/// resting Listening state is set explicitly at connect/reconnect.)
pub(super) fn set_status(status: impl Into<String>) {
    let status = status.into();
    eprintln!("[cc] status: {status}");
    if status.starts_with("error")
        || status.starts_with("rate limited")
        || status.starts_with("halting")
    {
        set_orb_state(OrbState::Error, None);
    }
}

/// The agent is connected and resting, listening for the next command.
pub(super) fn set_orb_listening() {
    set_orb_state(OrbState::Listening, None);
}

/// Mic active/quiet — stderr breadcrumb only (the orb's Listening state comes from
/// the "ready" status; live voice level is pushed via `set_orb_audio`).
pub(super) fn set_listening(on: bool) {
    static LAST: AtomicBool = AtomicBool::new(false);
    if LAST.swap(on, Ordering::SeqCst) != on {
        eprintln!("[cc] listening: {on}");
    }
}

/// The user just spoke → the model is thinking; show the command as the caption.
pub(super) fn set_user_text(text: impl Into<String>) {
    let text = text.into();
    eprintln!("[cc] you: {text}");
    set_orb_state(OrbState::Thinking, Some(&text));
}

/// The model is speaking → responding; show the reply as the caption.
pub(super) fn set_model_text(text: impl Into<String>) {
    set_orb_state(OrbState::Responding, Some(&text.into()));
}

/// The reply AUDIO finished playing — drop the caption and rest the orb. Driven by the runtime's
/// speech-end detection: the caption must outlive the transcript (which completes many seconds
/// before the audio does), so the text never vanishes mid-sentence.
pub(super) fn set_model_idle() {
    // Only act if the orb is STILL showing the reply — if a tool call already moved it into an
    // action state, leave it alone (don't flash to "listening" mid-task).
    if !ORB_RESPONDING.swap(false, Ordering::SeqCst) {
        return;
    }
    set_orb_state(OrbState::Listening, None);
    super::orb::post_orb_script("window.cc&&window.cc.setCaption('');".to_string());
}

pub(super) fn push_log(line: impl Into<String>) {
    eprintln!("[cc] {}", line.into());
}
