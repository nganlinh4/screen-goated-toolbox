mod arguments;
mod headless;
mod replay;
mod settings_window;
mod single_instance;

#[cfg(test)]
mod tests;

use arguments::StartupArgs;
use single_instance::InstanceOutcome;

pub(crate) fn run() -> eframe::Result<()> {
    if crate::overlay::realtime_webview::is_child_process() {
        if let Err(error) = crate::overlay::realtime_webview::run_child() {
            eprintln!("realtime compositor failed: {error:#}");
            std::process::exit(1);
        }
        return Ok(());
    }
    if crate::overlay::status_compositor::is_child_process() {
        if let Err(error) = crate::overlay::status_compositor::run_child() {
            eprintln!("status compositor failed: {error:#}");
            std::process::exit(1);
        }
        return Ok(());
    }
    if crate::overlay::result::scene_compositor::is_child_process() {
        if let Err(error) = crate::overlay::result::scene_compositor::run_child() {
            eprintln!("result compositor failed: {error:#}");
            std::process::exit(1);
        }
        return Ok(());
    }
    crate::component_registry::embedded_catalog();

    if crate::initialization::setup_console_utf8() {
        println!("[Console] UTF-8 input/output enabled");
    } else {
        eprintln!("[Console] WARNING: failed to enable UTF-8 input/output");
    }

    let startup_args = StartupArgs::collect();
    if let Some(exit_code) = headless::run_pre_boot(&startup_args) {
        std::process::exit(exit_code);
    }

    crate::log_info!("========================================");
    crate::log_info!(
        "Screen Goated Toolbox v{} STARTUP",
        env!("CARGO_PKG_VERSION")
    );
    crate::log_info!("========================================");

    // Install panic reporting before any substantial startup work so early failures
    // do not exit silently on Windows release builds.
    let crash_report_mode = if headless::is_requested(&startup_args) {
        crate::initialization::CrashReportMode::NonInteractive
    } else {
        crate::initialization::CrashReportMode::Interactive
    };
    crate::initialization::setup_crash_handler(crash_report_mode);

    if let Some(exit_code) = headless::run_after_bootstrap(&startup_args) {
        std::process::exit(exit_code);
    }

    let screen_record_wry_smoke = startup_args.configure_screen_record_wry_smoke();
    let creation_ui_test = startup_args.configure_creation_ui_test();
    let screen_translate_ui_test = startup_args.screen_translate_ui_test();
    let screen_translate_ui_test_image = startup_args.screen_translate_ui_test_image();
    let screen_translate_lab_queue = startup_args.screen_translate_lab_queue();
    let result_compositor_smoke = startup_args.result_compositor_smoke();
    let status_compositor_smoke = startup_args.status_compositor_smoke();
    let realtime_compositor_smoke = startup_args.realtime_compositor_smoke();
    let isolated_ui_test = screen_record_wry_smoke
        || creation_ui_test.is_some()
        || screen_translate_ui_test
        || screen_translate_lab_queue.is_some()
        || result_compositor_smoke
        || status_compositor_smoke
        || realtime_compositor_smoke;

    let _ = crate::RESTORE_EVENT.as_ref();
    // Establish process ownership before cleanup, installation, registry edits,
    // or update application. Component operations also hold their own named
    // mutation mutex for headless/test processes that intentionally bypass the
    // desktop singleton.
    let primary_instance = match single_instance::acquire(&startup_args, isolated_ui_test) {
        InstanceOutcome::Primary(instance) => instance,
        InstanceOutcome::SecondaryNotified => return Ok(()),
    };
    let _single_instance_mutex = primary_instance.guard;
    if primary_instance.owns_activation {
        crate::app_activation::start_listener();
    }

    crate::startup_launch::maybe_delay_for_windows_autostart(startup_args.raw());

    crate::initialization::cleanup_temporary_files();
    if let Err(error) = crate::component_registry::resume_pending_removals() {
        crate::log_info!("[Components] Pending removal maintenance failed: {error}");
    }
    if let Err(error) = crate::component_registry::external_tools::reconcile_interrupted_installs()
    {
        crate::log_info!("[Components] External-tool staging maintenance failed: {error}");
    }
    crate::component_registry::update_catalog::refresh_in_background();

    let webview2_ready = crate::runtime_support::webview2_runtime_installed();
    if !webview2_ready {
        crate::log_info!("[WebView2] Runtime not detected — starting auto-install in background.");
        crate::runtime_support::start_webview2_runtime_install();
    } else {
        // Begin the only result renderer as early as possible and overlap its
        // WebView/font bootstrap with the rest of application startup.
        crate::overlay::result::scene_compositor::warmup();
        crate::overlay::status_compositor::warmup();
    }

    crate::log_info!("Ensuring context menu entry...");
    crate::registry_integration::ensure_context_menu_entry();
    crate::log_info!("Context menu entry ensured.");

    crate::initialization::init_com_and_dpi();
    crate::initialization::enable_dark_mode_for_app();
    crate::initialization::apply_pending_updates();

    if !isolated_ui_test {
        std::thread::spawn(crate::hotkey::run_hotkey_listener);
    }

    crate::api::tts::init_tts();
    crate::api::gemini_live::init_gemini_live();

    let pending_file_path = startup_args.process_with_sgt_file();

    if startup_args.has("--restarted") {
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(2500));
            let badge = crate::overlay::auto_copy_badge::locale_text();
            crate::overlay::auto_copy_badge::show_update_notification(
                badge.app_restarted_after_recovery,
            );
        });
    }

    if let Some(path) = &pending_file_path {
        crate::log_info!(
            "Check arguments: Found Process with SGT file path: {:?}",
            path
        );
    } else if startup_args.has(arguments::PROCESS_WITH_SGT_FLAG) {
        crate::log_info!("Check arguments: Process with SGT flag present but no valid file path");
    }

    {
        let mut app = crate::APP.lock().unwrap();
        if app.config.clear_webview_on_startup {
            crate::overlay::clear_webview_permissions();
            app.config.clear_webview_on_startup = false;
            crate::config::save_config(&app.config);
        }
    }

    if result_compositor_smoke {
        std::process::exit(crate::overlay::result::smoke::run());
    }
    if status_compositor_smoke {
        std::process::exit(crate::overlay::status_compositor::smoke::run());
    }
    if realtime_compositor_smoke {
        std::process::exit(crate::overlay::realtime_webview::smoke::run());
    }

    settings_window::run(
        screen_record_wry_smoke,
        creation_ui_test,
        screen_translate_ui_test,
        screen_translate_ui_test_image,
        screen_translate_lab_queue,
        pending_file_path,
    )
}
