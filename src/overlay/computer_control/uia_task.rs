//! UIA-grounded task harness (the workhorse). Each turn the model gets a
//! screenshot + a NUMBERED LIST of the REAL on-screen elements (Windows
//! accessibility = ground truth). It clicks BY INDEX; we click the element's
//! true coordinate (zero VLM localization error). After each action we re-read
//! UIA so the model verifies from ground truth, not pixels. Saves per-step
//! screenshots. `--cc-uia-task --cc-task "..."`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
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
KEYBOARD-FIRST: a keystroke is INSTANT and reliable; locating a button with vision is slow and can misfire - so when \
a key does the job, use it instead of clicking. You ALREADY KNOW the shortcuts for whatever you're using (the OS, \
dialogs, browsers, editors, specific apps) - REASON OUT the most efficient keys for THIS app and moment; don't limit \
yourself to a fixed list. Common ones, as illustration only: Enter to confirm a dialog/form (type_text press_enter), \
Escape to cancel/close, Tab between fields, Ctrl+A/C/V/X/Z, Ctrl+S save, Ctrl+F find, Ctrl+L address bar, Alt+Left \
back, arrows+Enter for menus/lists. Only click a button when there's genuinely no keyboard path, or a key didn't register. \
POINTING: if the user refers to what THEY are hovering/pointing at ('this', 'the one I'm hovering on', 'where my \
mouse is'), use click_here - it acts at their ACTUAL cursor. Do NOT guess the target by description (you'll pick the \
wrong thing). Your context shows 'Cursor at (x,y)' if you need the position. \
SWITCHING WINDOWS: if a window you opened isn't visible (the view still shows the SAME app, often a FULLSCREEN game), \
do NOT spam alt+tab - those keystrokes get swallowed by the game. Call focus_window('Chrome') to bring it forward; \
if that fails, minimize_window the covering app first. list_windows() shows what's open. \
MEMORY: search_memory/open_memory recall our PAST conversations. When the user asks about something from BEFORE, \
answer from the open_memory TRANSCRIPT - NOT from the current screen. Quote what the transcript actually says; if a \
detail isn't in it, say it's not in your memory rather than guessing from what's on screen. \
DOING TASKS FOR THE USER: when the user asks you to set something up or perform a task, DO it yourself with your \
tools - don't narrate a list of steps for them to do. To submit a typed URL/search, use type_text with \
press_enter:true (never type a literal '{enter}'). For DEEP browser control, call browser_setup and then carry out \
its checklist yourself, pausing only at the extension permission prompt. Before flipping a setting (e.g. the \
Developer mode switch), look() to check whether it's ALREADY in the wanted state - don't toggle what's already on. \
After clicking a small toggle/switch, VERIFY the new state with look() - the screen-change detector misses tiny \
toggles, so do NOT retry just because it reports 'no visual change'; click ONCE, then look() to confirm it flipped. \
SETUP IS DONE the instant browser_status (or browser_setup) reports connected:true - say it's ready and STOP: do NOT \
re-run browser_setup, re-open chrome://extensions, re-toggle Developer mode, or look() to 'verify' the extension. \
Then just USE the browser tools for the user's actual task. \
BROWSER NAVIGATION: once connected, open URLs with the extension, NOT open_url (which opens a new WINDOW). Choose: \
browser_navigate replaces the CURRENT tab - use it when the current page is disposable or the user wants to go \
somewhere fresh; browser_open_tab opens a NEW tab in the same window, keeping the current page - use it when the user \
is working with the current page or wants something opened alongside. When unsure, prefer a new tab (less \
disruptive). Reserve open_url for when the extension isn't connected or to leave a chrome:// page. \
MULTI-STEP TASKS: once you START a multi-step task (e.g. browser setup), carry out ALL its steps BACK-TO-BACK in one \
go - do NOT do one step then stop and wait for the user. If the user makes a remark or asks a status question ('are \
you doing it?', 'why did you stop?') mid-task, answer in ONE short sentence if needed but KEEP GOING immediately with \
the next step in the same turn. Only stop for an explicit 'stop'/'wait' or when you truly need their input. \
BROWSER CONTROL SETUP: if the USER asks to set up / enable / turn on browser control, just call browser_setup \
RIGHT AWAY - do NOT offer or ask 'would you like'. Offering is ONLY for the proactive heads-up: when a heads-up tells \
you the user is browsing without deep control, you may offer ONCE, briefly; if they accept, run browser_setup; if they \
decline, call decline_browser_control and drop it. Never offer twice. \
To answer a question, look() at the CURRENT screen FIRST - it reads ONLY what is on screen now (it does NOT search \
the web). If the answer is already visible, just read it - do NOT open a search. ONLY when the needed information \
is genuinely NOT on the current screen, open_url('https://www.google.com/search?q=...') and read the results. \
NEVER pass a 'search for X' question to look(), and NEVER claim you searched when you only read the screen. \
Report ONLY what the screenshot shows; if it is not what you expected, say so and correct course. \
NEVER judge the screen state or claim done from your own low-res view - call look() and QUOTE what it says; your \
own view is unreliable for fine detail. Do NOT call look() twice without acting in between. You ALREADY have \
high-res tools (look to read, zoom to magnify) - use them yourself; never ask the user for a clearer or zoomed-in view. \
One tool call per reply, but keep going AUTONOMOUSLY: after each tool result, IMMEDIATELY make the next tool call \
toward the goal - do NOT pause, wait, or ask the user what to do between steps. A single request like 'play and \
win' or 'book the flight' means do the WHOLE task yourself, many actions in a row, until it is finished. Only call \
done when the goal is fully achieved (a fresh look() confirms it; an independent check will verify) - and only \
THEN stop and wait for the user's next request, without acting further on your own. Don't narrate every routine \
step, but DO speak up briefly when it matters: answer the user's questions, and proactively tell them when you \
are stuck, when an action isn't registering, or what you're seeing - never go silent while struggling. Before a \
SLOW step (a look or a page load can take 10-20s), say a quick word ('one sec, reading that') so the user knows \
you're working, not frozen.";

