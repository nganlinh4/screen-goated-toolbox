use std::fs;
use std::path::{Path, PathBuf};

fn source_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn read_source(relative: &str) -> String {
    fs::read_to_string(source_path(relative))
        .unwrap_or_else(|error| panic!("failed to read {relative}: {error}"))
}

fn assert_markers_in_order(source: &str, relative: &str, markers: &[&str]) {
    let mut cursor = 0;
    for marker in markers {
        let offset = source[cursor..]
            .find(marker)
            .unwrap_or_else(|| panic!("{relative} is missing startup-order marker `{marker}`"));
        cursor += offset + marker.len();
    }
}

#[test]
fn desktop_startup_phases_remain_in_dependency_order() {
    let source = read_source("src/app_entry.rs");
    assert_markers_in_order(
        &source,
        "src/app_entry.rs",
        &[
            "setup_console_utf8",
            "headless::run_pre_boot",
            "setup_crash_handler",
            "headless::run_after_bootstrap",
            "configure_screen_record_wry_smoke",
            "configure_creation_ui_test",
            "single_instance::acquire",
            "app_activation::start_listener",
            "maybe_delay_for_windows_autostart",
            "resume_pending_removals",
            "start_webview2_runtime_install",
            "init_com_and_dpi",
            "run_hotkey_listener",
            "init_tts",
            "init_gemini_live",
            "clear_webview_permissions",
            "settings_window::run",
        ],
    );
}

#[test]
fn desktop_startup_leaves_optional_status_overlays_on_demand() {
    let source = read_source("src/app_entry.rs");
    let gui_startup = read_source("src/gui/app/logic.rs");
    let screen_translate = read_source("src/overlay/screen_translate/mod.rs");
    let footer = read_source("src/gui/app/rendering/footer.rs");
    assert!(!source.contains("spawn_warmup_thread"));
    assert!(!source.contains("warm_up_orb"));
    assert!(source.contains("result::scene_compositor::warmup"));
    assert!(!source.contains("status_compositor::warmup"));
    assert!(!gui_startup.contains("screen_translate::prepare"));
    assert!(screen_translate.contains("prepare: prepare_detector"));
    assert!(footer.contains("screen_translate::prepare_detector"));
}

#[test]
fn desktop_startup_does_not_install_or_embed_native_support() {
    let entry = read_source("src/app_entry.rs");
    let native_support = read_source("src/unpack_dlls.rs");

    assert!(!entry.contains("unpack_dlls::unpack_dlls"));
    assert!(!entry.contains("vc_runtime::ensure_component"));
    assert!(!native_support.contains("include_bytes!"));
    assert!(!native_support.contains("SetDllDirectory"));
}

#[test]
fn computer_control_is_absent_from_headless_dispatch() {
    let source = read_source("src/app_entry/headless.rs");
    assert!(source.contains("super::replay::run(args)"));
    assert!(source.contains("computer_control_flags_are_not_headless_entrances"));
    assert!(!source.contains("run_headless"));
    assert!(!source.contains("run_probe_cli"));
}
