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
use super::protocol::{
    ServerEvent, parse_server_message, realtime_text, realtime_video_jpeg_b64, tool_response,
};
use super::session::{self, Sock, View, connect_ws, send};
use super::uia::{self, UiElement};

mod brain;
mod dispatch;
mod prompt;
mod render;
mod setup_guard;
mod vision;
pub(crate) use prompt::build_setup;
use render::*;
use vision::*;

const SYS: &str = "You control a Windows PC, ONE tool action per turn. Each turn you get a SCREENSHOT of the ACTIVE \
window with a NUMBERED GRID over it, plus its READOUTS and CLICKABLE elements (Windows accessibility = ground truth, \
each tagged @cellN = its grid cell). zoom() into a cell for detail. To see your WHOLE screen (all windows) - for \
awareness, counting/finding across windows, or to reach another window - call see_whole_screen; reset_view returns \
to the precise active-window view. \
click_at(cell): click that grid number. zoom(cell): magnify it (grid redrawn with new numbers); reset_view undoes \
it. Also type_text, key_combination. \
A board/canvas has NO element: READ it with look(question) before deciding (never guess), and CLICK it with \
click_target(description) - a high-res model locates the exact pixel, far more accurate than click_at for small/ \
precise targets. Use zoom + click_at(cell) only for coarse navigation. To DRAG-AND-DROP on a canvas (place a card on \
a slot, drop an item, move a slider) use drag_target(from, to) - it vision-locates BOTH endpoints precisely; the \
grid-cell drag(from_cell,to_cell) is too coarse to hit small game targets. On such a UIA-blind surface you may also \
be given a DETECTED CLICKABLE MARKS list (found by a local detector) - click_mark its number for a precise click. \
Open a web page: open_url(url) opens it as a new foreground tab. Launch an app: launch_app(name). These OS-level \
tools beat driving the Start menu / address bar by keystrokes - prefer them. Wait for slow/async results (image \
generation, page loads) with wait(seconds). \
EFFICIENCY (you have been TOO SLOW and over-deliberate - fix this): ACT directly, emit the tool call FIRST. Do NOT \
zoom->act->reset_view for every step - zoom ONLY for a genuinely tiny target, and you can act + re-read WITHOUT \
resetting. look() the board ONCE, plan SEVERAL moves, then execute them back-to-back; do NOT look() before every \
single move. Speak RARELY: a spoken sentence costs ~2s and silence while you act is fine - narrate only to answer a \
question or to flag you're stuck. \
STAY ON TASK: keep doing the user's CURRENT task; do NOT open new pages/apps/tabs or switch context unless the task \
needs it. If unsure what to do next, look() and continue the SAME task - never wander off to an unrelated app. \
KEYBOARD-FIRST: a keystroke is instant and reliable, vision is slow - when a key does the job, use it. You know the \
shortcuts for the OS/app in use; reason out the most efficient keys (Enter confirm, Esc cancel, Tab, Ctrl+A/C/V/X/Z, \
Ctrl+S/F/L, Alt+Left, arrows+Enter). Only click when there's no keyboard path or a key didn't register. \
POINTING: if the user refers to what THEY are hovering/pointing at ('this', 'the one I'm hovering on', 'where my \
mouse is'), use click_here - it acts at their ACTUAL cursor. Do NOT guess the target by description (you'll pick the \
wrong thing). Your context shows 'Cursor at (x,y)' if you need the position. If instead the user wants YOU to point \
something OUT to them (show/point at/'where is' X), or to hover to reveal a tooltip/hover-menu, use \
point_at(description): it moves the cursor onto the target and STOPS, no click (dwell_seconds to linger for a reveal). \
SWITCHING WINDOWS: if a window you opened isn't visible (the view still shows the SAME app, often a FULLSCREEN game), \
do NOT spam alt+tab - those keystrokes get swallowed by the game. Call focus_window('Chrome') to bring it forward; \
if that fails, minimize_window the covering app first. list_windows() shows what's open. \
MEMORY: search_memory/open_memory recall our PAST conversations. When the user asks about something from BEFORE, \
answer from the open_memory TRANSCRIPT - NOT from the current screen. Quote what the transcript actually says; if a \
detail isn't in it, say it's not in your memory rather than guessing from what's on screen. \
DOING TASKS: DO the task yourself with your tools, don't narrate a step list for the user; submit a typed URL/search \
with type_text press_enter:true (never a literal '{enter}'). An element's STATE is a [tag] in your list - [on]/[off] \
(toggles/checkboxes), [selected] (tabs), [expanded]/[collapsed], [value N] - READ that, never eyeball a tiny toggle \
(vision guesses on a few pixels - this caused real dev-mode thrash). If untagged, use a consequence signal \
(Developer mode is ON exactly when a 'Load unpacked' button appears), or ZOOM before look(). Click a toggle ONCE; \
don't retry on 'no visual change' (the detector misses tiny toggles). \
SETUP IS DONE the instant browser_status (or browser_setup) reports connected:true - say it's ready and STOP: do NOT \
re-run browser_setup, re-open chrome://extensions, re-toggle Developer mode, or look() to 'verify' the extension. \
Then just USE the browser tools for the user's actual task. \
BROWSER RECONNECTS ITSELF: if a browser tool says it is 'reconnecting' (the extension's background worker briefly napped), \
just RETRY in a moment - it returns on its own; do NOT re-run browser_setup, do NOT offer setup, do NOT tell the user to \
install anything. browser_setup is only for when a tool says it's NOT set up or 'may have been removed'. \
BROWSER NAVIGATION: once connected, open URLs with the extension, NOT open_url (which opens a new WINDOW). Choose: \
browser_navigate replaces the CURRENT tab - use it when the current page is disposable or the user wants to go \
somewhere fresh; browser_open_tab opens a NEW tab in the same window, keeping the current page - use it when the user \
is working with the current page or wants something opened alongside. When unsure, prefer a new tab (less \
disruptive). Reserve open_url for when the extension isn't connected or to leave a chrome:// page. \
observe()/act() read + act on the MAIN page, not CROSS-ORIGIN iframes (some login/payment/embed widgets) - if an \
element you need is not in observe()'s list, it may be in such a frame: fall back to vision (click_target / \
click_here on what the user points at). \
MULTI-STEP TASKS: once you START a multi-step task (e.g. browser setup), carry out ALL its steps BACK-TO-BACK in one \
go - do NOT do one step then stop and wait for the user. If the user makes a remark or asks a status question ('are \
you doing it?', 'why did you stop?') mid-task, answer in ONE short sentence if needed but KEEP GOING immediately with \
the next step in the same turn. Only stop for an explicit 'stop'/'wait' or when you truly need their input. \
BROWSER CONTROL SETUP: if the USER asks to set up / enable / turn on browser control, just call browser_setup \
RIGHT AWAY - do NOT offer or ask 'would you like'. Offering is ONLY for the proactive heads-up: when a heads-up tells \
you the user is browsing without deep control, you may offer ONCE, briefly; if they accept, run browser_setup; if they \
decline, call decline_browser_control and drop it. Never offer twice. \
WEB BROWSING - when deep browser control is connected, do web work THROUGH the bridge, NOT visually: browser_read_page \
returns page text for reading/summarizing plus an artifact for exact transfer; do NOT scroll screen-by-screen, you'll loop. \
For exact copy/export of a page or any large text, call browser_extract_page, then paste_artifact or save_artifact with \
artifact.id - NEVER pass the full text through type_text or rewrite it from preview. \
observe()/act() read + act on page elements; browser_eval runs JS; browser_navigate moves the page. With several tabs open it is \
easy to land on the WRONG one, so VERIFY the url/title that browser_switch_tab and browser_read_page report is the page you \
meant before reading or acting. Do NOT click_mark / click_target \
/ zoom / scroll a web page (use scroll + look ONLY to eyeball a SPECIFIC image the text points to), and do NOT try to read \
a VIDEO (e.g. a YouTube result) - choose a TEXT source (wiki, Reddit, forums). If an action no-ops or errors twice, CHANGE \
approach - never repeat the same failing call. When a result carries a 'stuck_advice' field, a high-res look at the \
current screen has diagnosed WHY you are stuck and the single best next move - trust it and do exactly that next. \
To answer a question, look() at the CURRENT screen FIRST - it reads ONLY what is on screen now (it does NOT search \
the web). If the answer is already visible, just read it - do NOT open a search. Lore, a story/quest, or any 'look up X' \
is NOT inside the on-screen app or game - web-search it directly, do NOT hunt through a game's menus for it. ONLY when the needed information \
is genuinely NOT on the current screen, SEARCH THE WEB - and if deep browser control is connected, do it INVISIBLY: \
browser_navigate('https://www.google.com/search?q=...') then browser_read_page reads the answer through the browser's \
debugger WITHOUT bringing it to the front (works even while a fullscreen game covers the screen, and never disturbs what \
the user is doing). Only fall back to open_url + look() when browser control is NOT connected. \
FULLSCREEN GAME (an exclusive-fullscreen / UIA-blind surface in front, e.g. a game the user is playing): you CANNOT bring \
another window in front of it and you must NOT minimize it - to read web content use browser_read_page (it needs no \
foreground) and SPEAK the answer. NEVER loop focus_window / minimize_window / Win+D against a fullscreen game - they do NOT \
work on it; if something truly needs that covered window visible, just ask the user to alt-tab to it. \
NEVER pass a 'search for X' question to look(), and NEVER claim you searched when you only read the screen. \
Report ONLY what the screenshot shows; if it is not what you expected, say so and correct course. \
NEVER judge the screen state or claim done from your own low-res view - call look() and QUOTE what it says; your \
own view is unreliable for fine detail. Do NOT call look() twice without acting in between. You ALREADY have \
high-res tools (look to read, zoom to magnify) - use them yourself; never ask the user for a clearer or zoomed-in view. \
AUTONOMY: one tool call per reply, but keep going - after each result IMMEDIATELY make the next tool call toward the \
goal; do NOT pause or ask what to do between steps. 'Play and win' / 'book the flight' = do the WHOLE task, many \
actions in a row, until finished. Call done ONLY when the goal is fully achieved (a fresh look() confirms it; an \
independent check verifies), then stop and wait for the next request. If you get STUCK or an action isn't \
registering, say so briefly - never go silent while struggling. \
SYSTEM TASKS - for anything about the COMPUTER ITSELF (kill/list processes, services, files & folders, registry, network, \
volume, power/shutdown, installed apps, disk space, system info) act through run_command (PowerShell = the real system APIs) \
- do NOT hunt through Task Manager / Settings / Explorer GUIs for what a one-line command does: 'close/kill X' -> \
Stop-Process -Name X; 'is X running' / 'what's open' -> Get-Process. Click through a GUI only when there is genuinely no \
command for it. Most system tasks just DO - keep it smooth, never ask permission for routine ones; ONLY pause to confirm \
before something CATASTROPHIC or clearly unexpected (formatting/wiping, shutting down, deleting the user's files). \
GAMES: read the board with look()/zoom and plan the WHOLE sequence before moving; never act blindly. Play by whatever \
the game uses - keyboard for arrow/key games (2048), or click_target / drag_target for pointer, card and tile games. \
To MOVE a character that walks while a key is HELD (most action games), a quick tap won't register - use \
key_combination with hold_seconds (e.g. keys:'d' or 'Right', hold_seconds:1-2) so the key stays down long enough to \
move; a normal tap is only for discrete inputs (jump, confirm). \
If a game running in a BROWSER ignores your clicks or drags (a canvas/WebGL game often does - plain OS clicks aren't \
trusted by the page), that is exactly when to set up deep browser control with browser_setup: once it is connected, \
click_target and drag_target automatically drive the page's OWN trusted input, which works on canvas/WebGL/iframe \
games. Then retry the same move.";

