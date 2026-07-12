#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// The Computer Control tool catalog is one large `json!` literal; its array
// expands recursively, so the default macro recursion limit (128) is too low.
#![recursion_limit = "512"]

mod api;
mod assets;
mod atomic_json;
mod config;
mod debug_log;
pub mod gui;
mod history;
mod hotkey;
mod icon_gen;
mod initialization;
pub mod lang_detect;
mod model_config;
mod overlay;
mod paths;
mod registry_integration;
mod retry_model_chain;
mod runtime_support;
mod screen_capture;
mod startup_launch;
mod unpack_dlls;
mod updater;
pub mod win_types;

use config::{Config, ThemeMode, load_config};
use gui::locale::LocaleText;
use history::HistoryManager;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Instant;
use tray_icon::menu::{CheckMenuItem, Menu, MenuItem};
use windows::Win32::Foundation::*;
use windows::Win32::System::Threading::*;
use windows::core::*;

pub use hotkey::RESTORE_EVENT;
pub use screen_capture::GdiCapture;

// Window dimensions
pub const WINDOW_WIDTH: f32 = 1250.0;
pub const WINDOW_HEIGHT: f32 = 650.0;
// Floor the user can't drag below — keeps the sidebar + editor usable.
pub const MIN_WINDOW_WIDTH: f32 = 1245.0;
pub const MIN_WINDOW_HEIGHT: f32 = 660.0;

// Wrappers for thread-safe types
use crate::win_types::SendHwnd;

pub struct AppState {
    pub config: Config,
    pub screenshot_handle: Option<GdiCapture>,
    pub hotkeys_updated: bool,
    pub registered_hotkey_ids: Vec<i32>,
    pub model_usage_stats: HashMap<String, String>,
    pub history: Arc<HistoryManager>,
    pub last_active_window: Option<SendHwnd>,
}

pub static APP: LazyLock<Arc<Mutex<AppState>>> = LazyLock::new(|| {
    Arc::new(Mutex::new({
        let config = load_config();
        let history = Arc::new(HistoryManager::new(config.max_history_items));
        AppState {
            config,
            screenshot_handle: None,
            hotkeys_updated: false,
            registered_hotkey_ids: Vec::new(),
            model_usage_stats: HashMap::new(),
            history,
            last_active_window: None,
        }
    }))
});

const PROCESS_WITH_SGT_FLAG: &str = "--process-with-sgt";
const SCREEN_RECORD_WRY_SMOKE_FLAG: &str = "--screen-record-wry-smoke";
const SCREEN_RECORD_WEBVIEW2_DEBUG_PORT_FLAG: &str = "--screen-record-webview2-debug-port";

fn parse_arg_value(args: &[String], key: &str) -> Option<String> {
    let mut idx = 0usize;
    while idx < args.len() {
        if args[idx] == key {
            return args.get(idx + 1).cloned();
        }
        idx += 1;
    }
    None
}

fn find_process_with_sgt_file_arg(args: &[String]) -> Option<std::path::PathBuf> {
    if !args.iter().any(|arg| arg == PROCESS_WITH_SGT_FLAG) {
        return None;
    }

    for arg in args.iter().skip(1) {
        if arg.starts_with("--") {
            continue;
        }
        let path = std::path::PathBuf::from(arg);
        if path.exists() && path.is_file() {
            return Some(path);
        }
    }

    None
}

fn resolve_replay_path(args: &[String]) -> Option<String> {
    parse_arg_value(args, "--sr-export-replay").or_else(|| {
        if args.iter().any(|arg| arg == "--sr-export-replay-last") {
            crate::overlay::screen_record::native_export::export_replay_args_path()
                .map(|p| p.to_string_lossy().to_string())
        } else {
            None
        }
    })
}