pub(super) fn build_setup(resume: Option<&str>, voice: bool, search: bool) -> Value {
    let think = std::env::var("CC_THINK").unwrap_or_else(|_| "medium".to_string());
    // Raise the per-turn output cap so a long spoken summary isn't cut off mid-word.
    // maxOutputTokens IS honored by the Live API (it's not in the documented
    // unsupported list); the server clamps anything above the model's own ceiling.
    let max_out: u32 = std::env::var("CC_MAX_OUTPUT_TOKENS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(16384);
    // Match the global TTS voice preference ("Cài đặt giọng đọc" in settings) so
    // the agent speaks in the user's chosen Gemini voice, not a hardcoded one.
    let voice_name = {
        let v = crate::load_config().tts_voice.trim().to_string();
        if v.is_empty() { "Aoede".to_string() } else { v }
    };
    // On a reconnect, resume the prior session by its handle so the server
    // restores the full conversation (survives an intermittent server drop).
    let resumption = match resume {
        Some(h) => json!({ "handle": h }),
        None => json!({}),
    };
    let mut setup = json!({ "setup": {
        "model": format!("models/{}", super::protocol::MODEL),
        "generationConfig": {
            "responseModalities": ["AUDIO"],
            "speechConfig": {"voiceConfig": {"prebuiltVoiceConfig": {"voiceName": voice_name}}},
            "mediaResolution": "MEDIA_RESOLUTION_HIGH",
            "maxOutputTokens": max_out,
            "thinkingConfig": {"thinkingLevel": think}
        },
        "systemInstruction": {"parts": [{"text": SYS}]},
        "tools": [{"googleSearch": {}}, {"functionDeclarations": [
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
            {"name": "map_targets", "description": "Build a reusable set of click ANCHORS in ONE vision call: a high-res model finds EVERY target matching your description (e.g. 'every cell of the game board', 'each toolbar button') and returns them as numbered anchors. Then click any anchor by number with click_mark - NO per-click vision, so it is far faster for repetitive clicking on a board/grid/menu. Re-run map_targets if the layout changes (resize/scroll/new screen); it is auto-cleared when you zoom.",
             "parameters": {"type": "object", "properties": {"description": {"type": "string", "description": "What set of targets to map, e.g. 'every empty cell of the board'."}}, "required": ["description"]}},
            {"name": "click_mark", "description": "Click a numbered anchor previously built by map_targets (exact pixel, no vision cost). Set button='right' for a context menu.",
             "parameters": {"type": "object", "properties": {"mark": {"type": "integer", "description": "The anchor number from map_targets."}, "button": {"type": "string", "enum": ["left", "right"]}}, "required": ["mark"]}},
            {"name": "wait", "description": "Pause for N seconds, for slow or asynchronous operations (e.g. waiting for an image to finish generating or a page to load). Then re-observe.",
             "parameters": {"type": "object", "properties": {"seconds": {"type": "number", "description": "Seconds to wait (max 30)."}}, "required": ["seconds"]}},
            {"name": "type_text", "description": "Type text at the current keyboard focus. Set press_enter=true to submit afterward (e.g. an address bar or search box) - do NOT put '{enter}' inside the text, it would be typed literally.",
             "parameters": {"type": "object", "properties": {"text": {"type": "string"}, "press_enter": {"type": "boolean", "description": "Press Enter after typing (to submit)."}}, "required": ["text"]}},
            {"name": "scroll", "description": "Scroll with the REAL mouse wheel (not PageDown) over the page/list. direction up/down (or left/right); 'amount' is how far (default 5; larger scrolls more). Optionally pass a grid 'cell' to scroll over a specific area, else it scrolls over the centre. Prefer this for natural scrolling.",
             "parameters": {"type": "object", "properties": {"direction": {"type": "string", "enum": ["up", "down", "left", "right"]}, "amount": {"type": "number"}, "cell": {"type": "integer"}}, "required": ["direction"]}},
            {"name": "drag", "description": "Press at one grid cell, glide to another, and release - for sliders, reordering items, drawing, or click-drag to SELECT text/items. Pass from_cell and to_cell (the printed grid numbers). zoom() first for finer cells when precision matters.",
             "parameters": {"type": "object", "properties": {"from_cell": {"type": "integer", "description": "Grid cell to press at."}, "to_cell": {"type": "integer", "description": "Grid cell to release at."}}, "required": ["from_cell", "to_cell"]}},
            {"name": "click_here", "description": "Click EXACTLY where the mouse cursor currently is, without moving it (button='right' for a context menu). Use when the user refers to what THEY are pointing at - 'this', 'the one I'm hovering on', 'where my mouse is' - because their pointer is already on the target. Far more reliable than guessing the target by description with click_target.",
             "parameters": {"type": "object", "properties": {"button": {"type": "string", "enum": ["left", "right", "middle"]}}}},
            {"name": "key_combination", "description": "Press a keyboard shortcut, e.g. Enter, Control+C, Alt+Tab.",
             "parameters": {"type": "object", "properties": {"keys": {"type": "string"}}, "required": ["keys"]}},
            {"name": "open_url", "description": "Open an http(s) URL in the default browser as a NEW foreground tab (via the OS shell). Use this to go to a web page directly - far more reliable than typing into the address bar.",
             "parameters": {"type": "object", "properties": {"url": {"type": "string"}}, "required": ["url"]}},
            {"name": "launch_app", "description": "Launch or focus a Windows app by name/path via the OS shell, e.g. 'chrome', 'notepad', 'explorer'. Pass 'args' to give it arguments - e.g. open a file in an app: name='notepad', args='C:\\\\path\\\\file.txt' (or just launch_app the file path itself to open it in its default app). Do NOT cram args into 'name'.",
             "parameters": {"type": "object", "properties": {"name": {"type": "string"}, "args": {"type": "string", "description": "Optional command-line arguments / file to open."}}, "required": ["name"]}},
            {"name": "run_command", "description": "Run a Windows PowerShell command and get its text output - your most GENERAL tool, for anything without a dedicated action: file operations (Get-ChildItem, Get-Content, Set-Content, Copy-Item, New-Item), processes (Get-Process), system info, audio volume, etc. Runs non-elevated and non-interactive (commands that prompt will fail rather than hang). Returns stdout/stderr (truncated). Prefer a real tool when one exists (e.g. open_url, type_text).",
             "parameters": {"type": "object", "properties": {"command": {"type": "string", "description": "The PowerShell command line to run."}}, "required": ["command"]}},
            {"name": "focus_window", "description": "Bring an already-open window to the FRONT by a piece of its title (e.g. 'Chrome', 'Notepad', a document name). Use this when a window you opened isn't visible because another window (often a FULLSCREEN game) is covering it - alt+tab keystrokes go to the game instead, so use this to switch reliably. Returns the window now in front so you can confirm. If it reports the same covering window, that app is likely exclusive-fullscreen: minimize_window it first.",
             "parameters": {"type": "object", "properties": {"title": {"type": "string", "description": "A substring of the target window's title bar."}}, "required": ["title"]}},
            {"name": "list_windows", "description": "List the titles of all open top-level windows, so you know what's available to focus_window or minimize_window. No arguments.",
             "parameters": {"type": "object", "properties": {}}},
            {"name": "minimize_window", "description": "Minimize a window by a piece of its title - use this to get a FULLSCREEN game or app OUT OF THE WAY when it covers what you need (it works even when the game swallows alt+tab/Win+D keystrokes, because it acts on the window directly). Returns what's in front afterward.",
             "parameters": {"type": "object", "properties": {"title": {"type": "string", "description": "A substring of the window to minimize."}}, "required": ["title"]}},
            {"name": "resize_window", "description": "Resize a window (matched by a piece of its title) to width x height in PIXELS. Restores it first if maximized, so you can make it smaller. e.g. resize_window('Notepad', 700, 500).",
             "parameters": {"type": "object", "properties": {"title": {"type": "string"}, "width": {"type": "integer"}, "height": {"type": "integer"}}, "required": ["title", "width", "height"]}},
            {"name": "move_window", "description": "Move a window (matched by a piece of its title) so its top-left corner is at screen pixel (x, y). Keeps its current size.",
             "parameters": {"type": "object", "properties": {"title": {"type": "string"}, "x": {"type": "integer"}, "y": {"type": "integer"}}, "required": ["title", "x", "y"]}},
            {"name": "read_clipboard", "description": "Read the text currently on the Windows clipboard (e.g. what you or the user just copied). Lets you grab a selection without retyping it. No arguments. (type_text already PASTES via the clipboard, so writing long text is fast.)",
             "parameters": {"type": "object", "properties": {}}},
            {"name": "done", "description": "Call ONLY when the goal is confirmed achieved; quote the evidence.",
             "parameters": {"type": "object", "properties": {"summary": {"type": "string"}}, "required": ["summary"]}},
            {"name": "search_memory", "description": "Search YOUR memory of PAST conversations (every prior session is saved). Use when the user refers to something from before ('remember when we...', 'what did we decide about X', 'last time'). Returns matching past conversations as numbered results with a title + snippet + id. Then call open_memory(id) to read the full one.",
             "parameters": {"type": "object", "properties": {"query": {"type": "string", "description": "What to recall, in plain words, e.g. 'the plan for the memory feature' or 'the Genshin quest story'."}}, "required": ["query"]}},
            {"name": "open_memory", "description": "Read the FULL transcript of one past conversation returned by search_memory. Pass its id.",
             "parameters": {"type": "object", "properties": {"id": {"type": "string", "description": "The conversation id from a search_memory result."}}, "required": ["id"]}},
            {"name": "browser_setup", "description": "Bring up DEEP browser control (read/act on the real page DOM, not just pixels) via the SGT browser extension. It opens chrome://extensions and returns the extension folder + pairing code + a 'do_yourself' checklist. DO the install YOURSELF with your tools (toggle Developer mode, Load unpacked the folder, paste the pairing code in the popup) - do NOT recite steps to the user. Pause ONLY at the permission prompt. Then poll browser_status.",
             "parameters": {"type": "object", "properties": {}}},
            {"name": "browser_status", "description": "Check whether the deep-browser extension is connected. Returns connected + the pairing code/port.",
             "parameters": {"type": "object", "properties": {}}},
            {"name": "browser_reset", "description": "Reset/repair browser control when it's stuck or won't connect (e.g. the user says 'reset/fix/forget browser control'): re-opens the pairing window so a loaded extension re-pairs cleanly and re-enables the setup offer. To fully UNINSTALL, the user removes the extension on the browser's extensions page.",
             "parameters": {"type": "object", "properties": {}}},
            {"name": "browser_read_page", "description": "Read the current page's real DOM: title, url, and visible text. Far more complete/reliable than look() for web pages once the extension is connected.",
             "parameters": {"type": "object", "properties": {}}},
            {"name": "browser_query", "description": "Find elements by CSS selector on the page; returns up to 50 with their text, tag, and on-screen rect. Use to locate things precisely before browser_click/browser_fill.",
             "parameters": {"type": "object", "properties": {"selector": {"type": "string", "description": "A CSS selector, e.g. 'button.submit' or 'a[href*=login]'."}}, "required": ["selector"]}},
            {"name": "browser_click", "description": "Click the element matching a CSS selector, using a TRUSTED page click (more reliable than pixel clicks). Scrolls it into view first.",
             "parameters": {"type": "object", "properties": {"selector": {"type": "string"}}, "required": ["selector"]}},
            {"name": "browser_fill", "description": "Focus the input/textarea matching a CSS selector, select its contents, and type text into it (trusted, fires input events).",
             "parameters": {"type": "object", "properties": {"selector": {"type": "string"}, "text": {"type": "string"}}, "required": ["selector", "text"]}},
            {"name": "browser_wait_for", "description": "Wait until an element matching a CSS selector appears (or timeout). Use after a click/navigation that loads content.",
             "parameters": {"type": "object", "properties": {"selector": {"type": "string"}, "timeout_ms": {"type": "integer"}}, "required": ["selector"]}},
            {"name": "browser_eval", "description": "Run JavaScript in the page and return its (JSON-able) result. Your general escape hatch for extracting structured data or doing precise DOM work.",
             "parameters": {"type": "object", "properties": {"code": {"type": "string", "description": "A JS expression; its value is returned (use an IIFE for statements)."}}, "required": ["code"]}},
            {"name": "browser_navigate", "description": "Navigate the CURRENT tab to a URL (replaces what's on it). Use when the current page is disposable or the user wants to go somewhere fresh.",
             "parameters": {"type": "object", "properties": {"url": {"type": "string"}}, "required": ["url"]}},
            {"name": "browser_open_tab", "description": "Open a URL in a NEW tab in the same window (keeps the current page). Use when the user is working with the current page or wants something opened alongside. Prefer this over browser_navigate when unsure (less disruptive).",
             "parameters": {"type": "object", "properties": {"url": {"type": "string"}}, "required": ["url"]}},
            {"name": "browser_upload", "description": "Set the file for a file <input> matching a CSS selector (real upload via DevTools). Pass an absolute file path.",
             "parameters": {"type": "object", "properties": {"selector": {"type": "string"}, "path": {"type": "string"}}, "required": ["selector", "path"]}},
            {"name": "browser_tabs", "description": "List the open browser tabs (id, title, url, active).",
             "parameters": {"type": "object", "properties": {}}},
            {"name": "browser_switch_tab", "description": "Make a browser tab active by its id (from browser_tabs).",
             "parameters": {"type": "object", "properties": {"tab_id": {"type": "integer"}}, "required": ["tab_id"]}},
            {"name": "browser_network", "description": "Read recent network requests the page made (url + status). Enables capture if needed; call again after the page loads to see results.",
             "parameters": {"type": "object", "properties": {"filter": {"type": "string", "description": "Optional substring of the CDP event name, e.g. 'responseReceived'."}}}},
            {"name": "decline_browser_control", "description": "Call ONLY when the user declines your offer to set up deep browser control - records it so you stop asking this session and don't nag (you may bring it up again much later). No args.",
             "parameters": {"type": "object", "properties": {}}}
        ]}],
        "inputAudioTranscription": {},
        "outputAudioTranscription": {},
        "sessionResumption": resumption,
        "contextWindowCompression": {"slidingWindow": {}}
    }});
    // Voice sessions need VAD + barge-in so a spoken command (or "stop") can
    // interrupt; the headless harness omits it (no mic).
    if voice {
        setup["setup"]["realtimeInputConfig"] = json!({
            "automaticActivityDetection": {
                "startOfSpeechSensitivity": "START_SENSITIVITY_HIGH",
                "endOfSpeechSensitivity": "END_SENSITIVITY_HIGH",
                "prefixPaddingMs": 30,
                "silenceDurationMs": 250
            },
            // Native barge-in: when you START speaking, the server interrupts the
            // model - it stops talking (we clear the audio sink on `interrupted`) and
            // cancels any pending tool call (handled as ToolCancellation: the action
            // still physically finishes, its result is dropped, and the model re-plans
            // from your new words). The Live API couples speech + action interruption
            // into this one switch, so getting "stop talking" back means actions are
            // interruptible too. Requires headphones - on open speakers the agent's own
            // voice leaks into the mic and self-interrupts, so set CC_MIC_GATE=1 to mute
            // the mic during playback (which trades away barge-in to stop the echo).
            "activityHandling": "START_OF_ACTIVITY_INTERRUPTS"
        });
    }
    // Google Search grounding needs a billing-enabled project / grounding quota;
    // without it the server rejects the whole session ("exceeded quota"). So it's
    // OPT-IN per call — callers retry without it if setup fails.
    if !search && let Some(tools) = setup["setup"]["tools"].as_array_mut() {
        tools.retain(|t| t.get("googleSearch").is_none());
    }
    setup
}

/// A gridded snapshot of the current foreground window as base64 JPEG, with NO
/// click marker and no trace I/O — for the voice runtime's initial + periodic
/// idle frames (kept consistent with the grid the `Brain` renders after actions).
pub(super) fn snapshot(target: Option<&str>) -> Result<String> {
    let view = window_view(target);
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
pub(super) fn reconnect(key: &str, resume: Option<&str>, voice: bool, search: bool) -> Result<Sock> {
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
}

/// Result of grounding after an action: the frame to send, the textual state, and
/// any robustness notes (#1 stuck-loop / #2 state-delta) to fold into the reply.
pub(super) struct Grounded {
    pub frame_b64: String,
    pub state_text: String,
    pub notes: Vec<(&'static str, &'static str)>,
}

impl Brain {
    pub fn new(target: Option<String>) -> Self {
        // Per-action frame + click traces (the key accuracy-refinement record).
        // Default to app-data so a released launch doesn't litter the cwd; a dev
        // run can override with CC_TRACE_DIR.
        let dir = std::env::var("CC_TRACE_DIR").unwrap_or_else(|_| {
            std::env::var("LOCALAPPDATA")
                .map(|p| format!("{p}/screen-goated-toolbox/cc-trace"))
                .unwrap_or_else(|_| "cc-trace".to_string())
        });
        std::fs::create_dir_all(&dir).ok();
        let view = window_view(target.as_deref());
        Self {
            dir,
            grid: Grid::from_env(),
            profile: HumanProfile::from_env(),
            dry: std::env::var("CC_DRY").is_ok(),
            target,
            view,
            zoomed: false,
            last_click: None,
            step: 0,
            recent_actions: Vec::new(),
            prev_state_sig: None,
            click_before: None,
            trail: Vec::new(),
            wait_accum: 0.0,
            anchors: Vec::new(),
        }
    }

    /// Per-turn grounding context the model gets above the element list: where it
    /// is (window), where the cursor is + what's under it, what it just did, and
    /// how long it's been waiting. Cheap situational awareness.
    fn context_block(&self) -> String {
        let (title, cx, cy) = uia::pointer_context();
        let title: String = if title.is_empty() { "(unknown)".into() } else { title.chars().take(70).collect() };
        let trail = if self.trail.is_empty() { "(none yet)".to_string() } else { self.trail.join("  |  ") };
        let mut s = format!(
            "Active window: {title}\nCursor at ({cx},{cy})\nYour recent actions: {trail}"
        );
        if self.wait_accum > 0.0 {
            s.push_str(&format!(
                "\nWaited {:.0}s so far on this - if nothing has changed, stop waiting and act.",
                self.wait_accum
            ));
        }
        s
    }

    /// Turn-0 grounding: (frame_b64, state_text). No click marker yet.
    pub fn initial(&mut self) -> Result<(String, String)> {
        let elements = uia::enumerate(self.target.as_deref()).unwrap_or_default();
        let (b, v, _fp) = render_view(&self.dir, self.step, self.view, self.grid, None)?;
        self.view = v;
        self.prev_state_sig = Some(state_signature(&elements));
        let state = format_state(&elements, self.target.as_deref(), self.view, self.grid);
        Ok((b, state))
    }

    /// Execute one tool call (NOT `done`). Returns the action result JSON; polls
    /// `cancel` (set on barge-in) between micro-steps via the humanized executor.
    pub fn dispatch(&mut self, name: &str, args: &Value, ctx: &str, cancel: &AtomicBool) -> Value {
        self.step += 1;
        let step = self.step;
        let t0 = Instant::now();
        let result = match name {
            "click_element" => {
                let elements = uia::enumerate(self.target.as_deref()).unwrap_or_default();
                let want = args.get("name").and_then(Value::as_str).unwrap_or("");
                let r = click_by_name(&elements, want, self.dry, &self.profile, cancel);
                if let Some(p) = r.get("screen_px").and_then(|v| v.as_array())
                    && p.len() == 2
                {
                    let (sx, sy) = (p[0].as_i64().unwrap_or(0) as i32, p[1].as_i64().unwrap_or(0) as i32);
                    self.last_click = Some((sx, sy));
                    append_click(&self.dir, json!({"step": step, "kind": "click_element", "name": want, "screen_px": [sx, sy]}));
                }
                r
            }
            "click_at" => {
                let cell = args.get("cell").and_then(Value::as_u64).unwrap_or(0) as u32;
                match self.grid.center_norm(cell) {
                    Some((mx, my)) => {
                        let (sx, sy) = self.view.to_screen_px(mx, my);
                        self.last_click = Some((sx, sy));
                        self.click_before = session::capture_region_fp(sx, sy, VC_HALF);
                        append_click(&self.dir, json!({"step": step, "kind": "click_at", "cell": cell,
                            "view_norm": [mx.round(), my.round()], "screen_px": [sx, sy],
                            "view": [self.view.x, self.view.y, self.view.w, self.view.h]}));
                        click_screen(sx, sy, self.dry, "left", &self.profile, cancel)
                    }
                    None => json!({"ok": false, "error": format!("cell {cell} out of range 1..={}", self.grid.cell_count())}),
                }
            }
            "zoom" => {
                let cell = args.get("cell").and_then(Value::as_u64).unwrap_or(0) as u32;
                match zoom_to_cell(self.view, &self.grid, cell) {
                    Some(v) => {
                        self.view = v;
                        self.zoomed = true;
                        self.anchors.clear(); // view changed -> old anchors are stale
                        json!({"ok": true, "zoomed_cell": cell})
                    }
                    None => json!({"ok": false, "error": format!("cell {cell} out of range 1..={}", self.grid.cell_count())}),
                }
            }
            "reset_view" => {
                self.zoomed = false;
                self.anchors.clear();
                json!({"ok": true, "view": "whole window"})
            }
            "look" => {
                let q = args.get("question").and_then(Value::as_str).unwrap_or("Describe exactly what is on screen.");
                match read_view(self.view, q, ctx, cancel) {
                    Ok(answer) => {
                        eprintln!("[cc] step {step:02} LOOK: {answer}");
                        json!({"ok": true, "reading": answer})
                    }
                    Err(e) => json!({"ok": false, "error": format!("vision read failed: {e}")}),
                }
            }
            "click_target" => {
                let desc = args.get("description").and_then(Value::as_str).unwrap_or("");
                let button = match args.get("button").and_then(Value::as_str) {
                    Some("right") => "right",
                    _ => "left",
                };
                match locate_in_view(self.view, desc, ctx, cancel) {
                    Ok(loc) => {
                        let (sx, sy) = self.view.to_screen_px(loc.x, loc.y);
                        self.last_click = Some((sx, sy));
                        self.click_before = session::capture_region_fp(sx, sy, VC_HALF);
                        append_click(&self.dir, json!({"step": step, "kind": "click_target", "desc": desc,
                            "button": button, "view_norm": [loc.x.round(), loc.y.round()],
                            "screen_px": [sx, sy], "saw": loc.note,
                            "view": [self.view.x, self.view.y, self.view.w, self.view.h]}));
                        eprintln!("[cc] step {step:02} CLICK_TARGET[{button}] '{desc}' -> screen({sx},{sy}) saw={:?}", loc.note);
                        let r = click_screen(sx, sy, self.dry, button, &self.profile, cancel);
                        json!({"ok": true, "located_view_norm": [loc.x, loc.y], "saw_at_target": loc.note, "click": r})
                    }
                    Err(e) => json!({"ok": false, "error": format!("could not locate '{desc}': {e}")}),
                }
            }
            "map_targets" => {
                let desc = args.get("description").and_then(Value::as_str).unwrap_or("");
                match map_in_view(self.view, desc, ctx, cancel) {
                    Ok(pts) => {
                        self.anchors = pts
                            .iter()
                            .map(|p| {
                                let (sx, sy) = self.view.to_screen_px(p.x, p.y);
                                (sx, sy, p.note.clone())
                            })
                            .collect();
                        let list: Vec<Value> = self
                            .anchors
                            .iter()
                            .enumerate()
                            .map(|(i, (_, _, note))| json!({"mark": i + 1, "what": note}))
                            .collect();
                        eprintln!("[cc] step {step:02} MAP_TARGETS '{desc}' -> {} anchors", self.anchors.len());
                        json!({"ok": true, "anchor_count": self.anchors.len(), "anchors": list,
                            "note": "Click any of these by its mark number with click_mark(mark). They stay valid until the layout changes - then re-run map_targets."})
                    }
                    Err(e) => json!({"ok": false, "error": format!("could not map '{desc}': {e}")}),
                }
            }
            "click_mark" => {
                let id = args.get("mark").and_then(Value::as_u64).unwrap_or(0) as usize;
                let button = match args.get("button").and_then(Value::as_str) {
                    Some("right") => "right",
                    _ => "left",
                };
                let anchor = self.anchors.get(id.wrapping_sub(1)).map(|(sx, sy, n)| (*sx, *sy, n.clone()));
                match anchor {
                    Some((sx, sy, note)) => {
                        self.last_click = Some((sx, sy));
                        self.click_before = session::capture_region_fp(sx, sy, VC_HALF);
                        append_click(&self.dir, json!({"step": step, "kind": "click_mark", "mark": id,
                            "button": button, "screen_px": [sx, sy], "saw": note}));
                        eprintln!("[cc] step {step:02} CLICK_MARK {id} -> screen({sx},{sy})");
                        let r = click_screen(sx, sy, self.dry, button, &self.profile, cancel);
                        json!({"ok": true, "clicked_mark": id, "what": note, "click": r})
                    }
                    None => json!({"ok": false, "error": format!("no anchor #{id} (have {}); run map_targets first", self.anchors.len())}),
                }
            }
            "wait" => {
                let secs = args.get("seconds").and_then(Value::as_f64).unwrap_or(3.0).clamp(0.0, 30.0);
                let aborted = human_input::sleep_cancellable((secs * 1000.0) as u64, cancel);
                json!({"ok": !aborted, "waited_seconds": secs})
            }
            "type_text" | "key_combination" | "open_url" | "launch_app" | "run_command" | "click_here" => {
                if self.dry {
                    json!({"ok": true, "note": "dry"})
                } else {
                    executor::execute_ex(name, args, &self.profile, cancel)
                }
            }
            "scroll" => {
                // Real mouse-wheel scroll. Resolve where to scroll: a given grid
                // cell, else the centre of the current view (the wheel acts on the
                // window under that point).
                let (mx, my) = args
                    .get("cell")
                    .and_then(Value::as_u64)
                    .and_then(|c| self.grid.center_norm(c as u32))
                    .unwrap_or((500.0, 500.0));
                let (sx, sy) = self.view.to_screen_px(mx, my);
                // executor scroll/drag take 0..1000 normalized, not screen px.
                let (nx, ny) = executor::screen_to_norm(sx, sy);
                let a = json!({
                    "x": nx, "y": ny,
                    "direction": args.get("direction").and_then(Value::as_str).unwrap_or("down"),
                    "magnitude": args.get("amount").and_then(Value::as_f64).unwrap_or(5.0),
                });
                if self.dry {
                    json!({"ok": true, "note": "dry"})
                } else {
                    executor::execute_ex("scroll", &a, &self.profile, cancel)
                }
            }
            "drag" => {
                // Press at from_cell, glide, release at to_cell — for sliders,
                // reordering, drawing, or click-drag selection. Zoom first for
                // finer cells when precision matters.
                let from = args
                    .get("from_cell")
                    .and_then(Value::as_u64)
                    .and_then(|c| self.grid.center_norm(c as u32));
                let to = args
                    .get("to_cell")
                    .and_then(Value::as_u64)
                    .and_then(|c| self.grid.center_norm(c as u32));
                match (from, to) {
                    (Some((fx, fy)), Some((tx, ty))) => {
                        // executor drag takes 0..1000 normalized, not screen px.
                        let (fpx, fpy) = self.view.to_screen_px(fx, fy);
                        let (tpx, tpy) = self.view.to_screen_px(tx, ty);
                        let (sx, sy) = executor::screen_to_norm(fpx, fpy);
                        let (dx, dy) = executor::screen_to_norm(tpx, tpy);
                        let a = json!({"x": sx, "y": sy, "dest_x": dx, "dest_y": dy});
                        if self.dry {
                            json!({"ok": true, "note": "dry"})
                        } else {
                            executor::execute_ex("drag", &a, &self.profile, cancel)
                        }
                    }
                    _ => json!({"ok": false, "error": "drag needs from_cell and to_cell"}),
                }
            }
            "focus_window" => {
                let title = args.get("title").and_then(Value::as_str).unwrap_or("");
                let raised = super::uia::raise_window(title);
                std::thread::sleep(Duration::from_millis(200)); // let the switch settle
                let now = super::uia::pointer_context().0;
                json!({
                    "ok": raised,
                    "foreground_now": now,
                    "note": if raised { "switched" } else { "could not bring it to front - it may be covered by an exclusive-fullscreen app; minimize_window that app first" }
                })
            }
            "list_windows" => {
                json!({"ok": true, "windows": super::uia::list_windows()})
            }
            "read_clipboard" => {
                json!({"ok": true, "text": super::clipboard::get_text()})
            }
            "minimize_window" => {
                let title = args.get("title").and_then(Value::as_str).unwrap_or("");
                let ok = super::uia::minimize_window(title);
                std::thread::sleep(Duration::from_millis(200)); // let the minimize settle
                json!({"ok": ok, "foreground_now": super::uia::pointer_context().0})
            }
            "resize_window" => {
                let title = args.get("title").and_then(Value::as_str).unwrap_or("");
                let w = args.get("width").and_then(Value::as_i64).unwrap_or(0) as i32;
                let h = args.get("height").and_then(Value::as_i64).unwrap_or(0) as i32;
                json!({"ok": super::uia::resize_window(title, w, h)})
            }
            "move_window" => {
                let title = args.get("title").and_then(Value::as_str).unwrap_or("");
                let x = args.get("x").and_then(Value::as_i64).unwrap_or(0) as i32;
                let y = args.get("y").and_then(Value::as_i64).unwrap_or(0) as i32;
                json!({"ok": super::uia::move_window(title, x, y)})
            }
            "search_memory" => {
                let query = args.get("query").and_then(Value::as_str).unwrap_or("");
                let hits = super::memory::search(query, 5);
                if hits.is_empty() {
                    json!({"ok": true, "results": [], "note": "no matching past conversation"})
                } else {
                    let results: Vec<Value> = hits
                        .iter()
                        .map(|h| {
                            json!({"id": h.id.to_string(), "when": h.timestamp, "title": h.title, "snippet": h.snippet})
                        })
                        .collect();
                    json!({"ok": true, "results": results, "instruction": "Results are ranked by relevance + recency; each has a 'when' timestamp. For 'the last/most recent/previous conversation', pick the one with the newest 'when'. Then open_memory(id) to read it in full."})
                }
            }
            "open_memory" => {
                let id = args.get("id").and_then(Value::as_str).unwrap_or("");
                match id.parse::<i64>().ok().and_then(super::memory::open) {
                    Some(transcript) => json!({"ok": true, "transcript": transcript}),
                    None => json!({"ok": false, "error": "no saved conversation with that id"}),
                }
            }
            "browser_setup" => super::browser::setup(),
            "browser_status" => super::browser::status(),
            "browser_reset" => super::browser::reset(),
            "browser_read_page" => super::browser::read_page(),
            "browser_query" => super::browser::query(args.get("selector").and_then(Value::as_str).unwrap_or("")),
            "browser_click" => super::browser::click_selector(args.get("selector").and_then(Value::as_str).unwrap_or("")),
            "browser_fill" => super::browser::fill(
                args.get("selector").and_then(Value::as_str).unwrap_or(""),
                args.get("text").and_then(Value::as_str).unwrap_or(""),
            ),
            "browser_wait_for" => super::browser::wait_for(
                args.get("selector").and_then(Value::as_str).unwrap_or(""),
                args.get("timeout_ms").and_then(Value::as_u64).unwrap_or(8000),
            ),
            "browser_eval" => super::browser::eval_js(args.get("code").and_then(Value::as_str).unwrap_or("")),
            "browser_navigate" => super::browser::navigate(args.get("url").and_then(Value::as_str).unwrap_or("")),
            "browser_open_tab" => super::browser::open_tab(args.get("url").and_then(Value::as_str).unwrap_or("")),
            "browser_upload" => super::browser::upload_file(
                args.get("selector").and_then(Value::as_str).unwrap_or(""),
                args.get("path").and_then(Value::as_str).unwrap_or(""),
            ),
            "browser_tabs" => super::browser::get_tabs(),
            "browser_switch_tab" => super::browser::switch_tab(args.get("tab_id").and_then(Value::as_i64).unwrap_or(0)),
            "browser_network" => super::browser::read_network(args.get("filter").and_then(Value::as_str).unwrap_or("")),
            "decline_browser_control" => {
                super::browser::record_decline();
                json!({"ok": true, "noted": "won't ask again for a while"})
            }
            _ => json!({"ok": false, "error": "unknown action"}),
        };
        // Per-action latency (excludes the settle wait) — the key refinement
        // signal for vision/click cost. Full result is truncated to avoid bloat;
        // look()/click_target log their rich detail on their own lines above.
        let ms = t0.elapsed().as_millis();
        let settle = if name == "open_url" || name == "launch_app" { 1100 } else { 250 };
        std::thread::sleep(Duration::from_millis(settle));
        let short: String = result.to_string().chars().take(120).collect();
        eprintln!("[cc] step {step:02} {name} {ms}ms -> {short}");
        // Record the action trail (for situational context) + consecutive wait time.
        let ok = result.get("ok").and_then(Value::as_bool).unwrap_or(true);
        self.trail.push(format!("{name}={}", if ok { "ok" } else { "fail" }));
        if self.trail.len() > 6 {
            self.trail.remove(0);
        }
        if name == "wait" {
            self.wait_accum += result.get("waited_seconds").and_then(Value::as_f64).unwrap_or(0.0);
        } else {
            self.wait_accum = 0.0;
        }
        result
    }

    /// Re-ground after an action: re-resolve the view (foreground-follow unless
    /// zoomed), render a marked frame, format state, and compute the #1 stuck +
    /// #2 state-delta notes.
    pub fn ground(&mut self, name: &str, args: &Value) -> Result<Grounded> {
        if !self.zoomed {
            self.view = window_view(self.target.as_deref());
        }
        let (b, v, fp) = render_view(&self.dir, self.step, self.view, self.grid, self.last_click)?;
        self.view = v;
        // Informational tools don't change the screen; skip the heavy UIA readout
        // dump so their OWN result (memory transcript, clipboard text, window list)
        // is the dominant signal instead of being buried under hundreds of on-screen
        // elements — which made the agent answer from the SCREEN, not from memory.
        if matches!(
            name,
            "search_memory" | "open_memory" | "read_clipboard" | "list_windows"
            | "browser_setup" | "browser_status" | "browser_reset" | "browser_read_page"
            | "browser_query" | "browser_eval" | "browser_tabs" | "browser_network"
            | "decline_browser_control"
        ) {
            eprintln!("[cc] step {:02} (info tool — screen readouts suppressed)", self.step);
            return Ok(Grounded { frame_b64: b, state_text: self.context_block(), notes: Vec::new() });
        }
        let elements = uia::enumerate(self.target.as_deref()).unwrap_or_default();
        // Did the click change ITS OWN target cell? Compare the region snapshot
        // taken just before the click (`click_before`) to the same region now
        // (`fp`, fingerprinted around the click point). Localized, so a timer or
        // animation elsewhere doesn't fool it. Only set for click_at/click_target.
        let visual_no_change = match self.click_before.take() {
            Some(before) => session::fingerprint_change(&before, &fp) < vc_min(),
            None => false,
        };
        let ro = readouts_inline(&elements);
        let ro_short: String = ro.chars().take(220).collect();
        let more = if ro.chars().count() > 220 { " ..." } else { "" };
        eprintln!("[cc] step {:02} READOUTS ({} els): {ro_short}{more}", self.step, elements.len());
        let new_sig = state_signature(&elements);
        let ui_changed = self.prev_state_sig.as_deref() != Some(new_sig.as_str());
        self.prev_state_sig = Some(new_sig);
        let uia_action = matches!(name, "click_element" | "type_text" | "key_combination" | "open_url" | "launch_app");
        let act_sig = format!("{name}|{}", compact_args(args));
        self.recent_actions.push(act_sig.clone());
        if self.recent_actions.len() > 8 {
            self.recent_actions.remove(0);
        }
        // Repeating a scroll / navigation key is legitimate ("scroll down a lot"),
        // so don't flag those as a stuck loop.
        let is_nav = name == "key_combination"
            && args.get("keys").and_then(Value::as_str).map(is_nav_keys).unwrap_or(false);
        let stuck = !is_nav && self.recent_actions.iter().filter(|a| **a == act_sig).count() >= 3;
        let mut notes: Vec<(&'static str, &'static str)> = Vec::new();
        if visual_no_change {
            eprintln!("[cc] step {:02} NO VISUAL CHANGE after {name}", self.step);
            notes.push(("screen_change", "NONE - the visible screen did NOT change after this action, so it likely did NOT register (wrong target, the element isn't focused, or this surface ignores that input). Try a different approach - do not just repeat it."));
        }
        if uia_action && !ui_changed && !visual_no_change {
            notes.push(("ui_change", "none - the accessible UI did not change after this action; it may not have registered."));
        }
        if stuck {
            eprintln!("[cc] step {:02} STUCK: repeated '{act_sig}'", self.step);
            notes.push(("stuck_warning", "You have repeated the same action ~3 times with no progress (the target likely isn't where you think, or the click isn't landing). Change approach: zoom in for a closer look, use a more specific click_target description, or restart."));
        }
        let state = format!(
            "{}\n\n{}",
            self.context_block(),
            format_state(&elements, self.target.as_deref(), self.view, self.grid)
        );
        Ok(Grounded { frame_b64: b, state_text: state, notes })
    }

    /// Independent high-res vision check of a `done` claim. Returns (accepted,
    /// verdict text). On checker error it accepts (don't trap the agent).
    pub fn verify_done(&self, task: &str, cancel: &AtomicBool) -> (bool, String) {
        let full = window_view(self.target.as_deref());
        let q = format!(
            "A computer agent claims this task is COMPLETE: \"{task}\". Looking ONLY at this screenshot, is that goal \
actually achieved right now? Start your answer with YES or NO, then quote the exact on-screen evidence (or state \
what is actually shown instead)."
        );
        match read_view(full, &q, &format!("task: {task}"), cancel) {
            Ok(a) => (a.trim_start().to_lowercase().starts_with("yes"), a),
            Err(e) => (true, format!("(vision check unavailable: {e})")),
        }
    }

    pub fn final_review(&self, note: &str) {
        final_review(&self.dir, self.target.as_deref(), note);
    }
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
    send(&mut socket, realtime_text(&format!("{st0}\n\nYOUR TASK: {task}\nBegin.")))?;

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
            eprintln!("[cc] CC_FORCE_DROP: simulating connection drop at step {}", brain.step);
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
                send(&mut socket, realtime_text(&format!(
                    "(reconnected after a dropped connection) Resume this task: {task}\nContinue from the CURRENT \
state shown below.\n{}",
                    g.state_text
                )))?;
                continue;
            }
        };
        for ev in parse_server_message(&frame) {
            match ev {
                ServerEvent::ModelText(t) | ServerEvent::OutputTranscript(t) => reasoning.push_str(&t),
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
                        if say.is_empty() { "(none stated)" } else { say.as_str() }
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
                    let mut resp = json!({"action_result": action_result, "new_state": g.state_text});
                    for (k, v) in &g.notes {
                        resp[*k] = json!(*v);
                    }
                    send(&mut socket, tool_response(&id, &name, resp))?; // answer first
                    send(&mut socket, realtime_video_jpeg_b64(&g.frame_b64))?; // then the new frame
                }
                _ => {}
            }
        }
    }
    eprintln!("[cc] STOPPED at step {} (timeout/max-steps without done)", brain.step);
    brain.final_review("(stopped without done)");
    Ok(())
}