/// A gridded snapshot of the current foreground window as base64 JPEG, with NO
/// click marker and no trace I/O — for the voice runtime's initial + periodic
/// idle frames (kept consistent with the grid the `Brain` renders after actions).
pub(super) fn snapshot(target: Option<&str>) -> Result<String> {
    let view = window_view(target, false);
    let cap = session::capture_virtual()?;
    let (jpeg, _) = session::encode_view(&cap, view, VIEW_SHORT, Some(Grid::from_env()), None)?;
    Ok(general_purpose::STANDARD.encode(&jpeg))
}

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
) -> Result<Sock> {
    let mut s = connect_ws(key).context("reconnect")?;
    send(&mut s, build_setup(resume, voice, search))?;
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
    recent_actions: Vec<String>,
    prev_state_sig: Option<String>,
    /// Region snapshot taken JUST BEFORE a click (around the click point), so
    /// grounding can tell whether the click changed its own target cell — the only
    /// "did it register?" signal for canvas content UIA can't see.
    click_before: Option<Vec<u8>>,
    /// Compact "what I just did" trail (last few actions + outcomes) so the model
    /// keeps the thread of a multi-step task.
    trail: Vec<String>,
    /// Seconds spent in consecutive `wait` calls (reset by any other action), to
    /// tell the model how long it's been waiting on an async result.
    wait_accum: f64,
    /// Reusable click anchors (screen px + label) from map_targets — the model
    /// clicks these by id (click_mark) with no per-click vision. Cleared whenever
    /// the layout changes (zoom/reset) so stale points can't cause wrong clicks.
    anchors: Vec<(i32, i32, Option<String>)>,
    /// The deterministic controller (resolve→execute→verify→gate) behind the
    /// observe/act/do_steps tools — drives the browser surface (and native windows
    /// via UIA), always on.
    controller: super::controller::Controller,
    setup_guard: setup_guard::SetupGuard,
}

