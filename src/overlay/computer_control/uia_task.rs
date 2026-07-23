//! UIA-grounded task harness (the workhorse). Each turn the model gets a
//! screenshot + a NUMBERED LIST of the REAL on-screen elements (Windows
//! accessibility = ground truth). It clicks BY INDEX; we click the element's
//! true coordinate (zero VLM localization error). After each action we re-read
//! UIA so the model verifies from ground truth, not pixels. Saves per-step
//! screenshots. `--cc-uia-task --cc-task "..."`.

use std::sync::{Arc, atomic::AtomicBool};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde_json::{Value, json};
use tungstenite::Message;

use crate::api::gemini_live::transport::{
    is_transient_socket_read_error, set_socket_nonblocking, set_socket_short_timeout,
};

use super::executor;
use super::grid::Grid;
use super::human_input::{self, HumanProfile};
use super::protocol::{
    ServerEvent, parse_server_message, realtime_text, realtime_video_jpeg_b64, tool_response,
};
use super::session::{self, Sock, View, connect_ws, send};
use super::uia::{self, UiElement};

mod anchors;
mod brain;
mod browser_dispatch;
mod dispatch;
mod dispatch_guard;
mod dispatch_telemetry;
mod exact_edit_guard;
mod executable_provenance;
mod frame_identity;
mod harness_options;
mod keyboard_target_gate;
mod perception;
mod postcondition;
mod prompt;
mod receipts;
mod render;
mod resource_authorization;
mod review;
mod setup_guard;
mod snapshot;
mod structural_authorization;
mod structural_edit;
mod tab_ownership;
#[cfg(test)]
mod turn_state_tests;
mod vision;
mod vision_verify;
use anchors::*;
pub(in crate::overlay::computer_control) use frame_identity::FrameSource;
use perception::*;
use postcondition::*;
pub(crate) use prompt::build_setup;
use render::*;
use review::*;
pub(super) use snapshot::{SnapshotFrame, snapshot};
use vision::*;
use vision_verify::*;

const SYS: &str = include_str!("uia_task/prompt_core.txt");

/// What a socket read yielded: a frame to process, nothing (skip), or an
/// unexpected close/error that should trigger a resumption reconnect.
enum ReadOutcome {
    Frame(String),
    Skip,
    Reconnect,
}

/// Reconnect the Live session, resuming the prior conversation by `resume` handle.
pub(super) fn reconnect(
    key: &str,
    resume: Option<&str>,
    voice: bool,
    search: bool,
    reconnect_context: Option<&str>,
) -> Result<Sock> {
    let mut s = connect_ws(key).context("reconnect")?;
    let setup = prompt::build_setup_with_context(resume, voice, search, reconnect_context);
    super::telemetry::record_model_setup(&setup, "reconnect");
    send(&mut s, setup)?;
    wait_for_setup(&mut s)?;
    set_socket_nonblocking(&mut s)?;
    Ok(s)
}