/// Longest edge target for the view crop sent to the model (short edge actually).
const VIEW_SHORT: u32 = 1024;

/// Short-edge size for the CLEAN crop sent to the aux vision reader. Larger than
/// the Live frame (the reader is not token-capped) so fine detail survives.
const VISION_SHORT: u32 = 1600;

/// Read the current view with the aux vision stack (clean crop, no grid overlay).
/// `ctx` is task/intent context for disambiguation. Returns the plain answer.
fn read_view(view: View, question: &str, ctx: &str, cancel: &AtomicBool) -> Result<String> {
    let cap = session::capture_virtual()?;
    let (jpeg, _shown) = session::encode_view(&cap, view, VISION_SHORT, None, None)?;
    let (q, c) = (question.to_string(), ctx.to_string());
    run_cancellable(cancel, move || super::vision_reader::read_image(&jpeg, &q, &c))
}

/// Run a (slow, blocking) vision call on a worker thread while polling `cancel`
/// every 50ms. A barge-in returns immediately ("cancelled") instead of blocking
/// the agent on a 15-25s HTTP round-trip; the abandoned call finishes in the
/// background and its result is dropped. Capture/encode stay on the caller (fast).
fn run_cancellable<T, F>(cancel: &AtomicBool, work: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T> + Send + 'static,
{
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(work());
    });
    loop {
        if cancel.load(Ordering::SeqCst) {
            anyhow::bail!("cancelled by user");
        }
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(r) => return r,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => anyhow::bail!("vision worker disconnected"),
        }
    }
}

