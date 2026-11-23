pub mod auto_copy_badge; // Auto-copy notification badge
pub(crate) mod cached_dib;
pub(crate) mod component_removal;
pub(crate) mod composition_anchor;
pub(crate) mod compositor_policy;
pub(crate) mod compositor_process;
pub(crate) mod compositor_renderer;
pub mod computer_control; // "Điều khiển máy tính" — Gemini Live computer-control agent (probe)
pub mod continuous_mode; // Continuous mode for image/text presets (hold-to-activate)
mod creation_close;
mod creation_commands;
pub mod creation_delivery;
pub(crate) mod creation_error;
pub(crate) mod creation_file_dialogs;
mod creation_file_identity;
mod creation_identity;
pub mod creation_intent_journal;
mod creation_lifecycle;
mod creation_output;
mod creation_path;
pub mod creation_preview;
mod creation_preview_protocol;
pub(crate) mod creation_process;
pub mod creation_process_supervisor;
pub mod creation_progress;
pub mod creation_recovery;
pub mod creation_runtime;
pub mod creation_source;
mod creation_source_snapshot;
#[cfg(test)]
mod creation_submission_test;
pub(crate) mod display_placement;
pub mod generation_history;
pub(crate) mod image_capture_target;
pub mod image_continuous_mode; // Non-blocking image selection mode (right-click gestures)
pub mod image_to_svg;
pub mod input_history; // Persistent input history for arrow up/down navigation
pub(crate) mod magnifier;
pub mod preset_wheel;
pub mod process;
pub mod prompt_dj;
pub mod recording;
pub(crate) mod region_handle;
pub mod result;
pub(crate) mod rounded_selection;
pub mod screen_record;
pub mod screen_translate;
pub mod selection; // Made public for extract_crop_from_hbitmap_public
pub(crate) mod shell_policy;
pub(crate) mod status_compositor;
pub(crate) mod store_support;
pub mod text_input; // NEW MODULE
pub mod text_selection;
pub mod three_d_generator;
pub mod translation_gummy;
pub(crate) mod trusted_page;
pub mod tts_playground; // WRY mini-app TTS Playground
pub(crate) mod wake_event;
pub(crate) mod webview_reset;

use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};

static IS_BUSY_WITH_OVERLAY: AtomicBool = AtomicBool::new(false);
#[cfg(debug_assertions)]
static CAPTURE_ACTIVITY: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[cfg(debug_assertions)]
fn mark_capture_activity() {
    let _ = CAPTURE_ACTIVITY.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |value| {
        Some(value.saturating_add(1))
    });
}

#[cfg(debug_assertions)]
pub(crate) fn capture_activity() -> u64 {
    CAPTURE_ACTIVITY.load(Ordering::SeqCst)
}

/// Canonical outer radius for selection, processing, and result boxes.
pub(crate) const BOX_CORNER_RADIUS_PHYSICAL_PX: f32 = 8.0;

pub fn is_busy() -> bool {
    IS_BUSY_WITH_OVERLAY.load(Ordering::SeqCst)
}

pub fn set_is_busy(busy: bool) {
    IS_BUSY_WITH_OVERLAY.store(busy, Ordering::SeqCst);
    #[cfg(debug_assertions)]
    if busy {
        mark_capture_activity();
    }
}

pub(crate) fn try_claim_capture() -> bool {
    if selection::is_selection_overlay_active() || image_continuous_mode::is_active() {
        return false;
    }
    try_claim_overlay()
}

// Internal lab operations use the same exclusion without recording a new
// user capture intent. Their ownership is tracked separately by the lab.
#[cfg(debug_assertions)]
pub(crate) fn try_claim_internal_capture() -> bool {
    if selection::is_selection_overlay_active() || image_continuous_mode::is_active() {
        return false;
    }
    IS_BUSY_WITH_OVERLAY
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
}

pub(crate) fn try_claim_overlay() -> bool {
    let claimed = IS_BUSY_WITH_OVERLAY
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok();
    #[cfg(debug_assertions)]
    if claimed {
        mark_capture_activity();
    }
    claimed
}

pub mod utils; // MASTER preset wheel
// realtime_overlay module removed (was old GDI-based, now using realtime_webview)
pub mod favorite_bubble; // Floating bubble for favorite presets
pub mod html_components; // Split HTML components (CSS/JS)
pub mod realtime_html; // HTML generation for realtime overlay
pub mod realtime_webview; // New WebView2-based with smooth scrolling
pub mod tray_popup; // Custom non-blocking tray popup menu
pub(crate) mod webview_diagnostics;
pub(crate) mod webview_init;
pub(crate) mod webview_runtime;
pub mod window_selector;
pub(crate) mod window_slot;

