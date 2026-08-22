use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const MAX_SOURCE_LINES: usize = 600;

fn manifest_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn rust_sources_below(root: &Path, sources: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(root)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", root.display()))
    {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_sources_below(&path, sources);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            sources.push(path);
        }
    }
}

fn read_source(relative: &str) -> String {
    fs::read_to_string(manifest_path(relative))
        .unwrap_or_else(|error| panic!("failed to read {relative}: {error}"))
}

#[test]
fn every_rust_source_stays_within_the_project_size_limit() {
    let mut sources = Vec::new();
    rust_sources_below(&manifest_path("src"), &mut sources);

    for path in sources {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        let line_count = source.lines().count();
        assert!(
            line_count <= MAX_SOURCE_LINES,
            "{} has {line_count} lines; limit is {MAX_SOURCE_LINES}",
            path.display()
        );
    }
}

#[test]
fn restore_kernel_event_has_one_wait_owner() {
    let activation = read_source("src/app_activation.rs");
    assert_eq!(activation.matches("WaitForSingleObject(").count(), 1);

    for relative in ["src/hotkey/mod.rs", "src/gui/app/init.rs"] {
        let source = read_source(relative);
        assert!(
            !source.contains("WaitForSingleObject("),
            "{relative} must not consume the restore kernel event"
        );
        assert!(
            !source.contains("ResetEvent("),
            "{relative} must not reset the restore kernel event"
        );
    }
}

#[test]
fn main_window_resize_has_one_native_owner() {
    let app = read_source("src/gui/app.rs");
    let app_utils = read_source("src/gui/app/utils.rs");
    let overlays = read_source("src/gui/app/rendering/overlays.rs");
    let native_resize = read_source("src/gui/resize_subclass.rs");

    assert!(!app.contains("render_window_resize_handles"));
    assert!(app.contains("visuals.panel_fill"));
    assert!(app_utils.contains("ViewportCommand::Transparent(false)"));
    assert!(!app_utils.contains("ViewportCommand::Transparent(true)"));
    assert!(!overlays.contains("ViewportCommand::BeginResize"));
    assert!(native_resize.contains("WM_NCHITTEST"));
}

#[test]
fn egui_modals_use_the_shared_material_surface() {
    let gui_root = manifest_path("src/gui");
    let shared_surface = gui_root.join("widgets.rs");
    let mut sources = Vec::new();
    rust_sources_below(&gui_root, &mut sources);

    for path in sources {
        if path == shared_surface {
            continue;
        }
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        assert!(
            !source.contains("egui::Modal::new"),
            "{} bypasses gui::widgets::material_modal",
            path.display()
        );
    }
}

#[test]
fn ignored_egui_checkouts_exactly_match_tracked_patches() {
    let validator = manifest_path("scripts/validate-egui-patches.ps1");
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&validator)
        .output()
        .unwrap_or_else(|error| panic!("failed to run {}: {error}", validator.display()));

    assert!(
        output.status.success(),
        "{} failed:\n{}{}",
        validator.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn graph_wheel_zoom_yields_to_open_popups() {
    let patch = read_source("scripts/egui-snarl-scroll-zoom.patch");

    assert!(patch.contains("let popup_open = egui::Popup::is_any_open(ui.ctx());"));
    assert!(
        patch.matches("pointer_in_canvas && !popup_open").count() >= 2,
        "both wheel and native zoom input must yield to graph-node dropdowns"
    );
}