/// Ask the aux vision stack for the click point of `description`, returned as
/// 0-1000 over `view` (+ what's there). DEFAULT: a SINGLE point call (fast, and
/// point-based so it's accurate for normal targets). `CC_LOCATE_MODE=refine` adds
/// a second zoomed pass for tiny adjacent cells (2x the latency); `=box` uses one
/// bounding-box call.
fn locate_in_view(view: View, description: &str, ctx: &str, cancel: &AtomicBool) -> Result<Located> {
    let cap = session::capture_virtual()?;
    let (jpeg, _s) = session::encode_view(&cap, view, VISION_SHORT, None, None)?;
    match std::env::var("CC_LOCATE_MODE").as_deref() {
        Ok("refine") => refine_in_view(&cap, view, &jpeg, description, ctx, cancel),
        Ok("box") => {
            let (j, d, c) = (jpeg.clone(), description.to_string(), ctx.to_string());
            match run_cancellable(cancel, move || super::vision_reader::locate_box(&j, &d, &c)) {
                Ok(p) => Ok(p),
                Err(_) => {
                    let (j, d, c) = (jpeg, description.to_string(), ctx.to_string());
                    run_cancellable(cancel, move || super::vision_reader::locate_point(&j, &d, &c))
                }
            }
        }
        // DEFAULT: one point call - half the latency of refine, accurate for
        // normal UI; opt into refine for tiny adjacent cells (game boards).
        _ => {
            let (j, d, c) = (jpeg, description.to_string(), ctx.to_string());
            run_cancellable(cancel, move || super::vision_reader::locate_point(&j, &d, &c))
        }
    }
}

