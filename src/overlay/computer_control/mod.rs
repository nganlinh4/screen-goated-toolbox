//! "Điều khiển máy tính" (Computer Control) — a Gemini Live screen+voice agent
//! that drives Windows via model tool calls. See `temp-computer-control-design.md`.
//!
//! - `protocol` — setup payloads, tool declarations, the server-frame decoder.
//! - `session` — connect/capture/send primitives shared by runtime + probe.
//! - `executor` — `SendInput` mouse/keyboard with frame→screen coordinate mapping.
//! - `runtime` — the continuous session loop (mic + screen → tool calls → actions).
//! - `overlay` — the always-on-top status/action-log UI + session lifecycle.
//! - `probe`   — the `--computer-control-probe` de-risk harness.

mod clipboard;
mod coord_test;
mod executor;
mod grid;
mod human_input;
mod memory;
mod overlay;
mod playback;
mod probe;
mod protocol;
mod runtime;
mod session;
mod trace;
mod uia;
mod uia_task;
mod vision_reader;

pub use overlay::{is_active, render_overlay, show_overlay, stop_overlay};

/// CLI entry for the de-risk probe: `--computer-control-probe [--cc-task "..."]`.
pub fn run_probe_cli(task: &str) -> Result<(), String> {
    probe::run(task).map_err(|e| format!("{e:?}"))
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

/// CLI entry for the model-free human-cursor demo: `--cc-cursor-demo`.
pub fn run_cursor_demo_cli() {
    executor::cursor_demo();
}

/// Headless CLI entry: run the real session loop (mic + screen + execute) with
/// stderr logging and no GUI overlay. Runs until the process is killed.
/// `--computer-control-run`.
pub fn run_headless() {
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    // The voice session may fall back to the gemini-live vision model, which needs
    // the Live worker pool (the app's main() starts it, but this CLI path doesn't).
    crate::api::gemini_live::init_gemini_live();
    // Run on a FRESH thread (like show_overlay) so cpal's WASAPI mic gets a clean
    // COM apartment — main() has already CoInitialize'd this process's main thread
    // for the GUI, which would otherwise trip RPC_E_CHANGED_MODE.
    let stop = Arc::new(AtomicBool::new(false));
    let _ = std::thread::spawn(move || runtime::run(stop)).join();
}
