//! UIA-grounded task harness (the workhorse). Each turn the model gets a
//! screenshot + a NUMBERED LIST of the REAL on-screen elements (Windows
//! accessibility = ground truth). It clicks BY INDEX; we click the element's
//! true coordinate (zero VLM localization error). After each action we re-read
//! UIA so the model verifies from ground truth, not pixels. Saves per-step
//! screenshots. `--cc-uia-task --cc-task "..."`.

use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use base64::{Engine as _, engine::general_purpose};
use serde_json::{Value, json};
use tungstenite::Message;

use crate::api::realtime_audio::websocket::{
    is_transient_socket_read_error, set_socket_nonblocking, set_socket_short_timeout,
};

use super::executor;
use super::grid::Grid;
use super::human_input::{self, HumanProfile};
use super::vision_reader::Located;
use super::protocol::{
    ServerEvent, parse_server_message, realtime_text, realtime_video_jpeg_b64, tool_response,
};
use super::session::{self, Sock, View, connect_ws, send};
use super::uia::{self, UiElement};

const SYS: &str = "You control a Windows PC, ONE tool action per turn. Each turn you get the focused window's \
READOUTS and CLICKABLE elements (Windows accessibility = ground truth, each tagged @cellN = its grid cell) and a \
SCREENSHOT with a NUMBERED GRID over it. The view follows the foreground window. \
click_at(cell): click that grid number. zoom(cell): magnify it (grid redrawn with new numbers); reset_view undoes \
it. click_element(name): click a listed element. Also type_text, key_combination. \
A board/canvas has NO element: READ it with look(question) before deciding (never guess), and CLICK it with \
click_target(description) - a high-res model locates the exact pixel, far more accurate than click_at for small/ \
precise targets. Use zoom + click_at(cell) only for coarse navigation. \
Open a web page: open_url(url) opens it as a new foreground tab. Launch an app: launch_app(name). These OS-level \
tools beat driving the Start menu / address bar by keystrokes - prefer them. Wait for slow/async results (image \
generation, page loads) with wait(seconds). \
Report ONLY what the screenshot shows; if it is not what you expected, say so and correct course. \
NEVER judge the screen state or claim done from your own low-res view - call look() and QUOTE what it says; your \
own view is unreliable for fine detail. Do NOT call look() twice without acting in between. \
Each turn: one line on what you SEE, then ONE tool. Call done only when a fresh look() confirms the goal (an \
independent check will verify).";

fn build_setup(resume: Option<&str>) -> Value {
    let think = std::env::var("CC_THINK").unwrap_or_else(|_| "medium".to_string());
    // On a reconnect, resume the prior session by its handle so the server
    // restores the full conversation (survives an intermittent server drop).
    let resumption = match resume {
        Some(h) => json!({ "handle": h }),
        None => json!({}),
    };
    json!({ "setup": {
        "model": format!("models/{}", super::protocol::MODEL),
        "generationConfig": {
            "responseModalities": ["AUDIO"],
            "speechConfig": {"voiceConfig": {"prebuiltVoiceConfig": {"voiceName": "Aoede"}}},
            "mediaResolution": "MEDIA_RESOLUTION_HIGH",
            "thinkingConfig": {"thinkingLevel": think}
        },
        "systemInstruction": {"parts": [{"text": SYS}]},
        "tools": [{"functionDeclarations": [
            {"name": "click_element", "description": "Click the UI element with this exact name (copied verbatim from the element list).",
             "parameters": {"type": "object", "properties": {"name": {"type": "string"}}, "required": ["name"]}},
            {"name": "click_at", "description": "Click the CENTER of the numbered GRID CELL shown over the current screenshot. Pass the cell's printed number. Use for targets NOT in the element list, e.g. a game board, canvas, or image.",
             "parameters": {"type": "object", "properties": {"cell": {"type": "integer", "description": "The grid number printed over the target."}}, "required": ["cell"]}},
            {"name": "zoom", "description": "Magnify the numbered GRID CELL so small targets become large and a fresh finer grid is drawn over it. Pass the cell's printed number.",
             "parameters": {"type": "object", "properties": {"cell": {"type": "integer", "description": "The grid number to magnify."}}, "required": ["cell"]}},
            {"name": "reset_view", "description": "Return the view to the whole focused window (undo zoom).",
             "parameters": {"type": "object", "properties": {}}},
            {"name": "look", "description": "Get a precise HIGH-RESOLUTION reading of what is currently on screen, for content you cannot read clearly yourself (game boards, charts, images, tiny text). Ask a specific question; a dedicated vision model answers from a clean high-res capture of the current view. Use this to read a board/canvas state before deciding a move, and to check results.",
             "parameters": {"type": "object", "properties": {"question": {"type": "string", "description": "What to read, e.g. 'List each of the 9 tic-tac-toe cells row by row as X, O, or empty.'"}}, "required": ["question"]}},
            {"name": "click_target", "description": "Click a target described in plain words; a high-resolution vision model locates its EXACT pixel and we click there. Use this for PRECISE clicks on canvas/board/image/button targets instead of click_at(cell) - it is far more accurate because it does not round to a grid cell. Set button='right' to open a context menu (e.g. to 'Save image as').",
             "parameters": {"type": "object", "properties": {"description": {"type": "string", "description": "Unambiguous target, e.g. 'the generated chicken image' or 'the download button'."}, "button": {"type": "string", "enum": ["left", "right"], "description": "left (default) or right for a context menu."}}, "required": ["description"]}},
            {"name": "wait", "description": "Pause for N seconds, for slow or asynchronous operations (e.g. waiting for an image to finish generating or a page to load). Then re-observe.",
             "parameters": {"type": "object", "properties": {"seconds": {"type": "number", "description": "Seconds to wait (max 30)."}}, "required": ["seconds"]}},
            {"name": "type_text", "description": "Type text at the current keyboard focus.",
             "parameters": {"type": "object", "properties": {"text": {"type": "string"}}, "required": ["text"]}},
            {"name": "key_combination", "description": "Press a keyboard shortcut, e.g. Enter, Control+C, Alt+Tab.",
             "parameters": {"type": "object", "properties": {"keys": {"type": "string"}}, "required": ["keys"]}},
            {"name": "open_url", "description": "Open an http(s) URL in the default browser as a NEW foreground tab (via the OS shell). Use this to go to a web page directly - far more reliable than typing into the address bar.",
             "parameters": {"type": "object", "properties": {"url": {"type": "string"}}, "required": ["url"]}},
            {"name": "launch_app", "description": "Launch or focus a Windows application by name/path via the OS shell, e.g. 'chrome', 'notepad', 'explorer'. More reliable than the Win+type Start-menu method.",
             "parameters": {"type": "object", "properties": {"name": {"type": "string"}}, "required": ["name"]}},
            {"name": "done", "description": "Call ONLY when the goal is confirmed achieved; quote the evidence.",
             "parameters": {"type": "object", "properties": {"summary": {"type": "string"}}, "required": ["summary"]}}
        ]}],
        "inputAudioTranscription": {},
        "outputAudioTranscription": {},
        "sessionResumption": resumption,
        "contextWindowCompression": {"slidingWindow": {}}
    }})
}