/// Ask the aux vision stack to map EVERY target matching `description` to a list
/// of points (0-1000 over `view`), cancellable. Used to build reusable click
/// anchors in one call.
fn map_in_view(view: View, description: &str, ctx: &str, cancel: &AtomicBool) -> Result<Vec<Located>> {
    let cap = session::capture_virtual()?;
    let (jpeg, _s) = session::encode_view(&cap, view, VISION_SHORT, None, None)?;
    let (d, c) = (description.to_string(), ctx.to_string());
    run_cancellable(cancel, move || super::vision_reader::locate_points(&jpeg, &d, &c))
}

/// Two-call coarse-to-fine locate: point over the whole view, then ZOOM a box
/// around it and point again so the target fills the frame.
fn refine_in_view(
    cap: &session::Capture,
    view: View,
    coarse_jpeg: &[u8],
    description: &str,
    ctx: &str,
    cancel: &AtomicBool,
) -> Result<Located> {
    let coarse = {
        let (j, d, c) = (coarse_jpeg.to_vec(), description.to_string(), ctx.to_string());
        run_cancellable(cancel, move || super::vision_reader::locate_point(&j, &d, &c))?
    };
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
    let fine = {
        let (d, c) = (description.to_string(), ctx.to_string());
        let fine_model = std::env::var("CC_VISION_FINE_MODEL").ok().filter(|m| !m.trim().is_empty());
        run_cancellable(cancel, move || match fine_model {
            Some(m) => super::vision_reader::locate_point_with(&fine_jpeg, &d, m.trim(), &c),
            None => super::vision_reader::locate_point(&fine_jpeg, &d, &c),
        })
    };
    match fine {
        Ok(f) => {
            let (fsx, fsy) = shown.to_screen_px(f.x, f.y);
            let mx = ((fsx - view.x) as f64 / view.w.max(1) as f64 * 1000.0).clamp(0.0, 1000.0);
            let my = ((fsy - view.y) as f64 / view.h.max(1) as f64 * 1000.0).clamp(0.0, 1000.0);
            eprintln!("[cc] locate refine: coarse({:.0},{:.0}) -> fine({mx:.0},{my:.0})", coarse.x, coarse.y);
            Ok(Located { x: mx, y: my, note: f.note.or(coarse.note) })
        }
        Err(_) => Ok(coarse),
    }
}

