//! The Live session setup payload (`build_setup`) and the controller prompt
//! addendum — split out of `uia_task.rs` for the file-size limit. The big `SYS`
//! system instruction stays in the parent (referenced here as `super::SYS`).

use serde_json::{Value, json};

use super::super::{executor, protocol};

/// The default controller guidance: observe/act are the primary way to read and
/// act on real UI - on connected browser PAGES and on DESKTOP apps with accessible
/// controls. Vision tools (look / click_target / click_at) are for canvas/games.
const CONTROLLER_RULES: &str = "CONTROLLER (your PRIMARY way to read + act on real UI - works on connected browser \
pages AND on desktop apps with accessible controls): call observe() to get the current view as an INDEXED list of \
interactive elements (each @id has a role, its paired label, current value, flags like required/disabled, and a \
'⚠ reason' when acting on it is consequential), then act(id, verb, value) to act on one BY ITS @id. The controller \
pairs every label to its field (so you never click the wrong control), VERIFIES a fill actually landed (reads the \
value back), and is a SAFETY CHECKPOINT - it BLOCKS a ⚠ consequential act (sign-out, payment, purchase, account \
delete) and a submit with a required field still empty; to proceed past a ⚠ you must confirm with the user, then \
re-issue the SAME act with confirm:true. Prefer observe/act for all normal UI work and re-observe() whenever the view \
changes. For a KNOWN multi-step run on a stable view (e.g. filling a form), observe() then do_steps([{id,verb,value}, \
...]) runs them all in ONE call, each verified, stopping at the first problem. For a canvas / board / game, or \
anything observe() returns no elements for, use look() + click_target / click_at instead.";