/// What a socket read yielded: a frame to process, nothing (skip), or an
/// unexpected close/error that should trigger a resumption reconnect.
enum ReadOutcome {
    Frame(String),
    Skip,
    Reconnect,
}

/// Reconnect the Live session, resuming the prior conversation by `resume` handle.
fn reconnect(key: &str, resume: Option<&str>) -> Result<Sock> {
    let mut s = connect_ws(key).context("reconnect")?;
    send(&mut s, build_setup(resume))?;
    wait_for_setup(&mut s)?;
    set_socket_nonblocking(&mut s)?;
    Ok(s)
}

pub fn run(task: &str) -> Result<()> {
    let dir = std::env::var("CC_TRACE_DIR").unwrap_or_else(|_| "cc-uia".to_string());
    std::fs::create_dir_all(&dir).ok();
    let dry = std::env::var("CC_DRY").is_ok();
    let max_steps: usize = std::env::var("CC_MAX_STEPS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(16);
    let target = std::env::var("CC_UIA_WINDOW").ok();
    eprintln!("[uia-task] task={task:?} dry={dry} target={target:?} max_steps={max_steps}");

    // If a specific window was requested, raise it to the foreground (the agent
    // is scoped to it) and confirm it's real — otherwise we'd silently fall back
    // to the whole desktop and click random places.
    if let Some(t) = &target {
        uia::raise_window(t);
        std::thread::sleep(Duration::from_millis(500));
        match uia::target_window_rect(Some(t)) {
            Some((x, y, w, h)) => eprintln!("[uia-task] target window rect ({x},{y},{w},{h})"),
            None => anyhow::bail!(
                "target window {t:?} not found or not visible — open it and make sure that tab/window is the \
active, foreground one (its title must contain {t:?})"
            ),
        }
    }

    let key = session::load_key()?;
    let mut socket = connect_ws(&key).context("connect")?;
    send(&mut socket, build_setup(None))?;
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

    let grid = Grid::from_env();
    eprintln!("[uia-task] grid {}x{} ({} cells)", grid.cols, grid.rows, grid.cell_count());
    // Humanization (CC_HUMANIZE) + a cancel flag. In this turn-based harness the
    // flag is never set (no live voice); it makes the human-input + wait paths
    // cancellable so the same code serves the voice runtime's steer/stop.
    let profile = HumanProfile::from_env();
    let cancel = AtomicBool::new(false);
    let mut step = 0usize;
    // The view follows the foreground window each turn (so the model can launch
    // an app and the view tracks it), UNLESS the model has zoomed in.
    let mut zoomed = false;
    // Click-accuracy trace: the last click's screen px (drawn on the next frame as
    // a red marker) and a JSONL log of every click for post-hoc debugging.
    let mut last_click: Option<(i32, i32)> = None;
    // #1 stuck-loop detection + #2 state-delta: track recent actions and the
    // previous accessible-UI signature so we can warn the model when it repeats
    // an action or when nothing changed (a likely non-registering click).
    let mut recent_actions: Vec<String> = Vec::new();
    let mut prev_state_sig: Option<String> = None;
    let mut view = window_view(target.as_deref());
    // Turn 0 (no pending tool): send the VIEW crop, then the state + task.
    let mut elements = uia::enumerate(target.as_deref()).unwrap_or_default();
    let (b0, v0) = render_view(&dir, step, view, grid, None)?;
    view = v0;
    send(&mut socket, realtime_video_jpeg_b64(&b0))?;
    eprintln!("[uia-task] step 00 READOUTS: {}", readouts_inline(&elements));
    {
        let mut msg = format_state(&elements, target.as_deref(), view, grid);
        msg.push_str(&format!("\n\nYOUR TASK: {task}\nBegin."));
        send(&mut socket, realtime_text(&msg))?;
    }

    let deadline_secs: u64 = std::env::var("CC_DEADLINE_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(180);
    let deadline = Instant::now() + Duration::from_secs(deadline_secs);
    let mut reasoning = String::new();
    while Instant::now() < deadline && step < max_steps {
        // Test hook: simulate an unexpected drop at a given step to exercise the
        // resumption-reconnect path (CC_FORCE_DROP=<step>).
        if !forced_drop
            && let Ok(n) = std::env::var("CC_FORCE_DROP")
            && n.parse::<usize>().ok() == Some(step)
        {
            forced_drop = true;
            eprintln!("[uia-task] CC_FORCE_DROP: simulating connection drop at step {step}");
            let _ = socket.close(None);
        }
        let outcome = match socket.read() {
            Ok(Message::Text(t)) => ReadOutcome::Frame(t.to_string()),
            Ok(Message::Binary(b)) => match String::from_utf8(b.to_vec()) {
                Ok(s) => ReadOutcome::Frame(s),
                Err(_) => ReadOutcome::Skip,
            },
            Ok(Message::Close(f)) => {
                eprintln!("[uia-task] closed: {f:?}");
                ReadOutcome::Reconnect
            }
            Ok(_) => ReadOutcome::Skip,
            Err(e) if is_transient_socket_read_error(&e) => ReadOutcome::Skip,
            Err(e) => {
                eprintln!("[uia-task] read error: {e}");
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
                    eprintln!("[uia-task] giving up after {MAX_RECONNECTS} reconnects");
                    break;
                }
                eprintln!("[uia-task] reconnecting #{reconnects} (fresh session + re-seed)");
                match reconnect(&key, None) {
                    Ok(s) => socket = s,
                    Err(e) => {
                        eprintln!("[uia-task] reconnect failed: {e}");
                        break;
                    }
                }
                // Fresh session lost server-side history — re-seed the task +
                // current state (like turn 0, which is always valid).
                if !zoomed {
                    view = window_view(target.as_deref());
                }
                elements = uia::enumerate(target.as_deref()).unwrap_or_default();
                let (b, v) = render_view(&dir, step, view, grid, last_click)?;
                view = v;
                let st = format_state(&elements, target.as_deref(), view, grid);
                send(&mut socket, realtime_video_jpeg_b64(&b))?;
                send(&mut socket, realtime_text(&format!(
                    "(reconnected after a dropped connection) Resume this task: {task}\nContinue from the CURRENT \
state shown below.\n{st}"
                )))?;
                continue;
            }
        };
        for ev in parse_server_message(&frame) {
            match ev {
                ServerEvent::ModelText(t) | ServerEvent::OutputTranscript(t) => reasoning.push_str(&t),
                ServerEvent::ToolCall { id, name, args } => {
                    step += 1;
                    let say = reasoning.trim().to_string();
                    if !say.is_empty() {
                        eprintln!("[uia-task] step {step:02} SAYS: {say}");
                    }
                    reasoning.clear();
                    // Context handed to the (otherwise stateless) vision model so
                    // it knows the task + why it's looking — disambiguates vague
                    // descriptions ("the other one").
                    let ctx = format!(
                        "task: {task}; agent intent: {}",
                        if say.is_empty() { "(none stated)" } else { say.as_str() }
                    );

                    if name == "done" {
                        let s = args.get("summary").and_then(|v| v.as_str()).unwrap_or("");
                        // Verify INDEPENDENTLY with the high-res vision model (the
                        // Live agent confabulates success, so it cannot judge itself).
                        let full = window_view(target.as_deref());
                        let q = format!(
                            "A computer agent claims this task is COMPLETE: \"{task}\". Looking ONLY at this \
screenshot, is that goal actually achieved right now? Start your answer with YES or NO, then quote the exact \
on-screen evidence (or state what is actually shown instead)."
                        );
                        match read_view(full, &q, &ctx) {
                            Ok(answer) => {
                                let yes = answer.trim_start().to_lowercase().starts_with("yes");
                                eprintln!("[uia-task] step {step:02} DONE-claim: {s}\n[uia-task]   vision verdict: {answer}");
                                if yes {
                                    final_review(&dir, target.as_deref(), &answer);
                                    send(&mut socket, tool_response(&id, &name, json!({"ok": true})))?;
                                    return Ok(());
                                }
                                if !zoomed {
                                    view = window_view(target.as_deref());
                                }
                                elements = uia::enumerate(target.as_deref()).unwrap_or_default();
                                let (b, v) = render_view(&dir, step, view, grid, last_click)?;
                                view = v;
                                let resp = json!({
                                    "ok": false,
                                    "independent_check": answer,
                                    "instruction": "An independent high-res check says the goal is NOT yet \
achieved (see above). Do not finish - keep working until it is actually done.",
                                    "new_state": format_state(&elements, target.as_deref(), view, grid),
                                });
                                send(&mut socket, tool_response(&id, &name, resp))?; // answer first
                                send(&mut socket, realtime_video_jpeg_b64(&b))?; // then frame
                                continue;
                            }
                            Err(e) => {
                                // Don't trap the agent if the checker is unavailable.
                                eprintln!("[uia-task] step {step:02} DONE (vision check unavailable: {e}): {s}");
                                send(&mut socket, tool_response(&id, &name, json!({"ok": true})))?;
                                return Ok(());
                            }
                        }
                    }

                    let action_result = match name.as_str() {
                        "click_element" => {
                            let want = args.get("name").and_then(Value::as_str).unwrap_or("");
                            let r = click_by_name(&elements, want, dry, &profile, &cancel);
                            if let Some(p) = r.get("screen_px").and_then(|v| v.as_array())
                                && p.len() == 2
                            {
                                let (sx, sy) = (p[0].as_i64().unwrap_or(0) as i32, p[1].as_i64().unwrap_or(0) as i32);
                                last_click = Some((sx, sy));
                                append_click(&dir, json!({"step": step, "kind": "click_element", "name": want, "screen_px": [sx, sy]}));
                            }
                            r
                        }
                        "click_at" => {
                            let cell = args.get("cell").and_then(Value::as_u64).unwrap_or(0) as u32;
                            match grid.center_norm(cell) {
                                Some((mx, my)) => {
                                    let (sx, sy) = view.to_screen_px(mx, my);
                                    last_click = Some((sx, sy));
                                    append_click(&dir, json!({"step": step, "kind": "click_at", "cell": cell,
                                        "view_norm": [mx.round(), my.round()], "screen_px": [sx, sy],
                                        "view": [view.x, view.y, view.w, view.h]}));
                                    click_screen(sx, sy, dry, "left", &profile, &cancel)
                                }
                                None => json!({"ok": false,
                                    "error": format!("cell {cell} out of range 1..={}", grid.cell_count())}),
                            }
                        }
                        "zoom" => {
                            let cell = args.get("cell").and_then(Value::as_u64).unwrap_or(0) as u32;
                            match zoom_to_cell(view, &grid, cell) {
                                Some(v) => {
                                    view = v;
                                    zoomed = true;
                                    json!({"ok": true, "zoomed_cell": cell})
                                }
                                None => json!({"ok": false,
                                    "error": format!("cell {cell} out of range 1..={}", grid.cell_count())}),
                            }
                        }
                        "reset_view" => {
                            zoomed = false;
                            json!({"ok": true, "view": "whole window"})
                        }
                        "look" => {
                            let q = args
                                .get("question")
                                .and_then(Value::as_str)
                                .unwrap_or("Describe exactly what is on screen.");
                            match read_view(view, q, &ctx) {
                                Ok(answer) => {
                                    eprintln!("[uia-task] step {step:02} LOOK: {answer}");
                                    json!({"ok": true, "reading": answer})
                                }
                                Err(e) => {
                                    eprintln!("[uia-task] step {step:02} LOOK failed: {e}");
                                    json!({"ok": false, "error": format!("vision read failed: {e}")})
                                }
                            }
                        }
                        "click_target" => {
                            let desc = args.get("description").and_then(Value::as_str).unwrap_or("");
                            let button = match args.get("button").and_then(Value::as_str) {
                                Some("right") => "right",
                                _ => "left",
                            };
                            match locate_in_view(view, desc, &ctx) {
                                Ok(loc) => {
                                    let (sx, sy) = view.to_screen_px(loc.x, loc.y);
                                    last_click = Some((sx, sy));
                                    append_click(&dir, json!({"step": step, "kind": "click_target", "desc": desc,
                                        "button": button, "view_norm": [loc.x.round(), loc.y.round()],
                                        "screen_px": [sx, sy], "saw": loc.note,
                                        "view": [view.x, view.y, view.w, view.h]}));
                                    eprintln!("[uia-task] step {step:02} CLICK_TARGET[{button}] '{desc}' -> view({:.0},{:.0}) screen({sx},{sy}) saw={:?}", loc.x, loc.y, loc.note);
                                    let r = click_screen(sx, sy, dry, button, &profile, &cancel);
                                    // Report what the vision model saw at the target so the Live
                                    // model knows its state without a separate look (#4).
                                    json!({"ok": true, "located_view_norm": [loc.x, loc.y], "saw_at_target": loc.note, "click": r})
                                }
                                Err(e) => {
                                    eprintln!("[uia-task] step {step:02} CLICK_TARGET '{desc}' failed: {e}");
                                    json!({"ok": false, "error": format!("could not locate '{desc}': {e}")})
                                }
                            }
                        }
                        "wait" => {
                            let secs = args.get("seconds").and_then(Value::as_f64).unwrap_or(3.0).clamp(0.0, 30.0);
                            eprintln!("[uia-task] step {step:02} WAIT {secs}s");
                            let aborted = human_input::sleep_cancellable((secs * 1000.0) as u64, &cancel);
                            json!({"ok": !aborted, "waited_seconds": secs})
                        }
                        "type_text" | "key_combination" | "open_url" | "launch_app" => {
                            if dry {
                                json!({"ok": true, "note": "dry"})
                            } else {
                                executor::execute_ex(&name, &args, &profile, &cancel)
                            }
                        }
                        _ => json!({"ok": false, "error": "unknown action"}),
                    };
                    eprintln!("[uia-task] step {step:02} {name}({args}) -> {action_result}");
                    // App launches / page loads need longer to settle than a click.
                    let settle = if name == "open_url" || name == "launch_app" { 1800 } else { 450 };
                    std::thread::sleep(Duration::from_millis(settle));

                    // Re-read state. ANSWER the tool first (state is in the response),
                    // THEN push the fresh frame — sending realtimeInput before the
                    // response can trip the INVALID_ARGUMENT abort.
                    // Unless zoomed, re-resolve the view to the current foreground
                    // window so it tracks app launches / focus changes.
                    if !zoomed {
                        view = window_view(target.as_deref());
                    }
                    elements = uia::enumerate(target.as_deref()).unwrap_or_default();
                    let (b, v) = render_view(&dir, step, view, grid, last_click)?;
                    view = v;
                    eprintln!("[uia-task] step {step:02} READOUTS: {}", readouts_inline(&elements));

                    // #2 state-delta: did the accessible UI change after this action?
                    // Only meaningful for actions that SHOULD move the UIA tree —
                    // canvas clicks (click_target/click_at) change pixels the tree
                    // can't see, so a "none" there is noise (rely on saw_at_target).
                    let new_sig = state_signature(&elements);
                    let ui_changed = prev_state_sig.as_deref() != Some(new_sig.as_str());
                    prev_state_sig = Some(new_sig);
                    let uia_action = matches!(
                        name.as_str(),
                        "click_element" | "type_text" | "key_combination" | "open_url" | "launch_app"
                    );
                    // #1 stuck-loop: same action repeated (non-consecutively) in a
                    // window — counts occurrences so interspersed look()s don't hide it.
                    let act_sig = format!("{name}|{}", compact_args(&args));
                    recent_actions.push(act_sig.clone());
                    if recent_actions.len() > 8 {
                        recent_actions.remove(0);
                    }
                    let stuck = recent_actions.iter().filter(|a| **a == act_sig).count() >= 3;

                    let mut resp = json!({
                        "action_result": action_result,
                        "new_state": format_state(&elements, target.as_deref(), view, grid),
                    });
                    if uia_action && !ui_changed {
                        resp["ui_change"] = json!("none - the accessible UI did not change after this action; it may \
not have registered.");
                    }
                    if stuck {
                        eprintln!("[uia-task] step {step:02} STUCK: repeated '{act_sig}'");
                        resp["stuck_warning"] = json!("You have repeated the same action ~3 times with no progress \
(the target likely isn't where you think, or the click isn't landing). Change approach: zoom in for a closer look, \
use a more specific click_target description, or restart the game/page.");
                    }
                    send(&mut socket, tool_response(&id, &name, resp))?; // answer first
                    send(&mut socket, realtime_video_jpeg_b64(&b))?; // then the new frame
                }
                _ => {}
            }
        }
    }
    eprintln!("[uia-task] STOPPED at step {step} (timeout/max-steps without done)");
    final_review(&dir, target.as_deref(), "(stopped without done)");
    Ok(())
}

/// Longest edge target for the view crop sent to the model (short edge actually).
const VIEW_SHORT: u32 = 1024;

/// Short-edge size for the CLEAN crop sent to the aux vision reader. Larger than
/// the Live frame (the reader is not token-capped) so fine detail survives.
const VISION_SHORT: u32 = 1600;

/// Read the current view with the aux vision stack (clean crop, no grid overlay).
/// `ctx` is task/intent context for disambiguation. Returns the plain answer.
fn read_view(view: View, question: &str, ctx: &str) -> Result<String> {
    let cap = session::capture_virtual()?;
    let (jpeg, _shown) = session::encode_view(&cap, view, VISION_SHORT, None, None)?;
    super::vision_reader::read_image(&jpeg, question, ctx)
}

/// Ask the aux vision stack for the click point of `description`, returned as
/// 0-1000 over `view` (+ what's there). DEFAULT: two-call coarse-to-fine
/// (accurate on small adjacent cells). `CC_LOCATE_MODE=box` uses a single
/// bounding-box call (faster, but mis-locates tiny cells — large targets only).
fn locate_in_view(view: View, description: &str, ctx: &str) -> Result<Located> {
    let cap = session::capture_virtual()?;
    let (jpeg, _s) = session::encode_view(&cap, view, VISION_SHORT, None, None)?;
    if std::env::var("CC_LOCATE_MODE").as_deref() == Ok("box") {
        return match super::vision_reader::locate_box(&jpeg, description, ctx) {
            Ok(p) => {
                eprintln!("[uia-task] locate box: ({:.0},{:.0})", p.x, p.y);
                Ok(p)
            }
            Err(e) => {
                eprintln!("[uia-task] box locate failed ({e}); falling back to point");
                super::vision_reader::locate_point(&jpeg, description, ctx)
            }
        };
    }
    refine_in_view(&cap, view, &jpeg, description, ctx)
}

/// Two-call coarse-to-fine locate: point over the whole view, then ZOOM a box
/// around it and point again so the target fills the frame.
fn refine_in_view(
    cap: &session::Capture,
    view: View,
    coarse_jpeg: &[u8],
    description: &str,
    ctx: &str,
) -> Result<Located> {
    let coarse = super::vision_reader::locate_point(coarse_jpeg, description, ctx)?;
    let (csx, csy) = view.to_screen_px(coarse.x, coarse.y);
    let zw = (view.w / 4).max(160);
    let zh = (view.h / 4).max(120);
    let zoom = View { x: csx - zw / 2, y: csy - zh / 2, w: zw, h: zh };
    let Ok((fine_jpeg, shown)) = session::encode_view(cap, zoom, VISION_SHORT, None, None) else {
        return Ok(coarse);
    };
    // The fine pass is easy localization (target fills the zoomed crop), so an
    // optional faster model (CC_VISION_FINE_MODEL) can do it — falling back to
    // the accurate default if it fails. Stateless; never loses correctness.
    let fine = match std::env::var("CC_VISION_FINE_MODEL") {
        Ok(m) if !m.trim().is_empty() => {
            super::vision_reader::locate_point_with(&fine_jpeg, description, m.trim(), ctx)
        }
        _ => super::vision_reader::locate_point(&fine_jpeg, description, ctx),
    };
    match fine {
        Ok(f) => {
            let (fsx, fsy) = shown.to_screen_px(f.x, f.y);
            let mx = ((fsx - view.x) as f64 / view.w.max(1) as f64 * 1000.0).clamp(0.0, 1000.0);
            let my = ((fsy - view.y) as f64 / view.h.max(1) as f64 * 1000.0).clamp(0.0, 1000.0);
            eprintln!("[uia-task] locate refine: coarse({:.0},{:.0}) -> fine({mx:.0},{my:.0})", coarse.x, coarse.y);
            Ok(Located { x: mx, y: my, note: f.note.or(coarse.note) })
        }
        Err(_) => Ok(coarse),
    }
}

/// Append one click record to `{dir}/clicks.jsonl` (the click-accuracy trace).
fn append_click(dir: &str, rec: Value) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(format!("{dir}/clicks.jsonl"))
    {
        let _ = writeln!(f, "{rec}");
    }
}