/// True if every token in a `key_combination` is a scroll/navigation key — those
/// are legitimately repeated (paging through a feed), so the stuck detector skips
/// them. A combo with a non-nav key (e.g. Ctrl+C) is not navigation.
fn is_nav_keys(keys: &str) -> bool {
    const NAV: &[&str] = &[
        "up", "down", "left", "right", "pageup", "pagedown", "home", "end", "space", "tab", "scroll",
    ];
    let toks: Vec<String> = keys
        .split(['+', ' '])
        .filter(|s| !s.is_empty())
        .map(|s| s.to_lowercase())
        .collect();
    !toks.is_empty() && toks.iter().all(|t| NAV.contains(&t.as_str()))
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
        &AtomicBool::new(false),
    )
    .unwrap_or_else(|e| format!("(vision read failed: {e})"));
    let _ = std::fs::write(
        format!("{dir}/final.txt"),
        format!("NOTE: {note}\n\nFINAL VISION READING:\n{reading}\n"),
    );
    eprintln!("[cc] FINAL REVIEW ({note}):\n{reading}");

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
    // Start the Gemini Live worker pool so a gemini-live vision model
    // (CC_VISION_MODEL=gemini-live-vision-3.1) is reachable from this CLI - it
    // routes through that worker (image-attach -> audio -> outputTranscription).
    crate::api::gemini_live::init_gemini_live();
    std::thread::sleep(Duration::from_millis(200));
    let view = window_view(target);
    eprintln!("[vision-test] reading view ({},{},{},{})", view.x, view.y, view.w, view.h);
    let never = AtomicBool::new(false);
    let answer = read_view(view, question, "", &never)?;
    eprintln!("[vision-test] ANSWER:\n{answer}");
    if let Ok(desc) = std::env::var("CC_LOCATE")
        && !desc.trim().is_empty()
    {
        let loc = locate_in_view(view, &desc, "", &never)?;
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
/// Half-width (screen px) of the box compared before/after a click for the "did
/// it register?" signal.
const VC_HALF: i32 = 90;

/// Minimum changed fingerprint cells (of 1024) to count as a real on-screen change
/// in that box. Above the cursor's own footprint, below a placed mark / reveal.
/// Tunable via CC_VC_MIN.
fn vc_min() -> u32 {
    std::env::var("CC_VC_MIN").ok().and_then(|s| s.parse().ok()).unwrap_or(14)
}

fn render_view(
    dir: &str,
    step: usize,
    view: View,
    grid: Grid,
    marker: Option<(i32, i32)>,
) -> Result<(String, View, Vec<u8>)> {
    let cap = session::capture_virtual()?;
    let (jpeg, shown) = session::encode_view(&cap, view, VIEW_SHORT, Some(grid), marker)?;
    // Fingerprint the CLEAN region around the click (no grid/marker overlay), so we
    // can tell whether the click changed ITS OWN cell - ignoring a timer/animation
    // elsewhere. With no marker (turn 0 / keyboard) fall back to the whole view.
    let fp = match marker {
        Some((mx, my)) => session::region_fingerprint(&cap, mx, my, VC_HALF),
        None => session::view_fingerprint(&cap, shown),
    };
    std::fs::write(format!("{dir}/step-{step:02}.jpg"), &jpeg).ok();
    eprintln!("[cc] step {step:02} frame {} KB", jpeg.len() / 1024);
    Ok((general_purpose::STANDARD.encode(&jpeg), shown, fp))
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