pub use recording::{
    is_recording_overlay_active, show_recording_overlay, stop_recording_and_submit,
};
pub use selection::is_selection_overlay_active;
pub(crate) use selection::show_image_capture_overlay;
pub use text_selection::show_text_selection_tag;
// Use the new WebView2-based realtime overlay
/// Mutex to ensure only one WebView is being initialized at a time globally.
/// This prevents concurrent on-demand builds from exhausting WebView2 resources.
pub static GLOBAL_WEBVIEW_MUTEX: LazyLock<std::sync::Mutex<()>> =
    LazyLock::new(|| std::sync::Mutex::new(()));

// pub use crate::api::realtime_audio::show_realtime_overlay; // REMOVED - Incorrect path
pub use realtime_webview::{
    is_realtime_overlay_active, show_realtime_overlay, stop_realtime_overlay,
};

fn delete_directory_contents_recursive(path: &std::path::Path) -> bool {
    let mut any_deleted = false;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let entry_path = entry.path();
            if entry_path.is_dir() {
                delete_directory_contents_recursive(&entry_path);
                if std::fs::remove_dir(&entry_path).is_ok() {
                    any_deleted = true;
                }
            } else if std::fs::remove_file(&entry_path).is_ok() {
                any_deleted = true;
            }
        }
    }
    any_deleted
}

/// Factory-reset cleanup: deletes app-managed dirs that have no per-tool UI.
///
/// Skipped on purpose because they already have dedicated delete buttons in
/// `settings_ui/global/downloaded_tools/`:
///   - `bin/` (ai_runtime, video_downloader, language detection assets)
///   - `pointer-gallery/` (pointer_packs)
///   - `backgrounds/` (backgrounds)
///   - `Roaming/screen-goated-toolbox/models/` (model_sections, legacy_offline)
///
/// WebView data under `SGT/webview_data` is skipped here because it is still
/// locked by the running process; it is cleaned on next startup via
/// `clear_webview_on_startup` instead.
pub fn clear_all_app_data() {
    let isolated = crate::paths::runtime_state_root().is_some();
    let legacy_sgt_roaming = (!isolated).then(|| {
        dirs::config_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("SGT")
    });
    let roots = ResetRoots {
        local: crate::paths::app_runtime_local_data_dir(),
        roaming: crate::paths::app_runtime_config_dir(),
        legacy_roaming: legacy_sgt_roaming,
        legacy_local: crate::paths::app_sgt_dir(),
    };
    computer_control::with_telemetry_paused(|| clear_reset_data(&roots));
}

struct ResetRoots {
    local: std::path::PathBuf,
    roaming: std::path::PathBuf,
    legacy_roaming: Option<std::path::PathBuf>,
    legacy_local: std::path::PathBuf,
}

fn clear_reset_data(roots: &ResetRoots) {
    // Local\screen-goated-toolbox\* — caches and user content with no UI.
    let local_children = [
        "recordings",
        "composition-snapshots",
        "cursor-anim-cache",
        "webview-selector",
        "export-debug",
        "composition-export",
    ];
    for name in local_children {
        let path = roots.local.join(name);
        if path.exists()
            && let Err(e) = std::fs::remove_dir_all(&path)
        {
            eprintln!("[reset] failed to remove {:?}: {}", path, e);
            delete_directory_contents_recursive(&path);
        }
    }

    clear_roaming_reset_data(&roots.roaming);

    // Legacy Roaming\SGT (orphaned from an old code version — pure garbage).
    if let Some(legacy_sgt_roaming) = &roots.legacy_roaming
        && legacy_sgt_roaming.exists()
        && let Err(e) = std::fs::remove_dir_all(legacy_sgt_roaming)
    {
        eprintln!(
            "[reset] failed to remove legacy {:?}: {}",
            legacy_sgt_roaming, e
        );
        delete_directory_contents_recursive(legacy_sgt_roaming);
    }

    // Local\SGT\crashes — finished crash reports, which can hold memory and
    // screen text; plus legacy full-memory dumps under the local root.
    if let Err(error) =
        crate::crash_report::clear_for_reset(&roots.legacy_local.join("crashes"), &roots.local)
    {
        eprintln!("[reset] failed to remove crash reports: {error:#}");
    }

    // Local\SGT\logs — debug session logs (debug_log.rs).
    // Local\SGT\bin — orphaned unpacked runtime DLLs from an old code version
    //                 (current code unpacks to screen-goated-toolbox\bin\).
    // Local\SGT\webview_data is intentionally skipped here — still locked by
    // the running process, cleaned on next startup.
    for name in ["logs", "bin"] {
        let path = roots.legacy_local.join(name);
        if path.exists()
            && let Err(e) = std::fs::remove_dir_all(&path)
        {
            eprintln!("[reset] failed to remove {:?}: {}", path, e);
            delete_directory_contents_recursive(&path);
        }
    }
}