fn configure_screen_record_wry_smoke(args: &[String]) -> bool {
    let smoke_enabled = args.iter().any(|arg| arg == SCREEN_RECORD_WRY_SMOKE_FLAG);
    let Some(port) = parse_arg_value(args, SCREEN_RECORD_WEBVIEW2_DEBUG_PORT_FLAG) else {
        return smoke_enabled;
    };
    if port
        .parse::<u16>()
        .ok()
        .filter(|value| *value > 0)
        .is_none()
    {
        crate::log_info!("[WrySmoke] Ignoring invalid WebView2 debug port: {port}");
        return smoke_enabled;
    }
    let remote_arg = format!("--remote-debugging-port={port} --remote-debugging-address=0.0.0.0");
    let next_args = match std::env::var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS") {
        Ok(existing) if !existing.trim().is_empty() => format!("{existing} {remote_arg}"),
        _ => remote_arg,
    };
    unsafe {
        std::env::set_var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", next_args);
        if std::env::var("SGT_SCREEN_RECORD_WEBVIEW2_DATA_DIR").is_err() {
            std::env::set_var(
                "SGT_SCREEN_RECORD_WEBVIEW2_DATA_DIR",
                std::env::temp_dir()
                    .join(format!("sgt-record-wry-smoke-webview2-{port}"))
                    .to_string_lossy()
                    .to_string(),
            );
        }
    }
    crate::log_info!("[WrySmoke] Enabled WebView2 remote debugging on port {port}");
    smoke_enabled
}

fn native_options_for_wgpu(viewport: eframe::egui::ViewportBuilder) -> eframe::NativeOptions {
    let mut options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };

    if let eframe::egui_wgpu::WgpuSetup::CreateNew(create_new) =
        &mut options.wgpu_options.wgpu_setup
    {
        create_new.instance_descriptor.backends = eframe::wgpu::Backends::PRIMARY;
    }

    options
}

fn run_settings_window(
    screen_record_wry_smoke: bool,
    pending_file_path: Option<PathBuf>,
) -> eframe::Result<()> {
    let initial_config = APP.lock().unwrap().config.clone();

    let tray_locale = LocaleText::get(&initial_config.ui_language);
    let tray_menu = Menu::new();

    let has_favorites = initial_config.presets.iter().any(|p| p.is_favorite);
    let favorite_bubble_text = if has_favorites {
        tray_locale.tray_favorite_bubble
    } else {
        tray_locale.tray_favorite_bubble_disabled
    };
    let tray_favorite_bubble_item = CheckMenuItem::with_id(
        "1003",
        favorite_bubble_text,
        true,
        initial_config.show_favorite_bubble,
        None,
    );

    let tray_settings_item = MenuItem::with_id("1002", tray_locale.tray_settings, true, None);
    let tray_quit_item = MenuItem::with_id("1001", tray_locale.tray_quit, true, None);
    let _ = tray_menu.append(&tray_favorite_bubble_item);
    let _ = tray_menu.append(&tray_settings_item);
    let _ = tray_menu.append(&tray_quit_item);

    let mut viewport_builder = eframe::egui::ViewportBuilder::default()
        .with_inner_size([WINDOW_WIDTH, WINDOW_HEIGHT])
        .with_min_inner_size([MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT])
        .with_resizable(true)
        .with_visible(false)
        .with_transparent(false)
        .with_decorations(true);

    let system_dark = gui::utils::is_system_in_dark_mode();
    let effective_dark = match initial_config.theme_mode {
        ThemeMode::Dark => true,
        ThemeMode::Light => false,
        ThemeMode::System => system_dark,
    };

    let icon_data = crate::icon_gen::get_window_icon(effective_dark);
    viewport_builder = viewport_builder.with_icon(std::sync::Arc::new(icon_data));

    let options = native_options_for_wgpu(viewport_builder);

    crate::log_info!("[Main] Starting eframe with wgpu renderer");
    eframe::run_native(
        "Screen Goated Toolbox (SGT by nganlinh4)",
        options,
        Box::new(move |cc| {
            gui::configure_fonts(&cc.egui_ctx);
            *gui::GUI_CONTEXT.lock().unwrap() = Some(cc.egui_ctx.clone());
            gui::theme::AppTheme::apply_global_style(&cc.egui_ctx, effective_dark);
            gui::utils::update_window_icon_native(effective_dark);

            if screen_record_wry_smoke {
                std::thread::spawn(|| {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    crate::log_info!("[WrySmoke] Opening SGT Record window");
                    crate::overlay::screen_record::show_screen_record();
                });
            }

            Ok(Box::new(gui::SettingsApp::new(gui::SettingsAppInit {
                config: initial_config,
                app_state: APP.clone(),
                tray_menu,
                tray_settings_item,
                tray_quit_item,
                tray_favorite_bubble_item,
                ctx: cc.egui_ctx.clone(),
                pending_file_path,
            })))
        }),
    )
}