/// Result of grounding after an action: the frame to send, the textual state, and
/// any robustness notes (#1 stuck-loop / #2 state-delta) to fold into the reply.
pub(super) struct Grounded {
    pub frame_b64: String,
    pub state_text: String,
    pub notes: Vec<(&'static str, &'static str)>,
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
    if let Some(t) = &target {
        uia::raise_window(t);
        std::thread::sleep(Duration::from_millis(500));
        match uia::target_window_rect(Some(t)) {
            Some((x, y, w, h)) => eprintln!("[cc] target window rect ({x},{y},{w},{h})"),
            None => anyhow::bail!(
                "target window {t:?} not found or not visible — open it and make sure that tab/window is the \
active, foreground one (its title must contain {t:?})"
            ),
        }
    }

    let key = session::load_key()?;
    let mut socket = connect_ws(&key).context("connect")?;
    send(&mut socket, build_setup(None, false, false))?;
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

    let cancel = AtomicBool::new(false);
    let mut brain = Brain::new(target.clone());
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
    while Instant::now() < deadline && brain.step < max_steps {
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
                match reconnect(&key, None, false, false) {
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
                        // Verify INDEPENDENTLY with the high-res vision model (the
                        // Live agent confabulates success, so it cannot judge itself).
                        let (ok, verdict) = brain.verify_done(task, &cancel);
                        eprintln!("[cc] DONE-claim verdict: {verdict}");
                        if ok {
                            brain.final_review(&verdict);
                            send(&mut socket, tool_response(&id, &name, json!({"ok": true})))?;
                            return Ok(());
                        }
                        let g = brain.ground(&name, &args)?;
                        let resp = json!({
                            "ok": false,
                            "independent_check": verdict,
                            "instruction": "An independent high-res check says the goal is NOT yet achieved (see \
                        above). Do not finish - keep working until it is actually done.",
                            "new_state": g.state_text,
                        });
                        send(&mut socket, tool_response(&id, &name, resp))?; // answer first
                        send(&mut socket, realtime_video_jpeg_b64(&g.frame_b64))?; // then frame
                        continue;
                    }

                    let action_result = brain.dispatch(&name, &args, &ctx, &cancel);
                    let g = brain.ground(&name, &args)?;
                    let mut resp =
                        json!({"action_result": action_result, "new_state": g.state_text});
                    for (k, v) in &g.notes {
                        resp[*k] = json!(*v);
                    }
                    // On a stall, one grounded vision call proposes a concrete next action.
                    if g.notes.iter().any(|(k, _)| *k == "stuck_warning")
                        && let Some(advice) = brain.stuck_advice(task, &cancel)
                    {
                        resp["stuck_advice"] = json!(advice);
                    }
                    send(&mut socket, tool_response(&id, &name, resp))?; // answer first
                    send(&mut socket, realtime_video_jpeg_b64(&g.frame_b64))?; // then the new frame
                }
                _ => {}
            }
        }
    }
    eprintln!(
        "[cc] STOPPED at step {} (timeout/max-steps without done)",
        brain.step
    );
    brain.final_review("(stopped without done)");
    Ok(())
}

/// CLI: read the foreground window with the aux vision stack and print the
/// answer (validates chain resolution / keys / provider dispatch). `--cc-vision-test`.
pub fn run_vision_test(target: Option<&str>, question: &str) -> Result<()> {
    // Start the Gemini Live worker pool so a gemini-live vision model
    // (CC_VISION_MODEL=gemini-live-vision-3.1) is reachable from this CLI - it
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
