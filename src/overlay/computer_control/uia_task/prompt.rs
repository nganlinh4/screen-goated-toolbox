//! The Live session setup payload (`build_setup`) and the controller prompt
//! addendum — split out of `uia_task.rs` for the file-size limit. The compact
//! core system contract stays in the parent (referenced here as `super::SYS`).

use serde_json::{Value, json};

use crate::api::gemini_live::setup::{LiveSetupBuilder, MediaResolution, TranscriptionMode};

use super::super::{executor, protocol};

/// A compact route map keeps the broad tool kit usable without repeating every
/// declaration in the system prompt. It ranks evidence by fidelity without
/// encoding task phrases or application-specific workflows.
const CONTROLLER_RULES: &str = "TOOL ROUTING: use the highest-fidelity available evidence. For an accessible surface, observe and act by current @id. For pixel-only content, use vision targets or current marks. Prefer direct browser, system, file, or integration providers when they expose the needed state. Raw input requires known focus and intended effect. Change route after a typed failure.";

pub(crate) fn build_setup(resume: Option<&str>, voice: bool, search: bool) -> Value {
    build_setup_with_integrations(resume, voice, search, true)
}

pub(super) fn build_setup_with_integrations(
    resume: Option<&str>,
    voice: bool,
    search: bool,
    include_integrations: bool,
) -> Value {
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
    let tools = json!([{"googleSearch": {}}, {"functionDeclarations": [
        {"name": "observe", "description": "Read the current controlled surface as an indexed list of interactive elements with @id, role, paired label, value, and structural flags. Returns current ids for act; re-run after the surface changes.",
         "parameters": {"type": "object", "properties": {}}},
        {"name": "act", "description": "Act on an @id from the latest observe. The controller resolves the exact element, enforces structural preconditions, preserves an execution receipt, verifies supported effects, and returns a fresh element list.",
         "parameters": {"type": "object", "properties": {
             "id": {"type": "integer", "description": "The @id of the target element from observe()."},
             "verb": {"type": "string", "enum": ["click", "activate", "fill", "select", "submit", "toggle"], "description": "click for one ordinary click (selection/simple control); activate for a default enter/open action; fill a text field; select an option; submit a form; toggle a checkbox."},
             "value": {"type": "string", "description": "The text to fill, or the option to select. Omit for click/submit/toggle."},
             "confirm": {"type": "boolean", "description": "Set true ONLY to clear a consequential-action checkpoint AFTER the user just explicitly approved this exact action (e.g. a payment, sign-out, account change, posting/sending content). Never set it pre-emptively or on your own judgement."}
         }, "required": ["id", "verb"]}},
        {"name": "do_steps", "description": "Run a short ordered sequence of current @id actions on one stable surface. Every step is independently resolved, gated, executed, and verified; execution stops at the first failure and returns fresh state.",
         "parameters": {"type": "object", "properties": {
             "steps": {"type": "array", "description": "The ordered steps, each shaped like an act() call.", "items": {"type": "object", "properties": {
                 "id": {"type": "integer"}, "verb": {"type": "string", "enum": ["click", "activate", "fill", "select", "submit", "toggle"]},
                 "value": {"type": "string"}, "confirm": {"type": "boolean"}
             }, "required": ["id", "verb"]}}
         }, "required": ["steps"]}},
        {"name": "click_at", "description": "Click the CENTER of a numbered grid cell. Only for UIA-blind canvas/image space; native elements in the cell are blocked. Never use to choose among collection items.",
         "parameters": {"type": "object", "properties": {"cell": {"type": "integer", "description": "The grid number printed over the target."}}, "required": ["cell"]}},
        {"name": "zoom", "description": "Magnify the numbered GRID CELL so small targets become large and a fresh finer grid is drawn over it. Pass the cell's printed number.",
         "parameters": {"type": "object", "properties": {"cell": {"type": "integer", "description": "The grid number to magnify."}}, "required": ["cell"]}},
        {"name": "reset_view", "description": "Return the view to the ACTIVE window (the default; undoes zoom and see_whole_screen).",
         "parameters": {"type": "object", "properties": {}}},
        {"name": "see_whole_screen", "description": "Switch perception to the entire desktop for cross-window awareness. Return to an active-window view before precise input.",
         "parameters": {"type": "object", "properties": {}}},
        {"name": "look", "description": "Get a precise high-resolution reading of visible content that structured providers cannot expose. Ask one specific spatial or visual question; the result comes from a clean capture of the current view.",
         "parameters": {"type": "object", "properties": {"question": {"type": "string", "description": "A precise question about the visible content and its spatial state."}}, "required": ["question"]}},
        {"name": "click_target", "description": "Click a visually described target only when structured providers expose no usable element. A grounding model locates it, then a fresh marked crop verifies the exact point before input. Prefer act on semantic elements. Set button='right' for a context menu.",
         "parameters": {"type": "object", "properties": {"description": {"type": "string", "description": "A visible, unambiguous target description."}, "button": {"type": "string", "enum": ["left", "right"], "description": "left (default) or right for a context menu."}}, "required": ["description"]}},
        {"name": "map_targets", "description": "Build numbered candidate anchors in one vision call for a structured-access-blind region. click_mark freshly verifies a mapped point before clicking, and any mutating action invalidates the set.",
         "parameters": {"type": "object", "properties": {"description": {"type": "string", "description": "The visible target set to map."}}, "required": ["description"]}},
        {"name": "click_mark", "description": "Click a numbered anchor shown on the current grounded frame. Stale marks fail closed. Set button='right' for a context menu.",
         "parameters": {"type": "object", "properties": {"mark": {"type": "integer", "description": "The anchor number from map_targets."}, "button": {"type": "string", "enum": ["left", "right"]}}, "required": ["mark"]}},
        {"name": "wait", "description": "Pause for N seconds only when an asynchronous operation is known to be pending. Then re-observe.",
         "parameters": {"type": "object", "properties": {"seconds": {"type": "number", "description": "Seconds to wait (max 30)."}}, "required": ["seconds"]}},
        {"name": "type_text", "description": "Insert text at the current keyboard focus. Text is always literal, including newlines and brace tokens. Set press_enter=true only when the requested effect includes a separate Enter keypress.",
         "parameters": {"type": "object", "properties": {"text": {"type": "string"}, "press_enter": {"type": "boolean", "description": "Press Enter after typing (to submit)."}, "slow": {"type": "boolean", "description": "Rarely needed: type slowly key-by-key for a field that genuinely demands paced input. Default false (fast)."}}, "required": ["text"]}},
        {"name": "scroll", "description": "Inject a mouse-wheel scroll in the requested direction. Optionally target a current grid cell; otherwise scroll at view center.",
         "parameters": {"type": "object", "properties": {"direction": {"type": "string", "enum": ["up", "down", "left", "right"]}, "amount": {"type": "number"}, "cell": {"type": "integer"}}, "required": ["direction"]}},
        {"name": "drag", "description": "Press at one current grid cell, move to another, and release. Zoom first when either endpoint needs finer localization.",
         "parameters": {"type": "object", "properties": {"from_cell": {"type": "integer", "description": "Grid cell to press at."}, "to_cell": {"type": "integer", "description": "Grid cell to release at."}}, "required": ["from_cell", "to_cell"]}},
        {"name": "drag_target", "description": "Precisely drag between two visually described endpoints when grid cells are too coarse. A vision model locates both current pixels before input.",
         "parameters": {"type": "object", "properties": {"from": {"type": "string", "description": "The visible object or handle to grab."}, "to": {"type": "string", "description": "The visible destination or final handle position."}}, "required": ["from", "to"]}},
        {"name": "click_here", "description": "Click exactly at the user's current cursor without moving it. Use only when the requested target is explicitly cursor-relative.",
         "parameters": {"type": "object", "properties": {"button": {"type": "string", "enum": ["left", "right", "middle"]}}}},
        {"name": "point_at", "description": "Move the cursor onto a visually described target without clicking. Optional dwell_seconds keeps the pointer there for hover state.",
         "parameters": {"type": "object", "properties": {"description": {"type": "string", "description": "A visible, unambiguous target description."}, "dwell_seconds": {"type": "number", "description": "Optional hover duration, 0-10 seconds."}}, "required": ["description"]}},
        {"name": "key_combination", "description": "Press a keyboard key or chord. Set hold_seconds only when the requested interaction is duration-sensitive; otherwise input is a normal quick press.",
         "parameters": {"type": "object", "properties": {"keys": {"type": "string"}, "hold_seconds": {"type": "number", "description": "Seconds to hold all requested keys before release (0-10)."}}, "required": ["keys"]}},
        {"name": "open_url", "description": "Open an http(s) URL in a new foreground tab through the OS shell.",
         "parameters": {"type": "object", "properties": {"url": {"type": "string"}}, "required": ["url"]}},
        {"name": "launch_app", "description": "Launch or focus a Windows application or open a file through the OS shell. Put the executable/path in name and keep optional command-line arguments separate in args.",
         "parameters": {"type": "object", "properties": {"name": {"type": "string"}, "args": {"type": "string", "description": "Optional command-line arguments / file to open."}}, "required": ["name"]}},
        {"name": "system_query", "description": "Read trusted OS facts without mutation; capabilities.list describes domains.",
         "parameters": {"type": "object", "properties": {
             "domain": {"type": "string", "enum": ["capabilities", "audio", "clipboard", "process", "window"]},
             "query": {"type": "string", "description": "The query inside the domain, e.g. 'active_sessions', 'list_basic', 'list', or 'text'."},
             "args": {"type": "object", "description": "Optional query filters."}
         }, "required": ["domain", "query"]}},
        {"name": "list_files", "description": "Read-only directory metadata. For relative choices, sort and use the returned rank and exact name.",
         "parameters": {"type": "object", "properties": {
             "path": {"type": "string", "description": "Absolute path or standard folder name."},
             "kind": {"type": "string", "enum": ["any", "file", "directory"]},
             "extensions": {"type": "array", "items": {"type": "string"}},
             "sort_by": {"type": "string", "enum": ["modified", "created", "name", "size"]},
             "order": {"type": "string", "enum": ["descending", "ascending"]},
             "limit": {"type": "integer"}
         }, "required": ["path"]}},
        {"name": "run_command", "description": "Run a noninteractive PowerShell command and return bounded stdout, stderr, and exit status. Use only when no dedicated capability fits or the user requested a shell operation.",
         "parameters": {"type": "object", "properties": {"command": {"type": "string", "description": "The PowerShell command line to run."}}, "required": ["command"]}},
        {"name": "focus_window", "description": "Bring an already-open top-level window to the foreground by exact normalized title/executable or the stable target returned by list_windows. Duplicate exact matches fail with stable choices.",
         "parameters": {"type": "object", "properties": {"title": {"type": "string", "description": "Exact title/executable, or @hwnd:<handle>:<pid> from list_windows."}}, "required": ["title"]}},
        {"name": "list_windows", "description": "List open top-level windows as title, executable, and stable identity so another window tool can target an unambiguous match.",
         "parameters": {"type": "object", "properties": {}}},
        {"name": "minimize_window", "description": "Minimize an exact or stable top-level window target and return the resulting foreground identity.",
         "parameters": {"type": "object", "properties": {"title": {"type": "string", "description": "Exact title/executable, or stable target from list_windows."}}, "required": ["title"]}},
        {"name": "resize_window", "description": "Resize an exact or stable top-level window target to width x height in screen pixels, restoring it first if necessary.",
         "parameters": {"type": "object", "properties": {"title": {"type": "string"}, "width": {"type": "integer"}, "height": {"type": "integer"}}, "required": ["title", "width", "height"]}},
        {"name": "move_window", "description": "Move an exact or stable top-level window target so its top-left corner is at screen pixel (x, y). Keeps its current size.",
         "parameters": {"type": "object", "properties": {"title": {"type": "string"}, "x": {"type": "integer"}, "y": {"type": "integer"}}, "required": ["title", "x", "y"]}},
        {"name": "read_clipboard", "description": "Read current Windows clipboard text without mutation.",
         "parameters": {"type": "object", "properties": {}}},
        {"name": "artifact_info", "description": "Inspect local artifact metadata, counts, hash, path, and bounded preview without returning full content.",
         "parameters": {"type": "object", "properties": {"id": {"type": "string", "description": "Artifact id returned by browser_extract_page/browser_read_page, or an artifact file path."}}, "required": ["id"]}},
        {"name": "save_artifact", "description": "Save or copy a local text artifact without routing its full content through the model.",
         "parameters": {"type": "object", "properties": {"id": {"type": "string"}, "path": {"type": "string", "description": "Optional absolute output path."}, "overwrite": {"type": "boolean", "description": "Default false; true to overwrite an existing file."}}, "required": ["id"]}},
        {"name": "paste_artifact", "description": "Paste a local text artifact into the focused destination without routing its full contents through the model. Use for large or exact transfers.",
         "parameters": {"type": "object", "properties": {"id": {"type": "string", "description": "Artifact id returned by browser_extract_page/browser_read_page, or an artifact file path."}}, "required": ["id"]}},
        {"name": "done", "description": "Finish a confirmed computer action; quote evidence. Never use for an answer.",
         "parameters": {"type": "object", "properties": {"summary": {"type": "string"}}, "required": ["summary"]}},
        {"name": "search_memory", "description": "Search saved prior-conversation records when the current request depends on earlier context. Returns matching ids and bounded snippets.",
         "parameters": {"type": "object", "properties": {"query": {"type": "string", "description": "The prior context to retrieve."}}, "required": ["query"]}},
        {"name": "open_memory", "description": "Read the FULL transcript of one past conversation returned by search_memory. Pass its id.",
         "parameters": {"type": "object", "properties": {"id": {"type": "string", "description": "The conversation id from a search_memory result."}}, "required": ["id"]}},
        {"name": "browser_setup", "description": "Prepare the browser-control extension and return its bounded setup state. Use only when browser control is needed but disconnected; follow the returned state once, then check browser_status.",
         "parameters": {"type": "object", "properties": {}}},
        {"name": "browser_status", "description": "Check browser-bridge connection, negotiated capabilities, compatibility, staged-update state, and pairing state.",
         "parameters": {"type": "object", "properties": {}}},
        {"name": "browser_reset", "description": "Reset browser-control pairing state and reopen its pairing window. Use only when the connection state requires repair.",
         "parameters": {"type": "object", "properties": {}}},
        {"name": "browser_read_page", "description": "Read the controlled page's title, URL, and visible DOM text.",
         "parameters": {"type": "object", "properties": {}}},
        {"name": "research_web", "description": "Retrieve source-aware web evidence for claims requiring external verification. Default source_policy 'best_available' reads source pages when possible.",
         "parameters": {"type": "object", "properties": {
             "query": {"type": "string", "description": "Concise search query for the user's question."},
             "purpose": {"type": "string", "description": "Why this research is needed for the current user turn."},
             "source_policy": {"type": "string", "enum": ["best_available", "broad"], "description": "best_available reads source pages when possible; broad keeps the search general."},
             "max_sources": {"type": "integer", "description": "Number of source pages to read, 1-5. Default 3."}
         }, "required": ["query"]}},
        {"name": "browser_extract_page", "description": "Extract full visible DOM text into a local artifact and return only metadata, counts, and preview.",
         "parameters": {"type": "object", "properties": {}}},
        {"name": "browser_wait_for", "description": "Wait until an element matching a CSS selector appears (or timeout). Use after a click/navigation that loads content.",
         "parameters": {"type": "object", "properties": {"selector": {"type": "string"}, "timeout_ms": {"type": "integer"}}, "required": ["selector"]}},
        {"name": "browser_eval", "description": "Run nonblocking JavaScript in the page and return a JSON-compatible result. Modal dialogs and blocking or unbounded loops are forbidden.",
         "parameters": {"type": "object", "properties": {"code": {"type": "string", "description": "A JS expression; its value is returned. An IIFE containing statements must explicitly return a JSON-compatible value. Must NOT block (no alert/confirm/prompt, no while(true))."}}, "required": ["code"]}},
        {"name": "browser_navigate", "description": "Navigate the controlled tab to a URL (replaces what's on it). Use when the current page is disposable or the user wants to go somewhere fresh.",
         "parameters": {"type": "object", "properties": {"url": {"type": "string"}}, "required": ["url"]}},
        {"name": "browser_open_tab", "description": "Open a URL in a new tab while preserving the current page. Call browser_switch_tab with its returned id before targeting the new tab with page tools.",
         "parameters": {"type": "object", "properties": {"url": {"type": "string"}}, "required": ["url"]}},
        {"name": "browser_upload", "description": "Set the file for a file <input> matching a CSS selector (real upload via DevTools). Pass an absolute file path.",
         "parameters": {"type": "object", "properties": {"selector": {"type": "string"}, "path": {"type": "string"}}, "required": ["selector", "path"]}},
        {"name": "browser_tabs", "description": "List the open browser tabs (id, title, url, active).",
         "parameters": {"type": "object", "properties": {}}},
        {"name": "browser_switch_tab", "description": "Select a browser tab by id (from browser_tabs). Page tools keep targeting that exact tab for the rest of the current user turn even if focus moves.",
         "parameters": {"type": "object", "properties": {"tab_id": {"type": "integer"}}, "required": ["tab_id"]}},
        {"name": "browser_close_tab", "description": "Close exactly one browser tab by its id (from browser_tabs).",
         "parameters": {"type": "object", "properties": {"tab_id": {"type": "integer"}}, "required": ["tab_id"]}},
        {"name": "browser_network", "description": "Read bounded recent network events from the controlled page, enabling capture first if needed.",
         "parameters": {"type": "object", "properties": {"filter": {"type": "string", "description": "Optional substring of the CDP event name, e.g. 'responseReceived'."}}}},
        {"name": "browser_console", "description": "Read bounded page console and browser-log events, enabling capture first if needed.",
         "parameters": {"type": "object", "properties": {}}},
        {"name": "list_app_integrations", "description": "List curated precise-control providers and their installed/connected state. Use only when such a provider is relevant to the requested outcome.",
         "parameters": {"type": "object", "properties": {}}},
        {"name": "setup_app_integration", "description": "Install and activate a curated precise-control provider by id. This runs third-party software and requires explicit user approval represented by confirmed:true.",
         "parameters": {"type": "object", "properties": {"id": {"type": "string", "description": "Integration id from list_app_integrations."}, "confirmed": {"type": "boolean", "description": "Pass true ONLY after the user agreed to install it."}}, "required": ["id"]}},
        {"name": "app_integration_status", "description": "Return structural readiness evidence for a curated provider: connection, app-side probe, and tool activation.",
         "parameters": {"type": "object", "properties": {"id": {"type": "string", "description": "Integration id."}}, "required": ["id"]}},
        {"name": "read_app_integration_docs", "description": "Fetch the curated integration's own README/docs from its catalog source URL. Use this to research in-app setup. This cannot fetch arbitrary model-provided URLs.",
         "parameters": {"type": "object", "properties": {"id": {"type": "string", "description": "Integration id."}}, "required": ["id"]}},
        {"name": "remove_app_integration", "description": "Uninstall and disconnect a curated app-control integration by id (stops its server, forgets it). Use when the user wants it gone.",
         "parameters": {"type": "object", "properties": {"id": {"type": "string", "description": "Integration id."}}, "required": ["id"]}}
    ]}]);
    let mut setup = LiveSetupBuilder::new(protocol::MODEL)
        .media_resolution(MediaResolution::High)
        .voice(&voice_name)
        .thinking_override(protocol::thinking_config())
        .system_instruction(&format!(
            "{}\n{}\n{}\n{privilege}",
            super::SYS,
            CONTROLLER_RULES,
            protocol::session_rules()
        ))
        .transcription(TranscriptionMode::Both)
        .context_window_compression()
        .setup_field("tools", tools)
        .setup_field("sessionResumption", resumption)
        .build();
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
    let mcp_decls = integration_declarations(include_integrations, || {
        super::super::mcp::active_tool_declarations()
    });
    append_integration_declarations(&mut setup, mcp_decls);
    setup
}