fn load_replay_payload(replay_path: &str) -> std::result::Result<serde_json::Value, String> {
    let raw = std::fs::read_to_string(replay_path).map_err(|e| {
        format!(
            "Failed to read export replay payload '{}': {}",
            replay_path, e
        )
    })?;
    serde_json::from_str::<serde_json::Value>(&raw).map_err(|e| {
        format!(
            "Invalid JSON in export replay payload '{}': {}",
            replay_path, e
        )
    })
}

fn percentile(sorted: &[f64], ratio: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let clamped = ratio.clamp(0.0, 1.0);
    let idx = ((sorted.len() - 1) as f64 * clamped).round() as usize;
    sorted[idx]
}

fn maybe_run_headless_export_replay(args: &[String]) -> Option<i32> {
    let replay_path = resolve_replay_path(args)?;
    let payload = match load_replay_payload(&replay_path) {
        Ok(v) => v,
        Err(err) => {
            eprintln!("[Replay] {}", err);
            return Some(2);
        }
    };

    initialization::init_com_and_dpi();
    let bench_runs = parse_arg_value(args, "--sr-export-replay-bench")
        .and_then(|raw| raw.parse::<usize>().ok())
        .filter(|runs| *runs > 0);
    let keep_outputs = args
        .iter()
        .any(|arg| arg == "--sr-export-replay-keep-output");

    if bench_runs.is_none() {
        println!("[Replay] Running native export replay from {}", replay_path);
        return match crate::overlay::screen_record::native_export::start_native_export(payload) {
            Ok(result) => {
                println!(
                    "[Replay] Export replay succeeded: {}",
                    serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string())
                );
                Some(0)
            }
            Err(err) => {
                eprintln!("[Replay] Export replay failed: {}", err);
                Some(1)
            }
        };
    }

    let runs = bench_runs.unwrap_or(1);
    println!(
        "[ReplayBench] Running {} native export replay run(s) from {}",
        runs, replay_path
    );
    let mut successful_wall_secs: Vec<f64> = Vec::with_capacity(runs);
    let mut failed_runs = 0usize;
    for run_idx in 0..runs {
        let run_start = Instant::now();
        match crate::overlay::screen_record::native_export::start_native_export(payload.clone()) {
            Ok(result) => {
                let wall_secs = run_start.elapsed().as_secs_f64();
                successful_wall_secs.push(wall_secs);
                let status = result
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let bytes = result.get("bytes").and_then(|v| v.as_u64()).unwrap_or(0);
                let output_path = result.get("path").and_then(|v| v.as_str()).unwrap_or("");
                println!(
                    "[ReplayBench] run={}/{} status={} wall={:.3}s bytes={} path={}",
                    run_idx + 1,
                    runs,
                    status,
                    wall_secs,
                    bytes,
                    if output_path.is_empty() {
                        "-"
                    } else {
                        output_path
                    }
                );
                if !keep_outputs && !output_path.is_empty() {
                    let _ = std::fs::remove_file(output_path);
                }
            }
            Err(err) => {
                failed_runs += 1;
                eprintln!("[ReplayBench] run={}/{} failed: {}", run_idx + 1, runs, err);
            }
        }
    }

    if successful_wall_secs.is_empty() {
        eprintln!("[ReplayBench] all runs failed");
        return Some(1);
    }

    let mut sorted = successful_wall_secs.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let sum: f64 = sorted.iter().copied().sum();
    let avg = sum / sorted.len() as f64;
    let min = *sorted.first().unwrap_or(&0.0);
    let max = *sorted.last().unwrap_or(&0.0);
    let p50 = percentile(&sorted, 0.50);
    let p90 = percentile(&sorted, 0.90);
    println!(
        "[ReplayBench] summary runs={} ok={} failed={} min={:.3}s p50={:.3}s p90={:.3}s avg={:.3}s max={:.3}s keep_outputs={}",
        runs,
        sorted.len(),
        failed_runs,
        min,
        p50,
        p90,
        avg,
        max,
        keep_outputs
    );
    Some(if failed_runs > 0 { 1 } else { 0 })
}

