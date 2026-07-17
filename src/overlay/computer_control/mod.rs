//! Computer Control — a Gemini Live screen+voice agent that drives Windows via
//! model tool calls. Development contract:
//! `docs/COMPUTER_CONTROL_DEVELOPMENT.md`.
//!
//! - `protocol` — setup payloads, tool declarations, the server-frame decoder.
//! - `session` — connect/capture/send primitives shared by runtime + probe.
//! - `executor` — `SendInput` mouse/keyboard with frame→screen coordinate mapping.
//! - `runtime` — the continuous session loop (mic + screen → tool calls → actions).
//! - `overlay` — the always-on-top status/action-log UI + session lifecycle.
//! - `probe`   — the `--computer-control-probe` de-risk harness.

mod artifacts;
mod browser;
mod clipboard;
mod controller;
mod coord_test;
mod detector;
mod effect_receipt;
mod executor;
mod grid;
mod human_input;
mod mcp;
mod memory;
mod orb;
mod overlay;
mod playback;
mod probe;
mod protocol;
mod research;
mod runtime;
mod session;
mod system_query;
mod telemetry;
mod trace;
mod turn_policy;
mod uia;
mod uia_task;
mod vision_reader;

/// Detector model hooks for the Downloaded Tools settings UI (download/remove/probe).
pub(crate) use detector::{
    DOWNLOAD_TITLE as DETECTOR_DOWNLOAD_TITLE, detector_model_dir, download_detector_model,
    is_detector_downloaded, remove_detector_model,
};
/// MCP capability-store hooks for the Downloaded Tools settings UI (list/install/remove).
pub(crate) use mcp::{ui_install, ui_list, ui_remove, ui_remove_all};
pub use overlay::{is_active, show_overlay, stop_overlay};

/// CLI entry for the de-risk probe. Multiple tasks run in one real Live session,
/// which exercises conversation-state behavior without enabling input execution.
pub fn run_probe_cli(tasks: &[String]) -> Result<(), String> {
    probe::run(tasks).map_err(|e| format!("{e:?}"))
}

/// CLI entry for the coordinate-convention debug: `--cc-coord-test`.
pub fn run_coord_test_cli() -> Result<(), String> {
    coord_test::run().map_err(|e| format!("{e:?}"))
}

/// CLI entry for the task-trace harness: `--cc-task-trace --cc-task "..."`.
pub fn run_task_trace_cli(task: &str) -> Result<(), String> {
    trace::run(task).map_err(|e| format!("{e:?}"))
}

/// CLI entry for the UIA ground-truth element dump: `--cc-uia-dump`.
pub fn run_uia_dump_cli(target: Option<&str>) -> Result<(), String> {
    uia::run_dump(target).map_err(|e| format!("{e:?}"))
}

/// CLI entry for the UIA-grounded task workhorse: `--cc-uia-task --cc-task "..."`.
pub fn run_uia_task_cli(task: &str) -> Result<(), String> {
    uia_task::run(task).map_err(|e| format!("{e:?}"))
}

/// CLI entry for the grid-overlay legibility check: `--cc-grid-test`.
pub fn run_grid_test_cli(target: Option<&str>) -> Result<(), String> {
    uia_task::run_grid_test(target).map_err(|e| format!("{e:?}"))
}

/// CLI entry for the aux vision-stack smoke test: `--cc-vision-test`.
pub fn run_vision_test_cli(target: Option<&str>, question: &str) -> Result<(), String> {
    uia_task::run_vision_test(target, question).map_err(|e| format!("{e:?}"))
}

/// CLI entry for validated local UI-DETR inference: `--cc-detector-test`.
pub fn run_detector_test_cli(target: Option<&str>) -> Result<(), String> {
    detector::run_test(target).map_err(|e| format!("{e:?}"))
}

/// CLI entry for the MCP stdio bridge smoke test: `--cc-mcp-test <id>` (no Gemini).
pub fn run_mcp_test_cli(
    id: &str,
    tool: Option<&str>,
    args_json: Option<&str>,
    list_only: bool,
) -> Result<(), String> {
    mcp::run_mcp_test(id, tool, args_json, list_only)
}

/// CLI entry for typed OS fact queries: `--cc-system-query-test audio.active_sessions`.
pub fn run_system_query_test_cli(spec: &str, args_json: Option<&str>) -> Result<(), String> {
    let (domain, query) = spec
        .split_once('.')
        .ok_or_else(|| "expected <domain>.<query>, e.g. audio.active_sessions".to_string())?;
    let args = match args_json {
        Some(raw) => serde_json::from_str(raw).map_err(|e| format!("invalid args JSON: {e}"))?,
        None => serde_json::json!({}),
    };
    let result = system_query::query(&serde_json::json!({
        "domain": domain,
        "query": query,
        "args": args,
    }));
    println!(
        "{}",
        serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?
    );
    if result
        .get("ok")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        Ok(())
    } else {
        Err(result
            .get("error")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("system_query failed")
            .to_string())
    }
}

/// CLI entry for the model-free human-cursor demo: `--cc-cursor-demo`.
pub fn run_cursor_demo_cli() {
    executor::cursor_demo();
}

/// Headless CLI entry: run the real session loop (mic + screen + execute) with
/// stderr logging and no GUI overlay. Runs until the process is killed.
/// `--computer-control-run`.
pub fn run_headless(scripted_turns: Option<Vec<String>>) -> anyhow::Result<()> {
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    // The voice session may fall back to the gemini-live vision model, which needs
    // the Live worker pool (the app's main() starts it, but this CLI path doesn't).
    crate::api::gemini_live::init_gemini_live();
    // Run on a FRESH thread (like show_overlay) so cpal's WASAPI mic gets a clean
    // COM apartment — main() has already CoInitialize'd this process's main thread
    // for the GUI, which would otherwise trip RPC_E_CHANGED_MODE.
    let stop = Arc::new(AtomicBool::new(false));
    std::thread::spawn(move || match scripted_turns {
        Some(turns) => runtime::run_scripted(stop, turns),
        None => {
            runtime::run(stop);
            Ok(())
        }
    })
    .join()
    .map_err(|_| anyhow::anyhow!("computer-control runtime thread panicked"))?
}