/// Every recorded click's screen px, for the cumulative final overlay.
fn read_click_points(dir: &str) -> Vec<(i32, i32)> {
    let mut out = Vec::new();
    if let Ok(s) = std::fs::read_to_string(format!("{dir}/clicks.jsonl")) {
        for line in s.lines() {
            if let Ok(v) = serde_json::from_str::<Value>(line)
                && let Some(p) = v.get("screen_px").and_then(|x| x.as_array())
                && p.len() == 2
            {
                out.push((
                    p[0].as_i64().unwrap_or(0) as i32,
                    p[1].as_i64().unwrap_or(0) as i32,
                ));
            }
        }
    }
    out
}

/// After the task ends, do a final high-res vision reading of the result and
/// save a frame with EVERY click point marked — so we can tell whether a wrong
/// outcome was a harness mis-click or a model decision.
fn final_review(dir: &str, target: Option<&str>, note: &str) {
    let view = window_view(target);
    let reading = read_view(
        view,
        "Describe the final on-screen state in detail. If this is a game, state the exact result \
(win / lose / draw) and the full final board.",
        "",
    )
    .unwrap_or_else(|e| format!("(vision read failed: {e})"));
    let _ = std::fs::write(
        format!("{dir}/final.txt"),
        format!("NOTE: {note}\n\nFINAL VISION READING:\n{reading}\n"),
    );
    eprintln!("[uia-task] FINAL REVIEW ({note}):\n{reading}");

    if let Ok(cap) = session::capture_virtual()
        && let Ok((jpeg, clamped)) = session::encode_view(&cap, view, VISION_SHORT, None, None)
        && let Ok(img) = image::load_from_memory(&jpeg)
    {
        let mut rgb = img.to_rgb8();
        for (sx, sy) in read_click_points(dir) {
            let fx = ((sx - clamped.x) as f64 / clamped.w.max(1) as f64 * rgb.width() as f64).round() as i32;
            let fy = ((sy - clamped.y) as f64 / clamped.h.max(1) as f64 * rgb.height() as f64).round() as i32;
            super::grid::draw_click_marker(&mut rgb, fx, fy);
        }
        let mut buf = std::io::Cursor::new(Vec::new());
        if image::DynamicImage::ImageRgb8(rgb)
            .write_to(&mut buf, image::ImageFormat::Jpeg)
            .is_ok()
        {
            let _ = std::fs::write(format!("{dir}/final-clicks.jpg"), buf.into_inner());
        }
    }
}