fn integration_declarations(include: bool, load: impl FnOnce() -> Vec<Value>) -> Vec<Value> {
    if include { load() } else { Vec::new() }
}

fn append_integration_declarations(setup: &mut Value, declarations: Vec<Value>) {
    if !declarations.is_empty()
        && let Some(fd) = setup["setup"]["tools"]
            .as_array_mut()
            .and_then(|tools| {
                tools
                    .iter_mut()
                    .find_map(|t| t.get_mut("functionDeclarations"))
            })
            .and_then(|d| d.as_array_mut())
    {
        fd.extend(declarations);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use serde_json::Value;

    fn declarations(setup: &Value) -> &[Value] {
        setup["setup"]["tools"]
            .as_array()
            .and_then(|tools| {
                tools
                    .iter()
                    .find_map(|tool| tool.get("functionDeclarations"))
            })
            .and_then(Value::as_array)
            .expect("function declarations")
    }

    #[test]
    fn setup_catalog_has_unique_named_tools() {
        let setup = super::build_setup(None, false, false);
        let declarations = declarations(&setup);
        let mut names = HashSet::new();
        for declaration in declarations {
            let name = declaration["name"].as_str().expect("tool name");
            assert!(names.insert(name), "duplicate tool declaration: {name}");
            assert!(
                declaration["description"]
                    .as_str()
                    .is_some_and(|d| !d.trim().is_empty()),
                "missing description: {name}"
            );
        }
        eprintln!(
            "setup profile: {} tools, {} system bytes, {} declaration bytes, {} total bytes",
            declarations.len(),
            setup["setup"]["systemInstruction"].to_string().len(),
            serde_json::to_string(declarations).unwrap().len(),
            setup.to_string().len()
        );
        assert_eq!(
            declarations.len(),
            57,
            "built-in capability was added or lost"
        );
        assert!(
            serde_json::to_string(declarations).unwrap().len() <= 20_000,
            "function catalog exceeded its reviewed prompt budget"
        );
        assert!(
            setup["setup"]["systemInstruction"].to_string().len() < 5_000,
            "system instruction exceeded its reviewed prompt budget"
        );
        assert!(
            setup.to_string().len() <= 42_000,
            "base Live setup exceeded its reviewed prompt budget"
        );
    }

    #[test]
    fn exact_tab_close_requires_a_tab_id() {
        let setup = super::build_setup(None, false, false);
        let close = declarations(&setup)
            .iter()
            .find(|declaration| declaration["name"] == "browser_close_tab")
            .expect("browser_close_tab declaration");
        assert_eq!(
            close["parameters"]["required"],
            serde_json::json!(["tab_id"])
        );
        assert_eq!(
            close["parameters"]["properties"]["tab_id"]["type"],
            "integer"
        );
    }

    #[test]
    fn integration_omission_is_scoped_to_one_setup() {
        let declaration = serde_json::json!({
            "name": "future_integration_tool",
            "parameters": {"type": "object", "properties": {}}
        });
        let omitted = super::integration_declarations(false, || vec![declaration.clone()]);
        let included = super::integration_declarations(true, || vec![declaration]);

        assert!(omitted.is_empty());
        assert_eq!(included.len(), 1);
        assert_eq!(included[0]["name"], "future_integration_tool");
    }
}
