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
mod render;
mod vision;
use render::*;
use vision::*;

const SYS: &str = "You control a Windows PC, ONE tool action per turn. Each turn you get a SCREENSHOT of the ACTIVE \
window with a NUMBERED GRID over it, plus its READOUTS and CLICKABLE elements (Windows accessibility = ground truth, \
each tagged @cellN = its grid cell). zoom() into a cell for detail. To see your WHOLE screen (all windows) - for \
awareness, counting/finding across windows, or to reach another window - call see_whole_screen; reset_view returns \
to the precise active-window view. \
click_at(cell): click that grid number. zoom(cell): magnify it (grid redrawn with new numbers); reset_view undoes \
it. click_element(name): click a listed element. Also type_text, key_combination. \
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
BROWSER NAVIGATION: once connected, open URLs with the extension, NOT open_url (which opens a new WINDOW). Choose: \
browser_navigate replaces the CURRENT tab - use it when the current page is disposable or the user wants to go \
somewhere fresh; browser_open_tab opens a NEW tab in the same window, keeping the current page - use it when the user \
is working with the current page or wants something opened alongside. When unsure, prefer a new tab (less \
disruptive). Reserve open_url for when the extension isn't connected or to leave a chrome:// page. \
The DOM tools (browser_query/click/fill) see the MAIN page, not CROSS-ORIGIN iframes (some login/payment/embed \
widgets) - if a selector isn't found, the target may be in such a frame: fall back to vision (click_target / \
click_here on what the user points at). \
MULTI-STEP TASKS: once you START a multi-step task (e.g. browser setup), carry out ALL its steps BACK-TO-BACK in one \
go - do NOT do one step then stop and wait for the user. If the user makes a remark or asks a status question ('are \
you doing it?', 'why did you stop?') mid-task, answer in ONE short sentence if needed but KEEP GOING immediately with \
the next step in the same turn. Only stop for an explicit 'stop'/'wait' or when you truly need their input. \
BROWSER CONTROL SETUP: if the USER asks to set up / enable / turn on browser control, just call browser_setup \
RIGHT AWAY - do NOT offer or ask 'would you like'. Offering is ONLY for the proactive heads-up: when a heads-up tells \
you the user is browsing without deep control, you may offer ONCE, briefly; if they accept, run browser_setup; if they \
decline, call decline_browser_control and drop it. Never offer twice. \
READING/SUMMARIZING A WEB PAGE: when deep browser control is connected, call browser_read_page ONCE to get the \
ENTIRE page's text in one shot - do NOT scroll screen-by-screen to read a page (slow, you lose your place, and you \
loop). Use scroll + look ONLY to inspect a SPECIFIC figure/chart/image the text refers to. Then answer from the \
full text - don't keep scrolling once you have it. \
To answer a question, look() at the CURRENT screen FIRST - it reads ONLY what is on screen now (it does NOT search \
the web). If the answer is already visible, just read it - do NOT open a search. ONLY when the needed information \
is genuinely NOT on the current screen, open_url('https://www.google.com/search?q=...') and read the results. \
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
GAMES: read the board with look()/zoom and plan the WHOLE sequence before moving; never act blindly. Play by whatever \
the game uses - keyboard for arrow/key games (2048), or click_target / drag_target for pointer, card and tile games. \
If a game running in a BROWSER ignores your clicks or drags (a canvas/WebGL game often does - plain OS clicks aren't \
trusted by the page), that is exactly when to set up deep browser control with browser_setup: once it is connected, \
click_target and drag_target automatically drive the page's OWN trusted input, which works on canvas/WebGL/iframe \
games. Then retry the same move.";

pub(super) fn build_setup(resume: Option<&str>, voice: bool, search: bool) -> Value {
    // "low" (not "medium") for a fast, action-oriented real-time agent: medium
    // thinking noticeably slows every turn and made it over-deliberate (narrate
    // instead of act). Override with CC_THINK=minimal|low|medium|high.
    let think = std::env::var("CC_THINK").unwrap_or_else(|_| "low".to_string());
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
            {"name": "reset_view", "description": "Return the view to the ACTIVE window (the default; undoes zoom and see_whole_screen).",
             "parameters": {"type": "object", "properties": {}}},
            {"name": "see_whole_screen", "description": "Switch the view to your ENTIRE screen (all windows) instead of just the active window. Use it for awareness - 'what's on my screen', counting/finding things across windows, or locating another window to switch to. Acting precisely (clicks) is best in the default active-window view, so reset_view (or focus_window) afterward.",
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
            {"name": "type_text", "description": "Type text at the current keyboard focus (FAST - instant/paste). Set press_enter=true to submit afterward (e.g. an address bar or search box) - do NOT put '{enter}' inside the text, it would be typed literally.",
             "parameters": {"type": "object", "properties": {"text": {"type": "string"}, "press_enter": {"type": "boolean", "description": "Press Enter after typing (to submit)."}, "slow": {"type": "boolean", "description": "Rarely needed: type slowly key-by-key for a field that genuinely demands paced input. Default false (fast)."}}, "required": ["text"]}},
            {"name": "scroll", "description": "Scroll with the REAL mouse wheel (not PageDown) over the page/list. direction up/down (or left/right); 'amount' is how far (default 5; larger scrolls more). Optionally pass a grid 'cell' to scroll over a specific area, else it scrolls over the centre. Prefer this for natural scrolling.",
             "parameters": {"type": "object", "properties": {"direction": {"type": "string", "enum": ["up", "down", "left", "right"]}, "amount": {"type": "number"}, "cell": {"type": "integer"}}, "required": ["direction"]}},
            {"name": "drag", "description": "Press at one grid cell, glide to another, and release - for sliders, reordering items, drawing, or click-drag to SELECT text/items. Pass from_cell and to_cell (the printed grid numbers). zoom() first for finer cells when precision matters.",
             "parameters": {"type": "object", "properties": {"from_cell": {"type": "integer", "description": "Grid cell to press at."}, "to_cell": {"type": "integer", "description": "Grid cell to release at."}}, "required": ["from_cell", "to_cell"]}},
            {"name": "drag_target", "description": "PRECISE drag-and-drop: a vision model locates the EXACT pixel of BOTH endpoints (described in plain words) and drags from one to the other. Use this - NOT drag(cells) - to place a card on a board slot, drop an item, or move a slider on a canvas/game, where grid cells are too coarse to hit the small targets.",
             "parameters": {"type": "object", "properties": {"from": {"type": "string", "description": "What to grab, e.g. 'the selected Full Moon card in my hand'."}, "to": {"type": "string", "description": "Where to drop it, e.g. 'the empty center slot of the board'."}}, "required": ["from", "to"]}},
            {"name": "click_here", "description": "Click EXACTLY where the mouse cursor currently is, without moving it (button='right' for a context menu). Use when the user refers to what THEY are pointing at - 'this', 'the one I'm hovering on', 'where my mouse is' - because their pointer is already on the target. Far more reliable than guessing the target by description with click_target.",
             "parameters": {"type": "object", "properties": {"button": {"type": "string", "enum": ["left", "right", "middle"]}}}},
            {"name": "point_at", "description": "MOVE the mouse onto a target described in plain words and STOP there - point/hover, NO click. Use when the user wants you to POINT something OUT to them ('point at the save button', 'show me which one', 'where is X') rather than act on it, OR to HOVER and reveal a tooltip / hover-menu. A high-res vision model locates the exact pixel (same as click_target). Set dwell_seconds to linger so a hover reveal can appear before you look() again.",
             "parameters": {"type": "object", "properties": {"description": {"type": "string", "description": "Unambiguous target to point at, e.g. 'the settings gear' or 'the second result'."}, "dwell_seconds": {"type": "number", "description": "Optional: seconds to hover in place (0-10) to let a tooltip / hover-menu surface. Default 0."}}, "required": ["description"]}},
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
            {"name": "browser_setup", "description": "Bring up DEEP browser control (read/act on the real page DOM, not just pixels) via the SGT browser extension. It writes the extension folder and returns it plus a 'do_yourself' checklist. DO the install YOURSELF with your tools (toggle Developer mode, Load unpacked the folder) - do NOT recite steps to the user. It auto-pairs over the socket, so there is NO code to paste and NO popup. Pause ONLY if a permission prompt appears. Then poll browser_status.",
             "parameters": {"type": "object", "properties": {}}},
            {"name": "browser_status", "description": "Check whether the deep-browser extension is connected. Returns connected, port, and whether a pairing window is open.",
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

/// CLI: read the foreground window with the aux vision stack and print the
/// answer (validates chain resolution / keys / provider dispatch). `--cc-vision-test`.
pub fn run_vision_test(target: Option<&str>, question: &str) -> Result<()> {
    // Start the Gemini Live worker pool so a gemini-live vision model
    // (CC_VISION_MODEL=gemini-live-vision-3.1) is reachable from this CLI - it
    // routes through that worker (image-attach -> audio -> outputTranscription).
    crate::api::gemini_live::init_gemini_live();
    std::thread::sleep(Duration::from_millis(200));
    let view = window_view(target, false);
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