/// CLI: read the foreground window with the aux vision stack and print the
/// answer (validates chain resolution / keys / provider dispatch). `--cc-vision-test`.
pub fn run_vision_test(target: Option<&str>, question: &str) -> Result<()> {
    let view = window_view(target);
    eprintln!("[vision-test] reading view ({},{},{},{})", view.x, view.y, view.w, view.h);
    let answer = read_view(view, question, "")?;
    eprintln!("[vision-test] ANSWER:\n{answer}");
    if let Ok(desc) = std::env::var("CC_LOCATE")
        && !desc.trim().is_empty()
    {
        let loc = locate_in_view(view, &desc, "")?;
        let (sx, sy) = view.to_screen_px(loc.x, loc.y);
        eprintln!("[vision-test] LOCATE '{desc}' -> view_norm({:.0},{:.0}) screen_px({sx},{sy}) saw={:?}", loc.x, loc.y, loc.note);
    }
    Ok(())
}

/// CLI: capture one frame with the Set-of-Mark grid overlaid and save it, so we
/// can eyeball label legibility / tune `CC_GRID_COLS`/`CC_GRID_ROWS`. No model.
pub fn run_grid_test(target: Option<&str>) -> Result<()> {
    let grid = Grid::from_env();
    let view = window_view(target);
    let cap = session::capture_virtual()?;
    let (jpeg, shown) = session::encode_view(&cap, view, VIEW_SHORT, Some(grid), None)?;
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

/// The default view: the target/foreground window rect, else the whole desktop.
fn window_view(target: Option<&str>) -> View {
    if let Some((x, y, w, h)) = uia::target_window_rect(target) {
        View { x, y, w, h }
    } else {
        let (x, y, w, h) = uia::virtual_desktop();
        View { x, y, w, h }
    }
}

/// Capture + overlay + save the current view; return (base64 JPEG, clamped view)
/// WITHOUT sending. Callers send the frame AFTER answering any pending tool call:
/// pushing realtimeInput while a synchronous-FC tool is unanswered can trip an
/// intermittent INVALID_ARGUMENT abort.
fn render_view(
    dir: &str,
    step: usize,
    view: View,
    grid: Grid,
    marker: Option<(i32, i32)>,
) -> Result<(String, View)> {
    let cap = session::capture_virtual()?;
    let (jpeg, shown) = session::encode_view(&cap, view, VIEW_SHORT, Some(grid), marker)?;
    std::fs::write(format!("{dir}/step-{step:02}.jpg"), &jpeg).ok();
    eprintln!("[uia-task] step {step:02} frame {} KB", jpeg.len() / 1024);
    Ok((general_purpose::STANDARD.encode(&jpeg), shown))
}

/// Click at an absolute screen pixel (maps to 0-1000 over the virtual desktop,
/// which the executor turns into the SendInput absolute coordinate). `button` is
/// "left" or "right" (right is for context menus, e.g. "Save image as").
fn click_screen(
    sx: i32,
    sy: i32,
    dry: bool,
    button: &str,
    profile: &HumanProfile,
    cancel: &AtomicBool,
) -> Value {
    let (vx, vy, vw, vh) = uia::virtual_desktop();
    let nx = (sx - vx) as f64 / vw.max(1) as f64 * 1000.0;
    let ny = (sy - vy) as f64 / vh.max(1) as f64 * 1000.0;
    if dry {
        return json!({"ok": true, "note": "dry", "screen_px": [sx, sy], "button": button});
    }
    // Grid/vision-located clicks are "uncertain" → humanized executor hesitates
    // on the target so the user can barge in before it commits.
    executor::execute_ex(
        "click",
        &json!({"x": nx, "y": ny, "button": button, "uncertain": true}),
        profile,
        cancel,
    )
}

/// A new view magnified to the labeled grid cell (plus a little context), in
/// screen pixels. Returns None if the label is out of range.
fn zoom_to_cell(view: View, grid: &Grid, label: u32) -> Option<View> {
    let (fx0, fy0, fx1, fy1) = grid.frac_rect(label, 0.25)?;
    let x0 = view.x + (fx0 * view.w as f64).round() as i32;
    let y0 = view.y + (fy0 * view.h as f64).round() as i32;
    let x1 = view.x + (fx1 * view.w as f64).round() as i32;
    let y1 = view.y + (fy1 * view.h as f64).round() as i32;
    Some(View {
        x: x0,
        y: y0,
        w: (x1 - x0).max(8),
        h: (y1 - y0).max(8),
    })
}

/// Control types we treat as clickable targets.
fn is_clickable(ct: &str) -> bool {
    matches!(
        ct,
        "Button"
            | "MenuItem"
            | "TabItem"
            | "ListItem"
            | "CheckBox"
            | "RadioButton"
            | "Edit"
            | "ComboBox"
            | "Hyperlink"
            | "SplitButton"
            | "TreeItem"
            | "Slider"
            | "Tab"
    )
}

/// Inline summary of the read-only text elements (the live "values"), for logging.
fn readouts_inline(elements: &[UiElement]) -> String {
    elements
        .iter()
        .filter(|e| e.control_type == "Text" && !e.name.trim().is_empty())
        .map(|e| e.name.as_str())
        .collect::<Vec<_>>()
        .join(" | ")
}

/// Order-independent signature of the accessible UI (readout + clickable names),
/// for detecting whether an action changed anything (#2 state-delta).
fn state_signature(elements: &[UiElement]) -> String {
    let mut names: Vec<&str> = elements
        .iter()
        .filter(|e| !e.name.trim().is_empty() && (e.control_type == "Text" || is_clickable(e.control_type)))
        .map(|e| e.name.as_str())
        .collect();
    names.sort_unstable();
    names.dedup();
    names.join("|")
}

/// A truncated action signature for the stuck-loop detector (#1).
fn compact_args(args: &Value) -> String {
    args.to_string().chars().take(60).collect()
}

/// The structured state the model sees each turn: window title, live READOUTS
/// (Text values), and CLICKABLE elements by exact name. Each element is tagged
/// with the grid cell its center falls in (when inside the current view), giving
/// the model ground-truth spatial anchors in the SAME coordinate system it
/// clicks with — so it can locate canvas/board targets (which have no UIA
/// element) by reasoning relative to the named anchors instead of guessing.
fn format_state(elements: &[UiElement], target: Option<&str>, view: View, grid: Grid) -> String {
    let title = elements
        .iter()
        .find(|e| e.control_type == "Window" && !e.name.trim().is_empty())
        .map(|e| e.name.clone())
        .or_else(|| target.map(str::to_string))
        .unwrap_or_else(|| "(unknown)".to_string());

    let cell_of = |e: &UiElement| -> String {
        let (cx, cy) = e.center();
        let mx = (cx - view.x) as f64 / view.w.max(1) as f64 * 1000.0;
        let my = (cy - view.y) as f64 / view.h.max(1) as f64 * 1000.0;
        if (0.0..=1000.0).contains(&mx) && (0.0..=1000.0).contains(&my) {
            format!(" @cell{}", grid.cell_at(mx, my))
        } else {
            " @off-view".to_string()
        }
    };

    // Dedup identical entries and cap total size: some windows expose enormous,
    // heavily-repeated UIA trees, and an oversized turn payload aborts the Live
    // session. Keep the state compact and unique.
    let mut readouts = String::new();
    let mut clickable = String::new();
    let mut seen = std::collections::HashSet::new();
    for e in elements {
        let name = e.name.trim();
        if name.is_empty() {
            continue;
        }
        if readouts.len() + clickable.len() > 3200 {
            break;
        }
        if e.control_type == "Text" {
            let line = format!("- {name}{}\n", cell_of(e));
            if seen.insert(line.clone()) {
                readouts.push_str(&line);
            }
        } else if is_clickable(e.control_type) {
            let flag = if e.enabled { "" } else { " [disabled]" };
            let line = format!("- {} \"{name}\"{flag}{}\n", e.control_type, cell_of(e));
            if seen.insert(line.clone()) {
                clickable.push_str(&line);
            }
        }
    }
    if readouts.is_empty() {
        readouts.push_str("(none)\n");
    }
    format!(
        "WINDOW: {title}\n\nREADOUTS (live values, with grid cell):\n{readouts}\nCLICKABLE \
(click_element by exact name; @cellN is where it sits on the grid):\n{clickable}\nNote: targets with NO \
UIA element (game boards, canvas, images) are not listed - locate them visually, using the @cell anchors \
above as reference, then zoom that cell and click_at.\n"
    )
}

/// Resolve an element by exact name (case-insensitive) and click its true center.
/// Prefers an enabled, on-screen match; falls back to a unique substring match.
fn click_by_name(
    elements: &[UiElement],
    want: &str,
    dry: bool,
    profile: &HumanProfile,
    cancel: &AtomicBool,
) -> Value {
    let want_l = want.trim().to_lowercase();
    if want_l.is_empty() {
        return json!({"ok": false, "error": "missing name"});
    }
    let exact: Vec<&UiElement> = elements
        .iter()
        .filter(|e| e.name.to_lowercase() == want_l)
        .collect();
    let candidates = if !exact.is_empty() {
        exact
    } else {
        elements
            .iter()
            .filter(|e| e.name.to_lowercase().contains(&want_l))
            .collect()
    };
    let Some(e) = candidates.iter().find(|e| e.enabled).or_else(|| candidates.first()) else {
        return json!({"ok": false, "error": format!("no element named '{want}' on screen")});
    };
    if !e.enabled {
        return json!({"ok": false, "error": format!("element '{}' is disabled", e.name)});
    }
    let (cx, cy) = e.center();
    let (vx, vy, vw, vh) = uia::virtual_desktop();
    let nx = (cx - vx) as f64 / vw.max(1) as f64 * 1000.0;
    let ny = (cy - vy) as f64 / vh.max(1) as f64 * 1000.0;
    if dry {
        return json!({"ok": true, "note": "dry", "clicked": e.name, "norm": [nx.round(), ny.round()], "screen_px": [cx, cy]});
    }
    // Pass the element's true width so the humanized cursor's Fitts-law timing
    // and aim-jitter scale to the real target size.
    let target_w = (e.right - e.left).max(1) as f64;
    let r = executor::execute_ex(
        "click",
        &json!({"x": nx, "y": ny, "target_w": target_w}),
        profile,
        cancel,
    );
    json!({"ok": true, "clicked": e.name, "control_type": e.control_type, "result": r, "screen_px": [cx, cy]})
}

fn wait_for_setup(socket: &mut Sock) -> Result<()> {
    set_socket_short_timeout(socket)?;
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if Instant::now() > deadline {
            anyhow::bail!("timed out waiting for setupComplete");
        }
        let text = match socket.read() {
            Ok(Message::Text(t)) => t.to_string(),
            Ok(Message::Binary(b)) => match String::from_utf8(b.to_vec()) {
                Ok(s) => s,
                Err(_) => continue,
            },
            Ok(Message::Close(f)) => anyhow::bail!("server closed during setup: {f:?}"),
            Ok(_) => continue,
            Err(e) if is_transient_socket_read_error(&e) => continue,
            Err(e) => anyhow::bail!("setup read error: {e}"),
        };
        for ev in parse_server_message(&text) {
            if matches!(ev, ServerEvent::SetupComplete) {
                return Ok(());
            }
        }
    }
}