/// The shared agent "brain": per-session state + tool dispatch + grounding +
/// robustness. Owned by ONE thread so a slow humanized action can run while a
/// reader thread keeps receiving mic + barge-in (the voice runtime drives it
/// from its executor thread; the headless harness drives it inline).
pub(super) struct Brain {
    pub dir: String,
    grid: Grid,
    profile: HumanProfile,
    dry: bool,
    pub target: Option<String>,
    pub view: View,
    zoomed: bool,
    /// When set (the model called see_whole_screen), the base view is the WHOLE
    /// desktop for awareness; default false = the active window for precise acting.
    whole_screen: bool,
    last_click: Option<(i32, i32)>,
    pub step: usize,
    /// Immutable ids for the action currently being executed. Captures the
    /// originating turn so barge-in cannot relabel its click/frame evidence.
    active_action: Option<super::telemetry::ActionTrace>,
    /// Turn currently owning recovery and setup state. The executor advances
    /// this before every queued job so task-local evidence cannot cross turns.
    current_turn_id: Option<u64>,
    /// Identity of the exact latest frame the model could reason from. Direct
    /// input may use this identity only; foreground state at dispatch is not an
    /// implicit retarget signal.
    source_frame: Option<FrameSource>,
    /// Exact browser tab selected during this user turn. Never crosses turns.
    controlled_tab_id: Option<i64>,
    controlled_document_id: Option<String>,
    /// Browser tabs opened for incidental work in this user turn. They retire
    /// through an identity-bound, verified lease unless the model explicitly
    /// selected persistent lifetime.
    turn_tabs: tab_ownership::TurnTabOwnership,
    recent_actions: Vec<String>,
    /// Structural action+state signatures for which recovery vision was already
    /// attempted this turn. Prevents unchanged frames from producing repeated
    /// advice while still allowing a distinct action or state one bounded try.
    advice_latches: Vec<String>,
    prev_state_sig: Option<String>,
    /// Region snapshot taken JUST BEFORE a click (around the click point), so
    /// grounding can tell whether the click changed its own target cell — the only
    /// "did it register?" signal for canvas content UIA can't see.
    click_before: Option<Vec<u8>>,
    /// Compact "what I just did" trail (last few actions + outcomes) so the model
    /// keeps the thread of a multi-step task.
    trail: Vec<String>,
    /// Prevents a rejected exact-file transaction from being recreated through
    /// a broader mutation tool during the same turn.
    exact_edit_guard: exact_edit_guard::ExactEditGuard,
    /// Bounded user-authored request history used only by the independent
    /// structural-mutation request-contract checkpoint.
    structural_authorization: structural_authorization::StructuralAuthorization,
    resource_authorization: resource_authorization::ResourceAuthorization,
    /// Seconds spent in consecutive `wait` calls (reset by any other action), to
    /// tell the model how long it's been waiting on an async result.
    wait_accum: f64,
    /// Frame-owned, source-aware click anchors. Every mutating transition clears
    /// the set; IDs only increase within a session so a remap cannot silently make
    /// an old number mean a different target.
    anchors: Vec<ClickAnchor>,
    next_anchor_id: u32,
    /// The deterministic controller (resolve→execute→verify→gate) behind the
    /// observe/act/do_steps tools — drives the browser surface (and native windows
    /// via UIA), always on.
    controller: super::controller::Controller,
    /// Last-resort coordinate vocabulary for surfaces with no accessible
    /// actionable controls. Structured apps keep this off so the overlay cannot
    /// cover labels, values, or other evidence the model needs to read.
    show_coarse_grid: bool,
    setup_guard: setup_guard::SetupGuard,
    keyboard_target_gate: keyboard_target_gate::KeyboardTargetGate,
}

/// Result of grounding after an action: the frame to send, the textual state, and
/// one typed postcondition assessment to fold into the reply.
pub(super) struct Grounded {
    pub frame_b64: String,
    pub source: FrameSource,
    pub state_text: String,
    pub postcondition: GroundPostcondition,
}

struct SemanticSurfaceState {
    elements: String,
    title: String,
    url: String,
    identity: super::controller::world::SurfaceIdentity,
}