pub(crate) fn build_setup(resume: Option<&str>, voice: bool, search: bool) -> Value {
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
    // Tell the agent its current privilege level so it reaches the most powerful action available in
    // the current mode — and knows when to escalate via UAC rather than silently failing.
    let privilege = if executor::is_elevated() {
        "PRIVILEGE: you are running ELEVATED (full administrator) - run_command has admin rights, so do system tasks directly."
    } else {
        "PRIVILEGE: you are running as a STANDARD user (not elevated). run_command still does most things; but admin-only tasks (stop a service, kill another user's or a protected process, system-wide settings) fail with Access Denied - for THOSE, relaunch just that command via run_command with Start-Process -Verb RunAs (the user approves one UAC prompt), then verify."
    };
    let mut setup = json!({ "setup": {
        "model": format!("models/{}", protocol::MODEL),
        "generationConfig": {
            "responseModalities": ["AUDIO"],
            "speechConfig": {"voiceConfig": {"prebuiltVoiceConfig": {"voiceName": voice_name}}},
            "mediaResolution": "MEDIA_RESOLUTION_HIGH",
            "maxOutputTokens": max_out,
            "thinkingConfig": {"thinkingLevel": think, "includeThoughts": true}
        },
        "systemInstruction": {"parts": [{"text": format!("{}\n{}\n{}\n{privilege}", super::SYS, CONTROLLER_RULES, protocol::session_rules())}]},
        "tools": [{"googleSearch": {}}, {"functionDeclarations": [
            {"name": "observe", "description": "Read the CURRENT web page as an INDEXED list of its interactive elements - each with an @id, role, its paired label, current value, and flags (required / disabled / destructive). Use this on a connected browser page instead of guessing pixels or CSS selectors: the label is paired to its own field, so you target the right control. Returns the list; then act(id, ...) on any element. Re-run after the page changes.",
             "parameters": {"type": "object", "properties": {}}},
            {"name": "act", "description": "Act on an element from the latest observe() list, BY ITS @id. The controller resolves the exact element, performs the action, VERIFIES it took (reads the value back), and is a SAFETY CHECKPOINT: it BLOCKS a consequential/irreversible act (one whose @id is flagged with a '⚠' reason - signs you out, submits a payment or password, starts a purchase/transfer, deletes an account) and any submit while a required field is empty, returning a 'blocked' message to read and act on. Always observe() first to get current ids; the result includes the refreshed element list.",
             "parameters": {"type": "object", "properties": {
                 "id": {"type": "integer", "description": "The @id of the target element from observe()."},
                 "verb": {"type": "string", "enum": ["click", "fill", "select", "submit", "toggle"], "description": "click a button/link; fill a text field (value=text); select a dropdown option (value=option); submit a form; toggle a checkbox."},
                 "value": {"type": "string", "description": "The text to fill, or the option to select. Omit for click/submit/toggle."},
                 "confirm": {"type": "boolean", "description": "Set true ONLY to clear a consequential-action checkpoint AFTER the user just explicitly approved this exact action (e.g. a payment, sign-out, account change, posting/sending content). Never set it pre-emptively or on your own judgement."}
             }, "required": ["id", "verb"]}},
            {"name": "do_steps", "description": "Run a SHORT SEQUENCE of controller actions in ONE call (after a single observe), each gated + verified, stopping at the first failure and returning how far it got + the refreshed element list. Use it for a known multi-step run on a stable view - e.g. fill a form (several fills then submit) - to avoid a round-trip per step. observe() FIRST to get the @ids, then pass them here in order. If it stops early, read 'stopped', re-observe, and continue from there.",
             "parameters": {"type": "object", "properties": {
                 "steps": {"type": "array", "description": "The ordered steps, each shaped like an act() call.", "items": {"type": "object", "properties": {
                     "id": {"type": "integer"}, "verb": {"type": "string", "enum": ["click", "fill", "select", "submit", "toggle"]},
                     "value": {"type": "string"}, "confirm": {"type": "boolean"}
                 }, "required": ["id", "verb"]}}
             }, "required": ["steps"]}},
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
             "parameters": {"type": "object", "properties": {"description": {"type": "string", "description": "Unambiguous target, e.g. 'the generated image' or 'the download button'."}, "button": {"type": "string", "enum": ["left", "right"], "description": "left (default) or right for a context menu."}}, "required": ["description"]}},
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
             "parameters": {"type": "object", "properties": {"from": {"type": "string", "description": "What to grab, e.g. 'the selected card in my hand'."}, "to": {"type": "string", "description": "Where to drop it, e.g. 'the empty center slot of the board'."}}, "required": ["from", "to"]}},
            {"name": "click_here", "description": "Click EXACTLY where the mouse cursor currently is, without moving it (button='right' for a context menu). Use when the user refers to what THEY are pointing at - 'this', 'the one I'm hovering on', 'where my mouse is' - because their pointer is already on the target. Far more reliable than guessing the target by description with click_target.",
             "parameters": {"type": "object", "properties": {"button": {"type": "string", "enum": ["left", "right", "middle"]}}}},
            {"name": "point_at", "description": "MOVE the mouse onto a target described in plain words and STOP there - point/hover, NO click. Use when the user wants you to POINT something OUT to them ('point at the save button', 'show me which one', 'where is X') rather than act on it, OR to HOVER and reveal a tooltip / hover-menu. A high-res vision model locates the exact pixel (same as click_target). Set dwell_seconds to linger so a hover reveal can appear before you look() again.",
             "parameters": {"type": "object", "properties": {"description": {"type": "string", "description": "Unambiguous target to point at, e.g. 'the settings gear' or 'the second result'."}, "dwell_seconds": {"type": "number", "description": "Optional: seconds to hover in place (0-10) to let a tooltip / hover-menu surface. Default 0."}}, "required": ["description"]}},
            {"name": "key_combination", "description": "Press a keyboard shortcut (e.g. Enter, Control+C, Alt+Tab) - keys are held a few frames so even a game registers the press. To MOVE / walk in a game, or hold a key down, set hold_seconds (the key stays DOWN that long - e.g. hold 'd' or 'Right' for 1-2s to move the character).",
             "parameters": {"type": "object", "properties": {"keys": {"type": "string"}, "hold_seconds": {"type": "number", "description": "Hold the key(s) DOWN this many seconds before releasing (0-10). Use it to walk/move in a game or hold a key; omit for a normal quick shortcut."}}, "required": ["keys"]}},
            {"name": "open_url", "description": "Open an http(s) URL in the default browser as a NEW foreground tab (via the OS shell). Use this to go to a web page directly - far more reliable than typing into the address bar.",
             "parameters": {"type": "object", "properties": {"url": {"type": "string"}}, "required": ["url"]}},
            {"name": "launch_app", "description": "Launch or focus a Windows app by name/path via the OS shell, e.g. 'chrome', 'notepad', 'explorer'. Pass 'args' to give it arguments - e.g. open a file in an app: name='notepad', args='C:\\\\path\\\\file.txt' (or just launch_app the file path itself to open it in its default app). Do NOT cram args into 'name'.",
             "parameters": {"type": "object", "properties": {"name": {"type": "string"}, "args": {"type": "string", "description": "Optional command-line arguments / file to open."}}, "required": ["name"]}},
            {"name": "system_query", "description": "Read structured facts about the COMPUTER ITSELF through trusted built-in providers. Use this BEFORE run_command for OS facts: active audio apps -> domain:'audio', query:'active_sessions'; open windows -> domain:'window', query:'list'; process list -> domain:'process', query:'list_basic'; clipboard text -> domain:'clipboard', query:'text'. Start with domain:'capabilities', query:'list' if unsure. Read-only: it never changes the system.",
             "parameters": {"type": "object", "properties": {
                 "domain": {"type": "string", "enum": ["capabilities", "audio", "clipboard", "process", "window"]},
                 "query": {"type": "string", "description": "The query inside the domain, e.g. 'active_sessions', 'list_basic', 'list', or 'text'."},
                 "args": {"type": "object", "description": "Optional query args, e.g. {\"include_inactive\": true}, {\"limit\": 50}, or {\"name_contains\": \"chrome\"}."}
             }, "required": ["domain", "query"]}},
            {"name": "run_command", "description": "Run a PowerShell command and get stdout/stderr/exit. Use as a LAST-RESORT system escape hatch when no dedicated tool or system_query domain fits, or for a user-requested shell operation. Add '| ConvertTo-Json -Depth 4' when you need structured data back. Non-interactive: a command that prompts FAILS rather than hangs. If a task needs ADMIN and you are NOT elevated (see PRIVILEGE), relaunch JUST that command elevated via Start-Process -Verb RunAs (one UAC prompt for the user), then verify. Returns truncated stdout/stderr.",
             "parameters": {"type": "object", "properties": {"command": {"type": "string", "description": "The PowerShell command line to run."}}, "required": ["command"]}},
            {"name": "focus_window", "description": "Bring an already-open window to the FRONT by a piece of its title OR its app/exe name (e.g. 'Chrome', 'Notepad', 'Game.exe'). It matches the EXE name too, so target a FULLSCREEN GAME by its app name from list_windows (its on-screen title may collide with a browser tab about it). Restores the window if it was minimized. Returns the window now in front so you can confirm. If it reports the SAME covering window, that app is exclusive-fullscreen (a game) — you canNOT switch away from it and must not minimize what the user is playing; read any web content with browser_read_page instead, or ask the user to alt-tab.",
             "parameters": {"type": "object", "properties": {"title": {"type": "string", "description": "A substring of the target window's title bar."}}, "required": ["title"]}},
            {"name": "list_windows", "description": "List every open top-level window as 'title [app.exe]' — INCLUDING fullscreen GAMES that have no normal title bar and don't appear to other tools — so you know what's open to focus_window or minimize_window, and can target a game by its app name. No arguments.",
             "parameters": {"type": "object", "properties": {}}},
            {"name": "minimize_window", "description": "Minimize a window by a piece of its title - use this to get a FULLSCREEN game or app OUT OF THE WAY when it covers what you need (it works even when the game swallows alt+tab/Win+D keystrokes, because it acts on the window directly). Returns what's in front afterward.",
             "parameters": {"type": "object", "properties": {"title": {"type": "string", "description": "A substring of the window to minimize."}}, "required": ["title"]}},
            {"name": "resize_window", "description": "Resize a window (matched by a piece of its title) to width x height in PIXELS. Restores it first if maximized, so you can make it smaller. e.g. resize_window('Notepad', 700, 500).",
             "parameters": {"type": "object", "properties": {"title": {"type": "string"}, "width": {"type": "integer"}, "height": {"type": "integer"}}, "required": ["title", "width", "height"]}},
            {"name": "move_window", "description": "Move a window (matched by a piece of its title) so its top-left corner is at screen pixel (x, y). Keeps its current size.",
             "parameters": {"type": "object", "properties": {"title": {"type": "string"}, "x": {"type": "integer"}, "y": {"type": "integer"}}, "required": ["title", "x", "y"]}},
            {"name": "read_clipboard", "description": "Read the text currently on the Windows clipboard (e.g. what you or the user just copied). Lets you grab a selection without retyping it. No arguments. (type_text already PASTES via the clipboard, so writing long text is fast.)",
             "parameters": {"type": "object", "properties": {}}},
            {"name": "artifact_info", "description": "Inspect a local large-content artifact by id/path: counts, SHA-256, saved path, and a short preview. Use for verification; it does NOT return the full text.",
             "parameters": {"type": "object", "properties": {"id": {"type": "string", "description": "Artifact id returned by browser_extract_page/browser_read_page, or an artifact file path."}}, "required": ["id"]}},
            {"name": "save_artifact", "description": "Save/copy a local text artifact to a file path without routing its contents through the model. If path is omitted, returns the artifact's existing temp file path. Use for exact export/file workflows.",
             "parameters": {"type": "object", "properties": {"id": {"type": "string"}, "path": {"type": "string", "description": "Optional absolute output path."}, "overwrite": {"type": "boolean", "description": "Default false; true to overwrite an existing file."}}, "required": ["id"]}},
            {"name": "paste_artifact", "description": "Paste a local text artifact into the currently focused app by setting the clipboard from the artifact and pressing Ctrl+V. Use this for large/exact transfers into Word, Notepad, email, chats, etc. Do NOT use type_text with the artifact preview/full text.",
             "parameters": {"type": "object", "properties": {"id": {"type": "string", "description": "Artifact id returned by browser_extract_page/browser_read_page, or an artifact file path."}}, "required": ["id"]}},
            {"name": "done", "description": "Call ONLY when the goal is confirmed achieved; quote the evidence.",
             "parameters": {"type": "object", "properties": {"summary": {"type": "string"}}, "required": ["summary"]}},
            {"name": "search_memory", "description": "Search YOUR memory of PAST conversations (every prior session is saved). Use when the user refers to something from before ('remember when we...', 'what did we decide about X', 'last time'). Returns matching past conversations as numbered results with a title + snippet + id. Then call open_memory(id) to read the full one.",
             "parameters": {"type": "object", "properties": {"query": {"type": "string", "description": "What to recall, in plain words, e.g. 'the plan for the memory feature' or 'the quest story'."}}, "required": ["query"]}},
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
            {"name": "research_web", "description": "Search/read web sources for factual verification or definitions, returning source-aware answer material. Use this when the user asks to search/verify, says Google, asks for sources, or asks about terms you are not certain about. Default source_policy 'best_available' reads result pages when possible instead of relying on snippets.",
             "parameters": {"type": "object", "properties": {
                 "query": {"type": "string", "description": "Concise search query for the user's question."},
                 "purpose": {"type": "string", "description": "Why this research is needed for the current user turn."},
                 "source_policy": {"type": "string", "enum": ["best_available", "broad"], "description": "best_available reads source pages when possible; broad keeps the search general."},
                 "max_sources": {"type": "integer", "description": "Number of source pages to read, 1-5. Default 3."}
             }, "required": ["query"]}},
            {"name": "browser_extract_page", "description": "Extract the current page's full visible DOM text into a local artifact and return only metadata/counts/preview. Use this for exact copy/export or any page text too large to safely pass through the model; then call paste_artifact or save_artifact with artifact.id.",
             "parameters": {"type": "object", "properties": {}}},
            {"name": "browser_wait_for", "description": "Wait until an element matching a CSS selector appears (or timeout). Use after a click/navigation that loads content.",
             "parameters": {"type": "object", "properties": {"selector": {"type": "string"}, "timeout_ms": {"type": "integer"}}, "required": ["selector"]}},
            {"name": "browser_eval", "description": "Run JavaScript in the page and return its (JSON-able) result. Your general escape hatch for extracting structured data or doing precise DOM work. NEVER call alert(), confirm() or prompt() in your code, and don't write an infinite/blocking loop - they FREEZE the page and this call hangs until it times out; for game-over or messages, draw on the canvas or set element text instead. Use setInterval/requestAnimationFrame (non-blocking) for game loops.",
             "parameters": {"type": "object", "properties": {"code": {"type": "string", "description": "A JS expression; its value is returned (use an IIFE for statements). Must NOT block (no alert/confirm/prompt, no while(true))."}}, "required": ["code"]}},
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
            {"name": "browser_console", "description": "Read the page's CONSOLE output - console.log/info/warn/error(...) calls AND browser log entries (CORS / security / network / deprecation errors). Use this to DEBUG a web app or read the 'real developer error' behind a failure, instead of opening DevTools. Enables capture if needed, so the first call may be empty - run the page, then call again.",
             "parameters": {"type": "object", "properties": {}}},
            {"name": "decline_browser_control", "description": "Call ONLY when the user declines your offer to set up deep browser control - records it so you stop asking this session and don't nag (you may bring it up again much later). No args.",
             "parameters": {"type": "object", "properties": {}}},
            {"name": "list_app_integrations", "description": "List the CURATED app-control integrations available - dedicated MCP tools that drive a specific app's real API far more precisely than clicking its UI - and whether each is installed/connected. Use to see what deeper control you can offer for the app the user is working in. No args.",
             "parameters": {"type": "object", "properties": {}}},
            {"name": "setup_app_integration", "description": "Install + activate a curated app-control integration by its id (from list_app_integrations) so you gain its precise tools instead of guessing clicks. It INSTALLS AND RUNS third-party software, so get the user's explicit YES first, THEN call with confirmed:true. Its tools become available after a brief reconnect.",
             "parameters": {"type": "object", "properties": {"id": {"type": "string", "description": "Integration id from list_app_integrations."}, "confirmed": {"type": "boolean", "description": "Pass true ONLY after the user agreed to install it."}}, "required": ["id"]}},
            {"name": "app_integration_status", "description": "Check whether a curated app-control integration is actually ready: pinned MCP server connected, app-side readiness probe passed, and tools active after reconnect. Use this to verify setup and stop; do not guess from screenshots.",
             "parameters": {"type": "object", "properties": {"id": {"type": "string", "description": "Integration id."}}, "required": ["id"]}},
            {"name": "read_app_integration_docs", "description": "Fetch the curated integration's own README/docs from its catalog source URL. Use this to research in-app setup. This cannot fetch arbitrary model-provided URLs.",
             "parameters": {"type": "object", "properties": {"id": {"type": "string", "description": "Integration id."}}, "required": ["id"]}},
            {"name": "remove_app_integration", "description": "Uninstall and disconnect a curated app-control integration by id (stops its server, forgets it). Use when the user wants it gone.",
             "parameters": {"type": "object", "properties": {"id": {"type": "string", "description": "Integration id."}}, "required": ["id"]}},
            {"name": "decline_app_integration", "description": "Call ONLY when the user declines your proactive offer to set up an app integration - snoozes that offer so you don't nag (you may bring it up again much later).",
             "parameters": {"type": "object", "properties": {"id": {"type": "string", "description": "Integration id."}}, "required": ["id"]}}
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
    // Append any connected MCP integrations' tools. Gemini freezes the tool set at setup, so
    // installing/removing an integration triggers a reconnect that re-runs build_setup.
    let mcp_decls = super::super::mcp::active_tool_declarations();
    if !mcp_decls.is_empty()
        && let Some(fd) = setup["setup"]["tools"]
            .as_array_mut()
            .and_then(|tools| {
                tools
                    .iter_mut()
                    .find_map(|t| t.get_mut("functionDeclarations"))
            })
            .and_then(|d| d.as_array_mut())
    {
        fd.extend(mcp_decls);
    }
    setup
}
