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
            "unpack_dlls::unpack_dlls",
            "headless::run_post_unpack",
            "configure_screen_record_wry_smoke",
            "maybe_delay_for_windows_autostart",
            "start_webview2_runtime_install",
            "init_com_and_dpi",
            "single_instance::acquire",
            "app_activation::start_listener",
            "run_hotkey_listener",
            "init_tts",
            "init_gemini_live",
            "settings_window::run",
        ],
    );
}

#[test]
fn headless_dispatch_keeps_replay_last() {
    let source = read_source("src/app_entry/headless.rs");
    assert_markers_in_order(
        &source,
        "src/app_entry/headless.rs",
        &[
            "--gt-narration-test",
            "--computer-control-probe",
            "--computer-control-run",
            "--cc-coord-test",
            "--cc-uia-dump",
            "--cc-vision-test",
            "--cc-detector-test",
            "--cc-cursor-demo",
            "--cc-grid-test",
            "--cc-uia-task",
            "--cc-mcp-test",
            "--cc-system-query-test",
            "--cc-task-trace",
            "super::replay::run(args)",
        ],
    );
}