pub fn run(task: &str) -> Result<()> {
    let max_steps: usize = std::env::var("CC_MAX_STEPS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(16);
    let target = std::env::var("CC_UIA_WINDOW").ok();
    eprintln!("[cc] task={task:?} target={target:?} max_steps={max_steps}");

    // If a specific window was requested, raise it to the foreground (the agent
    // is scoped to it) and confirm it's real — otherwise we'd silently fall back
    // to the whole desktop and click random places.
    let pinned_target = if let Some(t) = &target {
        match uia::raise_window(t) {
            Ok(true) => {}
            Ok(false) => anyhow::bail!(
                "target window {t:?} was resolved but could not be verified as foreground"
            ),
            Err(error) => anyhow::bail!("cannot resolve target window {t:?}: {error}"),
        }
        std::thread::sleep(Duration::from_millis(500));
        let pinned = uia::pin_foreground_target()
            .ok_or_else(|| anyhow::anyhow!("focused window identity is unavailable"))?;
        match uia::target_window_rect(Some(&pinned)) {
            Some((x, y, w, h)) => eprintln!("[cc] target window rect ({x},{y},{w},{h})"),
            None => anyhow::bail!(
                "focused target window {t:?} is no longer visible or its stable identity changed"
            ),
        }
        Some(pinned)
    } else {
        None
    };

    let key = session::load_key()?;
    let mut socket = connect_ws(&key).context("connect")?;
    let setup = build_setup(None, false, false);
    super::telemetry::record_model_setup(&setup, "initial");
    send(&mut socket, setup)?;
    wait_for_setup(&mut socket)?;
    set_socket_nonblocking(&mut socket)?;
    // Resilience: the preview Live model intermittently drops the WS with
    // "invalid argument". Session-resumption handles are themselves rejected on
    // this preview model (setupComplete then immediate INVALID_ARGUMENT), so on
    // an unexpected close we reconnect to a FRESH session and re-seed the task +
    // current screen state — stateless and always valid, the task survives.
    let mut reconnects = 0u32;
    let mut forced_drop = false;
    const MAX_RECONNECTS: u32 = 6;

    let cancel = Arc::new(AtomicBool::new(false));
    let mut brain = Brain::new(pinned_target);
    brain.record_user_request(0, task);
    // Turn 0 (no pending tool): send the VIEW crop, then the state + task.
    let (b0, st0) = brain.initial()?;
    send(&mut socket, realtime_video_jpeg_b64(&b0))?;
    send(
        &mut socket,
        realtime_text(&format!("{st0}\n\nYOUR TASK: {task}\nBegin.")),
    )?;

    let deadline_secs: u64 = std::env::var("CC_DEADLINE_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(180);
    let deadline = Instant::now() + Duration::from_secs(deadline_secs);
    let mut reasoning = String::new();
    let mut tool_since_boundary = false;
    let mut stop_reason = "deadline_expired";
    'task_loop: while Instant::now() < deadline && brain.step < max_steps {
        // Test hook: simulate an unexpected drop at a given step to exercise the
        // resumption-reconnect path (CC_FORCE_DROP=<step>).
        if !forced_drop
            && let Ok(n) = std::env::var("CC_FORCE_DROP")
            && n.parse::<usize>().ok() == Some(brain.step)
        {
            forced_drop = true;
            eprintln!(
                "[cc] CC_FORCE_DROP: simulating connection drop at step {}",
                brain.step
            );
            let _ = socket.close(None);
        }
        let outcome = match socket.read() {
            Ok(Message::Text(t)) => ReadOutcome::Frame(t.to_string()),
            Ok(Message::Binary(b)) => match String::from_utf8(b.to_vec()) {
                Ok(s) => ReadOutcome::Frame(s),
                Err(_) => ReadOutcome::Skip,
            },
            Ok(Message::Close(f)) => {
                eprintln!("[cc] closed: {f:?}");
                ReadOutcome::Reconnect
            }
            Ok(_) => ReadOutcome::Skip,
            Err(e) if is_transient_socket_read_error(&e) => ReadOutcome::Skip,
            Err(e) => {
                eprintln!("[cc] read error: {e}");
                ReadOutcome::Reconnect
            }
        };
        let frame = match outcome {
            ReadOutcome::Frame(f) => {
                reconnects = 0; // healthy read — reset the budget
                f
            }
            ReadOutcome::Skip => continue,
            ReadOutcome::Reconnect => {
                reconnects += 1;
                if reconnects > MAX_RECONNECTS {
                    eprintln!("[cc] giving up after {MAX_RECONNECTS} reconnects");
                    break;
                }
                eprintln!("[cc] reconnecting #{reconnects} (fresh session + re-seed)");
                match reconnect(&key, None, false, false, None) {
                    Ok(s) => socket = s,
                    Err(e) => {
                        eprintln!("[cc] reconnect failed: {e}");
                        break;
                    }
                }
                // Fresh session lost server-side history — re-seed the task +
                // current state (like turn 0, which is always valid).
                let g = brain.ground("(reconnect)", &json!({}))?;
                send(&mut socket, realtime_video_jpeg_b64(&g.frame_b64))?;
                send(
                    &mut socket,
                    realtime_text(&format!(
                        "(reconnected after a dropped connection) Resume this task: {task}\nContinue from the CURRENT \
state shown below.\n{}",
                        g.state_text
                    )),
                )?;
                continue;
            }
        };
        for ev in parse_server_message(&frame) {
            match ev {
                ServerEvent::ModelText(t) | ServerEvent::OutputTranscript(t) => {
                    reasoning.push_str(&t)
                }
                ServerEvent::ToolCall { id, name, args } => {
                    tool_since_boundary = true;
                    let say = reasoning.trim().to_string();
                    if !say.is_empty() {
                        eprintln!("[cc] step {:02} SAYS: {say}", brain.step + 1);
                    }
                    reasoning.clear();
                    // Context handed to the (otherwise stateless) vision model so it
                    // knows the task + why it's looking — disambiguates vague
                    // descriptions ("the other one").
                    let ctx = format!(
                        "task: {task}; agent intent: {}",
                        if say.is_empty() {
                            "(none stated)"
                        } else {
                            say.as_str()
                        }
                    );

                    if name == "done" {
                        let summary = args
                            .get("summary")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .trim()
                            .chars()
                            .take(320)
                            .collect::<String>();
                        brain.retire_session_completed();
                        send(
                            &mut socket,
                            tool_response(
                                &id,
                                &name,
                                json!({
                                    "ok": true,
                                    "completion_status": "model_declared",
                                    "summary": summary,
                                }),
                            ),
                        )?;
                        super::telemetry::event(
                            "turn_summary",
                            "runtime",
                            super::telemetry::Privacy::Safe,
                            json!({"outcome": "done", "steps": brain.step}),
                        );
                        record_focused_session_end("completed", "accepted_done", brain.step);
                        return Ok(());
                    }

                    let action_result = brain.dispatch(&name, &args, &ctx, &cancel, None, false);
                    let g = brain.ground(&name, &args)?;
                    let execution_ok = action_result.get("ok").and_then(Value::as_bool);
                    let mutating = super::turn_policy::is_mutating_tool(&name);
                    let effect_status = super::effect_receipt::EffectStatus::after_dispatch(
                        &action_result,
                        mutating,
                    );
                    let recovery_advice = (execution_ok != Some(false)
                        && g.postcondition.request_advice())
                    .then(|| brain.stuck_advice(task, &cancel))
                    .flatten();
                    let postcondition = g.postcondition.response(
                        execution_ok,
                        mutating,
                        effect_status,
                        recovery_advice,
                    );
                    let mut resp = json!({
                        "action_result": action_result,
                        "execution_ok": execution_ok,
                        "new_state": g.state_text,
                        "postcondition": postcondition,
                    });
                    if execution_ok == Some(false)
                        || (g.postcondition.detected_no_effect() && !effect_status.is_verified())
                    {
                        resp["ok"] = json!(false);
                    } else if let Some(ok) = execution_ok {
                        resp["ok"] = json!(ok);
                    }
                    effect_status.annotate(&mut resp);
                    send(&mut socket, tool_response(&id, &name, resp))?; // answer first
                    send(&mut socket, realtime_video_jpeg_b64(&g.frame_b64))?; // then the new frame
                }
                ServerEvent::TurnComplete => {
                    match classify_autonomous_boundary(
                        &mut tool_since_boundary,
                        !reasoning.trim().is_empty(),
                    ) {
                        AutonomousBoundary::ToolGeneration => continue,
                        AutonomousBoundary::Complete => {
                            let text = reasoning.trim();
                            eprintln!(
                                "[cc] final response: {}",
                                text.chars().take(240).collect::<String>()
                            );
                            brain.retire_session_completed();
                            super::telemetry::event(
                                "turn_summary",
                                "runtime",
                                super::telemetry::Privacy::Safe,
                                json!({"outcome": "model_turn_complete", "steps": brain.step}),
                            );
                            record_focused_session_end(
                                "completed",
                                "model_turn_complete",
                                brain.step,
                            );
                            return Ok(());
                        }
                        AutonomousBoundary::SilentStop => {}
                    }
                    reasoning.clear();
                    stop_reason = "silent_turn_complete_without_tool";
                    super::telemetry::typed_error(
                        "ERR_AUTONOMOUS_TURN_SILENT",
                        "runtime",
                        "model ended an autonomous task turn without a tool or user-visible response",
                        json!({"steps": brain.step}),
                    );
                    break 'task_loop;
                }
                _ => {}
            }
        }
    }
    if brain.step >= max_steps {
        stop_reason = "max_actions_reached";
    }
    eprintln!("[cc] STOPPED at step {} ({stop_reason})", brain.step,);
    brain.final_review(task, stop_reason);
    record_focused_session_end("failed", stop_reason, brain.step);
    anyhow::bail!("computer-control task stopped without accepted done: {stop_reason}")
}

fn record_focused_session_end(outcome: &str, reason: &str, steps: usize) {
    super::telemetry::event(
        "session_end",
        "runtime",
        super::telemetry::Privacy::Safe,
        json!({"outcome": outcome, "reason": reason, "steps": steps}),
    );
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AutonomousBoundary {
    ToolGeneration,
    Complete,
    SilentStop,
}

fn classify_autonomous_boundary(
    tool_since_boundary: &mut bool,
    output_seen: bool,
) -> AutonomousBoundary {
    if *tool_since_boundary {
        *tool_since_boundary = false;
        return AutonomousBoundary::ToolGeneration;
    }
    if output_seen {
        AutonomousBoundary::Complete
    } else {
        AutonomousBoundary::SilentStop
    }
}

/// CLI: read the foreground window with the aux vision stack and print the
/// answer (validates chain resolution / keys / provider dispatch). `--cc-vision-test`.
pub fn run_vision_test(target: Option<&str>, question: &str) -> Result<()> {
    // Start the Gemini Live worker pool so a gemini-live vision model
    // (CC_VISION_MODEL=google-gemini-3-1-live-vision) is reachable from this CLI - it
    // routes through that worker (image-attach -> audio -> outputTranscription).
    crate::api::gemini_live::init_gemini_live();
    std::thread::sleep(Duration::from_millis(200));
    let view = window_view(target, false);
    eprintln!(
        "[vision-test] reading view ({},{},{},{})",
        view.x, view.y, view.w, view.h
    );
    let never = AtomicBool::new(false);
    let answer = read_view(view, question, "", &never)?;
    eprintln!("[vision-test] ANSWER:\n{answer}");
    if let Ok(desc) = std::env::var("CC_LOCATE")
        && !desc.trim().is_empty()
    {
        let loc = locate_in_view(view, &desc, "", &never)?;
        let (sx, sy) = view.to_screen_px(loc.x, loc.y);
        eprintln!(
            "[vision-test] LOCATE '{desc}' -> view_norm({:.0},{:.0}) screen_px({sx},{sy}) saw={:?}",
            loc.x, loc.y, loc.note
        );
    }
    Ok(())
}

/// CLI: capture one frame with the Set-of-Mark grid overlaid and save it, so we
/// can eyeball label legibility / tune `CC_GRID_COLS`/`CC_GRID_ROWS`. No model.
pub fn run_grid_test(target: Option<&str>) -> Result<()> {
    let grid = Grid::from_env();
    let view = window_view(target, false);
    let cap = session::capture_virtual()?;
    let (jpeg, shown) = session::encode_view(&cap, view, VIEW_SHORT, Some(grid), None, None)?;
    let dir = std::env::var("CC_TRACE_DIR").unwrap_or_else(|_| "cc-grid".to_string());
    std::fs::create_dir_all(&dir).ok();
    std::fs::write(format!("{dir}/grid.jpg"), &jpeg)?;
    eprintln!(
        "[grid-test] grid {}x{} ({} cells); view ({},{},{},{}); saved {dir}/grid.jpg",
        grid.cols,
        grid.rows,
        grid.cell_count(),
        shown.x,
        shown.y,
        shown.w,
        shown.h
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{AutonomousBoundary, classify_autonomous_boundary};

    #[test]
    fn autonomous_boundary_finishes_once_without_manufacturing_a_new_user_turn() {
        let mut tool = true;
        assert_eq!(
            classify_autonomous_boundary(&mut tool, true),
            AutonomousBoundary::ToolGeneration
        );
        assert_eq!(
            classify_autonomous_boundary(&mut tool, true),
            AutonomousBoundary::Complete
        );
        assert_eq!(
            classify_autonomous_boundary(&mut tool, false),
            AutonomousBoundary::SilentStop
        );
    }
}
