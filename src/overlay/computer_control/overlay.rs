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
/// the orb into an action state) doesn't get yanked back to "idle" mid-task.
static ORB_RESPONDING: AtomicBool = AtomicBool::new(false);
static CC_STOP: std::sync::LazyLock<Arc<AtomicBool>> =
    std::sync::LazyLock::new(|| Arc::new(AtomicBool::new(false)));
/// The latest spoken-reply text (kept only while Responding) - read by the
/// sentiment thread to drive the orb's live emotion glyph.
static LAST_REPLY: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

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
    // Live emotion: while the agent SPEAKS, classify its reply's tone ~1x/s via the
    // fast/free Taalas LLM and show the matching glyph. Exits when the session ends.
    let stop_sentiment = CC_STOP.clone();
    std::thread::spawn(move || sentiment_loop(stop_sentiment));
}

/// Signal the running session to stop (the runtime thread clears CC_ACTIVE).
pub fn stop_overlay() {
    CC_STOP.store(true, Ordering::SeqCst);
    super::orb::hide_orb();
}

// --- orb activity drivers (called from the runtime/reader thread) ---

pub(super) fn set_orb_state(state: OrbState, caption: Option<&str>) {
    ORB_RESPONDING.store(matches!(state, OrbState::Responding), Ordering::SeqCst);
    // Track the reply text for the live-sentiment thread: keep it while speaking, drop
    // it the moment we leave Responding so a stale reply can't drive the next turn's
    // emotion glyph before fresh text arrives.
    match (matches!(state, OrbState::Responding), caption) {
        (true, Some(c)) => {
            if let Ok(mut g) = LAST_REPLY.lock() {
                *g = c.to_string();
            }
        }
        (false, _) => {
            if let Ok(mut g) = LAST_REPLY.lock() {
                g.clear();
            }
        }
        _ => {}
    }
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

/// Push a live glyph override onto the orb (the sentiment thread uses this to show
/// the agent's spoken-reply emotion). The next `setState` clears the override.
fn set_orb_icon(name: &str) {
    super::orb::post_orb_script(format!("window.cc&&window.cc.setIcon('{name}');"));
}

/// Background: while Responding, classify the spoken reply's emotional tone with the
/// fast/free Taalas LLM (~1x/s) and push the matching `sentiment_*` glyph to the orb.
/// Only re-pushes on change to avoid glyph thrash; exits when the session stops.
fn sentiment_loop(stop: Arc<AtomicBool>) {
    let mut last: &'static str = "";
    while !stop.load(Ordering::SeqCst) && CC_ACTIVE.load(Ordering::SeqCst) {
        std::thread::sleep(std::time::Duration::from_millis(1000));
        if !ORB_RESPONDING.load(Ordering::SeqCst) {
            last = "";
            continue;
        }
        let reply = LAST_REPLY.lock().map(|g| g.clone()).unwrap_or_default();
        if reply.trim().is_empty() {
            continue;
        }
        if let Some(icon) = sentiment_icon(&reply)
            && icon != last
        {
            last = icon;
            set_orb_icon(icon);
        }
    }
}

/// Ask Taalas to label the reply's tone, then map it to one of the orb's 13
/// `sentiment_*` glyphs. `None` only when the LLM is unreachable (orb keeps its base
/// glyph). Labels are matched most-specific-first so "dissatisfied" can't shadow
/// "very_dissatisfied" / "extremely_dissatisfied", nor "satisfied" shadow "very_satisfied".
fn sentiment_icon(reply: &str) -> Option<&'static str> {
    let snippet: String = reply.chars().take(600).collect();
    let prompt = format!(
        "You label the emotional TONE of an assistant's spoken reply. Respond with EXACTLY ONE of \
         these labels and nothing else: very_satisfied, satisfied, excited, content, calm, neutral, \
         worried, stressed, frustrated, sad, dissatisfied, very_dissatisfied, extremely_dissatisfied.\
         \n\nReply: {snippet}"
    );
    let resp = crate::api::taalas::generate(&prompt)?.to_lowercase();
    const LABELS: &[(&str, &str)] = &[
        ("extremely_dissatisfied", "sentiment_extremely_dissatisfied"),
        ("very_dissatisfied", "sentiment_very_dissatisfied"),
        ("very_satisfied", "sentiment_very_satisfied"),
        ("dissatisfied", "sentiment_dissatisfied"),
        ("satisfied", "sentiment_satisfied"),
        ("frustrated", "sentiment_frustrated"),
        ("stressed", "sentiment_stressed"),
        ("excited", "sentiment_excited"),
        ("worried", "sentiment_worried"),
        ("content", "sentiment_content"),
        ("neutral", "sentiment_neutral"),
        ("calm", "sentiment_calm"),
        ("sad", "sentiment_sad"),
    ];
    for (key, icon) in LABELS {
        if resp.contains(key) || resp.contains(&key.replace('_', " ")) {
            return Some(icon);
        }
    }
    Some("sentiment_neutral")
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
        "type_text" | "key_combination" | "browser_eval" | "paste_artifact" => OrbState::Type,
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

pub(super) fn push_log(line: impl Into<String>) {
    let line = line.into();
    super::telemetry::human("cc", &line);
    super::telemetry::event(
        "log",
        "runtime",
        super::telemetry::Privacy::Safe,
        serde_json::json!({"line": line.clone()}),
    );
}