fn main() -> eframe::Result<()> {
    if initialization::setup_console_utf8() {
        println!("[Console] UTF-8 input/output enabled");
    } else {
        eprintln!("[Console] WARNING: failed to enable UTF-8 input/output");
    }
    let startup_args: Vec<String> = std::env::args().collect();
    if startup_args
        .iter()
        .any(|arg| arg == api::realtime_audio::sherpa_onnx::ffi_tts::SHERPA_TTS_LOAD_PROBE_FLAG)
    {
        std::process::exit(api::realtime_audio::sherpa_onnx::ffi_tts::run_load_probe_process());
    }

    crate::log_info!("========================================");
    crate::log_info!(
        "Screen Goated Toolbox v{} STARTUP",
        env!("CARGO_PKG_VERSION")
    );
    crate::log_info!("========================================");

    // Install panic reporting before any substantial startup work so early failures
    // do not exit silently on Windows release builds.
    initialization::setup_crash_handler();

    // Unpack embedded DLLs
    unpack_dlls::unpack_dlls();

    // Standalone CLI test for the Gemini Translate narration streaming: feed a
    // 16 kHz mono WAV, write `<wav>.narration.wav`, exit. No GUI / app wiring.
    if let Some(input_wav) = parse_arg_value(&startup_args, "--gt-narration-test") {
        let target_language = parse_arg_value(&startup_args, "--gt-narration-lang")
            .unwrap_or_else(|| "vi".to_string());
        match crate::overlay::screen_record::run_gt_narration_test_cli(&input_wav, &target_language)
        {
            Ok(()) => std::process::exit(0),
            Err(error) => {
                eprintln!("[gt-test] ERROR: {error}");
                std::process::exit(1);
            }
        }
    }

    // De-risk probe for the Computer Control feature: open a real Gemini Live
    // session, stream one screenshot + a text task, log tool calls / usage, exit.
    if startup_args
        .iter()
        .any(|arg| arg == "--computer-control-probe")
    {
        let task = parse_arg_value(&startup_args, "--cc-task").unwrap_or_else(|| {
            "Look at the screen and describe what you see, then call done.".to_string()
        });
        let tasks = match parse_arg_value(&startup_args, "--cc-turns-json") {
            Some(raw) => match serde_json::from_str::<Vec<String>>(&raw) {
                Ok(tasks)
                    if !tasks.is_empty() && tasks.iter().all(|task| !task.trim().is_empty()) =>
                {
                    tasks
                }
                Ok(_) => {
                    eprintln!(
                        "[cc-probe] ERROR: --cc-turns-json must contain non-empty task strings"
                    );
                    std::process::exit(2);
                }
                Err(error) => {
                    eprintln!("[cc-probe] ERROR: invalid --cc-turns-json: {error}");
                    std::process::exit(2);
                }
            },
            None => vec![task],
        };
        match crate::overlay::computer_control::run_probe_cli(&tasks) {
            Ok(()) => std::process::exit(0),
            Err(error) => {
                eprintln!("[cc-probe] ERROR: {error}");
                std::process::exit(1);
            }
        }
    }

    // Headless Computer Control session (mic + screen + execute, stderr logs, no GUI).
    if startup_args
        .iter()
        .any(|arg| arg == "--computer-control-run")
    {
        let scripted_turns = parse_arg_value(&startup_args, "--cc-turns-json").map(|raw| {
            serde_json::from_str::<Vec<String>>(&raw).unwrap_or_else(|error| {
                eprintln!("[cc-runtime] ERROR: invalid --cc-turns-json: {error}");
                std::process::exit(2);
            })
        });
        if scripted_turns.as_ref().is_some_and(|turns| {
            turns.is_empty() || turns.iter().any(|turn| turn.trim().is_empty())
        }) {
            eprintln!("[cc-runtime] ERROR: scripted turns must be non-empty strings");
            std::process::exit(2);
        }
        match crate::overlay::computer_control::run_headless(scripted_turns) {
            Ok(()) => std::process::exit(0),
            Err(error) => {
                eprintln!("[cc-runtime] ERROR: {error}");
                std::process::exit(1);
            }
        }
    }

    // Coordinate-convention debug harness.
    if startup_args.iter().any(|arg| arg == "--cc-coord-test") {
        match crate::overlay::computer_control::run_coord_test_cli() {
            Ok(()) => std::process::exit(0),
            Err(e) => {
                eprintln!("[coord] ERROR: {e}");
                std::process::exit(1);
            }
        }
    }

    // UIA ground-truth element dump.
    if startup_args.iter().any(|arg| arg == "--cc-uia-dump") {
        let target = parse_arg_value(&startup_args, "--cc-window")
            .or_else(|| std::env::var("CC_UIA_WINDOW").ok());
        match crate::overlay::computer_control::run_uia_dump_cli(target.as_deref()) {
            Ok(()) => std::process::exit(0),
            Err(e) => {
                eprintln!("[uia] ERROR: {e}");
                std::process::exit(1);
            }
        }
    }

    // Aux vision-stack smoke test: read the foreground window, print the answer.
    if startup_args.iter().any(|arg| arg == "--cc-vision-test") {
        let target = parse_arg_value(&startup_args, "--cc-window")
            .or_else(|| std::env::var("CC_UIA_WINDOW").ok());
        let question = parse_arg_value(&startup_args, "--cc-task").unwrap_or_else(|| {
            "In one sentence, what application and content is shown?".to_string()
        });
        match crate::overlay::computer_control::run_vision_test_cli(target.as_deref(), &question) {
            Ok(()) => std::process::exit(0),
            Err(e) => {
                eprintln!("[vision-test] ERROR: {e}");
                std::process::exit(1);
            }
        }
    }

    // Model-free human-cursor demo: glide the cursor on a tour of the screen.
    if startup_args.iter().any(|arg| arg == "--cc-cursor-demo") {
        crate::overlay::computer_control::run_cursor_demo_cli();
        std::process::exit(0);
    }

    // Grid-overlay legibility check: capture one frame + Set-of-Mark grid, save it.
    if startup_args.iter().any(|arg| arg == "--cc-grid-test") {
        let target = parse_arg_value(&startup_args, "--cc-window")
            .or_else(|| std::env::var("CC_UIA_WINDOW").ok());
        match crate::overlay::computer_control::run_grid_test_cli(target.as_deref()) {
            Ok(()) => std::process::exit(0),
            Err(e) => {
                eprintln!("[grid-test] ERROR: {e}");
                std::process::exit(1);
            }
        }
    }

    // UIA-grounded task workhorse (element-list grounding + per-step screenshots).
    if startup_args.iter().any(|arg| arg == "--cc-uia-task") {
        let task = parse_arg_value(&startup_args, "--cc-task")
            .unwrap_or_else(|| "Describe the focused window, then call done.".to_string());
        match crate::overlay::computer_control::run_uia_task_cli(&task) {
            Ok(()) => std::process::exit(0),
            Err(e) => {
                eprintln!("[uia-task] ERROR: {e}");
                std::process::exit(1);
            }
        }
    }

    // MCP stdio bridge smoke test (no Gemini): spawn a catalog server, list + health probe.
    if startup_args.iter().any(|arg| arg == "--cc-mcp-test") {
        let id =
            parse_arg_value(&startup_args, "--cc-mcp-test").unwrap_or_else(|| "time".to_string());
        let tool = parse_arg_value(&startup_args, "--cc-mcp-tool");
        let args_json = parse_arg_value(&startup_args, "--cc-mcp-args-json");
        let list_only = startup_args.iter().any(|arg| arg == "--cc-mcp-list-only");
        match crate::overlay::computer_control::run_mcp_test_cli(
            &id,
            tool.as_deref(),
            args_json.as_deref(),
            list_only,
        ) {
            Ok(()) => std::process::exit(0),
            Err(e) => {
                eprintln!("[mcp-test] ERROR: {e}");
                std::process::exit(1);
            }
        }
    }

    // Typed system-query smoke test (no Gemini): print structured OS facts.
    if startup_args
        .iter()
        .any(|arg| arg == "--cc-system-query-test")
    {
        let spec = parse_arg_value(&startup_args, "--cc-system-query-test")
            .unwrap_or_else(|| "capabilities.list".to_string());
        let args_json = parse_arg_value(&startup_args, "--cc-system-query-args-json");
        match crate::overlay::computer_control::run_system_query_test_cli(
            &spec,
            args_json.as_deref(),
        ) {
            Ok(()) => std::process::exit(0),
            Err(e) => {
                eprintln!("[system-query-test] ERROR: {e}");
                std::process::exit(1);
            }
        }
    }

    // Task-trace harness (multi-step task + per-step screenshots).
    if startup_args.iter().any(|arg| arg == "--cc-task-trace") {
        let task = parse_arg_value(&startup_args, "--cc-task")
            .unwrap_or_else(|| "Open the Windows Start menu, then call done.".to_string());
        match crate::overlay::computer_control::run_task_trace_cli(&task) {
            Ok(()) => std::process::exit(0),
            Err(e) => {
                eprintln!("[trace] ERROR: {e}");
                std::process::exit(1);
            }
        }
    }

    if let Some(exit_code) = maybe_run_headless_export_replay(&startup_args) {
        std::process::exit(exit_code);
    }
    let screen_record_wry_smoke = configure_screen_record_wry_smoke(&startup_args);
    startup_launch::maybe_delay_for_windows_autostart(&startup_args);

    // Cleanup temp files
    initialization::cleanup_temporary_files();

    // Auto-install WebView2 Runtime in the background if it's missing.
    // Every web-based overlay needs it; runs in a background thread so the
    // GUI can start using fallback native menus while downloading. The install
    // function auto-restarts the app on success so the fresh runtime is used.
    if !runtime_support::webview2_runtime_installed() {
        crate::log_info!("[WebView2] Runtime not detected — starting auto-install in background.");
        runtime_support::start_webview2_runtime_install();
    }

    // Ensure context menu entry
    crate::log_info!("Ensuring context menu entry...");
    registry_integration::ensure_context_menu_entry();
    crate::log_info!("Context menu entry ensured.");

    // Initialize COM and DPI
    initialization::init_com_and_dpi();

    // Enable dark mode for native menus
    initialization::enable_dark_mode_for_app();

    // Apply pending updates
    initialization::apply_pending_updates();

    // Ensure the named event exists
    let _ = RESTORE_EVENT.as_ref();

    // Single instance check
    let _single_instance_mutex = unsafe {
        let mutex_name = hotkey::single_instance_mutex_name_wide();
        let instance = CreateMutexW(None, true, PCWSTR(mutex_name.as_ptr()));
        if screen_record_wry_smoke {
            instance.ok()
        } else if let Ok(handle) = instance {
            if GetLastError() == ERROR_ALREADY_EXISTS {
                // Another instance is running - pass arguments via temp file and signal it
                let args: Vec<String> = std::env::args().collect();
                if let Some(path) = find_process_with_sgt_file_arg(&args) {
                    let temp_file = std::env::temp_dir().join("sgt_pending_file.txt");
                    if let Ok(mut f) = std::fs::File::create(temp_file) {
                        use std::io::Write;
                        let _ = write!(f, "{}", path.to_string_lossy());
                    }
                }

                if let Some(event) = RESTORE_EVENT.as_ref() {
                    let _ = SetEvent(event.0);
                }
                let _ = CloseHandle(handle);
                return Ok(());
            }
            Some(handle)
        } else {
            None
        }
    };

    // Start hotkey listener thread
    if !screen_record_wry_smoke {
        std::thread::spawn(|| {
            hotkey::run_hotkey_listener();
        });
    }

    // Initialize TTS
    api::tts::init_tts();

    // Initialize Gemini Live connection pool
    api::gemini_live::init_gemini_live();

    // Check for --restarted flag and file arguments
    let args: Vec<String> = std::env::args().collect();
    let pending_file_path = find_process_with_sgt_file_arg(&args);

    if args.iter().any(|arg| arg == "--restarted") {
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(2500));
            overlay::auto_copy_badge::show_update_notification(
                "Đã khởi động lại app để khôi phục hoàn toàn",
            );
        });
    }

    if let Some(path) = &pending_file_path {
        crate::log_info!(
            "Check arguments: Found Process with SGT file path: {:?}",
            path
        );
    } else if args.iter().any(|arg| arg == PROCESS_WITH_SGT_FLAG) {
        crate::log_info!("Check arguments: Process with SGT flag present but no valid file path");
    }

    // Clear WebView data if scheduled
    {
        let mut config = APP.lock().unwrap();
        if config.config.clear_webview_on_startup {
            overlay::clear_webview_permissions();
            config.config.clear_webview_on_startup = false;
            config::save_config(&config.config);
        }
    }

    // Spawn warmup thread
    initialization::spawn_warmup_thread();
    runtime_support::show_startup_compatibility_notice_if_needed();

    run_settings_window(screen_record_wry_smoke, pending_file_path)
}

// Re-export hotkey functions for external access
pub use hotkey::{register_all_hotkeys, unregister_all_hotkeys};