/// Clear history and cache entries in the roaming root while deliberately
/// leaving config, downloaded models, and other user-managed state untouched.
fn clear_roaming_reset_data(sgt_roaming: &std::path::Path) {
    // history_media — transcript audio clips.
    // history.json — matching history database.
    // fonts — app-bundled font cache.
    for name in ["history_media", "fonts"] {
        let path = sgt_roaming.join(name);
        if path.exists()
            && let Err(e) = std::fs::remove_dir_all(&path)
        {
            eprintln!("[reset] failed to remove {:?}: {}", path, e);
            delete_directory_contents_recursive(&path);
        }
    }
    for name in ["history.json", "history.json.tmp"] {
        let path = sgt_roaming.join(name);
        if path.exists()
            && let Err(e) = std::fs::remove_file(&path)
        {
            eprintln!("[reset] failed to remove {:?}: {}", path, e);
        }
    }
}

/// Check if we should use dark mode based on config.
/// Uses direct registry check for System theme to avoid crate overhead/crashes.
pub fn is_dark_mode() -> bool {
    let mode = {
        if let Ok(app) = crate::APP.lock() {
            app.config.theme_mode.clone()
        } else {
            crate::config::ThemeMode::Dark
        }
    };

    mode.is_dark()
}

#[cfg(test)]
mod reset_cleanup_tests {
    use super::{ResetRoots, clear_reset_data, clear_roaming_reset_data};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn roaming_cleanup_keeps_config_and_downloaded_models() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "sgt-restore-defaults-test-{}-{unique}",
            std::process::id()
        ));
        let history_media = root.join("history_media");
        let fonts = root.join("fonts");
        let models = root.join("models");
        fs::create_dir_all(&history_media).unwrap();
        fs::create_dir_all(&fonts).unwrap();
        fs::create_dir_all(&models).unwrap();
        fs::write(history_media.join("clip.wav"), b"history").unwrap();
        fs::write(fonts.join("font.ttf"), b"font").unwrap();
        fs::write(root.join("history.json"), b"[]").unwrap();
        fs::write(root.join("history.json.tmp"), b"[]").unwrap();
        fs::write(root.join("config_v3.json"), b"{}").unwrap();
        fs::write(models.join("weights.bin"), b"model").unwrap();

        clear_roaming_reset_data(&root);

        assert!(!history_media.exists());
        assert!(!fonts.exists());
        assert!(!root.join("history.json").exists());
        assert!(!root.join("history.json.tmp").exists());
        assert!(root.join("config_v3.json").exists());
        assert!(models.join("weights.bin").exists());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn isolated_cleanup_only_deletes_its_owned_roots() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "sgt-isolated-reset-test-{}-{unique}",
            std::process::id()
        ));
        let production = root.join("production");
        let isolated = root.join("isolated");
        fs::create_dir_all(production.join("recordings")).unwrap();
        fs::write(
            production.join("recordings").join("keep.mp4"),
            b"user recording",
        )
        .unwrap();
        fs::create_dir_all(isolated.join("local").join("recordings")).unwrap();
        fs::write(
            isolated.join("local").join("recordings").join("test.mp4"),
            b"test recording",
        )
        .unwrap();
        fs::create_dir_all(isolated.join("roaming").join("history_media")).unwrap();
        fs::write(isolated.join("roaming").join("history.json"), b"[]").unwrap();
        fs::create_dir_all(isolated.join("legacy-local").join("logs")).unwrap();
        let crash_report = isolated.join("legacy-local/crashes/bootstrap-0");
        fs::create_dir_all(&crash_report).unwrap();
        fs::write(crash_report.join("events.log"), b"report").unwrap();
        let legacy_dumps = isolated.join("local").join("crash-dumps");
        fs::create_dir_all(&legacy_dumps).unwrap();
        fs::write(legacy_dumps.join("old.dmp"), b"dump").unwrap();
        clear_reset_data(&ResetRoots {
            local: isolated.join("local"),
            roaming: isolated.join("roaming"),
            legacy_roaming: None,
            legacy_local: isolated.join("legacy-local"),
        });
        assert!(production.join("recordings").join("keep.mp4").exists());
        assert!(!isolated.join("local").join("recordings").exists());
        assert!(!isolated.join("roaming").join("history.json").exists());
        assert!(!isolated.join("legacy-local").join("logs").exists());
        assert!(!crash_report.exists());
        assert!(!legacy_dumps.exists());
        let absolute_root = fs::canonicalize(&root).unwrap();
        assert!(absolute_root.starts_with(fs::canonicalize(std::env::temp_dir()).unwrap()));
        fs::remove_dir_all(absolute_root).unwrap();
    }
}
