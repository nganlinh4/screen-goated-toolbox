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
    crate::initialization::setup_crash_handler();

    crate::unpack_dlls::unpack_dlls();

    if let Some(exit_code) = headless::run_post_unpack(&startup_args) {
        std::process::exit(exit_code);
    }

    let screen_record_wry_smoke = startup_args.configure_screen_record_wry_smoke();
    crate::startup_launch::maybe_delay_for_windows_autostart(startup_args.raw());

    crate::initialization::cleanup_temporary_files();

    // Preserve the existing ordering: installer startup happens before the
    // single-instance decision and may spawn its background worker.
    if !crate::runtime_support::webview2_runtime_installed() {
        crate::log_info!("[WebView2] Runtime not detected — starting auto-install in background.");
        crate::runtime_support::start_webview2_runtime_install();
    }

    crate::log_info!("Ensuring context menu entry...");
    crate::registry_integration::ensure_context_menu_entry();
    crate::log_info!("Context menu entry ensured.");

    crate::initialization::init_com_and_dpi();
    crate::initialization::enable_dark_mode_for_app();
    crate::initialization::apply_pending_updates();

    let _ = crate::RESTORE_EVENT.as_ref();

    // The handle must remain alive through the full eframe loop. Moving this
    // guard inside `single_instance::acquire` would silently disable enforcement.
    let primary_instance = match single_instance::acquire(&startup_args, screen_record_wry_smoke) {
        InstanceOutcome::Primary(instance) => instance,
        InstanceOutcome::SecondaryNotified => return Ok(()),
    };
    let _single_instance_mutex = primary_instance.guard;

    if primary_instance.owns_activation {
        crate::app_activation::start_listener();
    }

    if !screen_record_wry_smoke {
        std::thread::spawn(crate::hotkey::run_hotkey_listener);
    }

    crate::api::tts::init_tts();
    crate::api::gemini_live::init_gemini_live();

    let pending_file_path = startup_args.process_with_sgt_file();

    if startup_args.has("--restarted") {
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(2500));
            crate::overlay::auto_copy_badge::show_update_notification(
                "Đã khởi động lại app để khôi phục hoàn toàn",
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

    crate::initialization::spawn_warmup_thread();
    crate::runtime_support::show_startup_compatibility_notice_if_needed();

    settings_window::run(screen_record_wry_smoke, pending_file_path)
}
