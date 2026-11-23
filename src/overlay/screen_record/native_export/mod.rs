pub mod anim_cache;
mod audio_download;
mod audio_mix;
mod background_presets;
mod camera_path;
mod capabilities;
mod capture_dimensions;
mod clip_audio_slice;
mod composition;
pub mod config;
mod crop_geometry;
mod cursor;
mod cursor_path;
mod gif;
pub(crate) mod native_stitch;
mod overlay;
pub mod overlay_frames;
mod pipeline;
mod pipeline_build;
mod progress;
pub mod sampling;
pub mod staging;
mod util;
pub(crate) mod validation;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};

use super::SR_HWND;
use super::gpu_export;
use super::gpu_pipeline;
use super::mf_decode;
use super::mf_encode;

pub use audio_download::start_audio_download;
pub use composition::start_composition_export;
pub(crate) use pipeline::run_native_export_with_staged;
pub use pipeline::start_native_export;
pub use progress::persist_export_result;

pub fn prewarm_custom_background(url: &str) -> Result<(), String> {
    overlay::prewarm_custom_background(url)
}

/// Abort signal shared by every export stage and helper process. A user cancel
/// sets it, and so does any internal stage failure so sibling stages unwind.
static EXPORT_CANCELLED: AtomicBool = AtomicBool::new(false);
/// Set only by [`cancel_export`]. An unwound export is reported as cancelled
/// only when this is set; otherwise its own error is surfaced.
static EXPORT_USER_CANCELLED: AtomicBool = AtomicBool::new(false);
/// Indicates an export is actively running.
static EXPORT_ACTIVE: AtomicBool = AtomicBool::new(false);

static EXPORT_LEASE: Mutex<Weak<ExportLease>> = Mutex::new(Weak::new());
struct ExportLease;
impl Drop for ExportLease {
    fn drop(&mut self) {
        EXPORT_ACTIVE.store(false, Ordering::SeqCst);
    }
}
struct ExportActiveGuard {
    _lease: Arc<ExportLease>,
}

impl ExportActiveGuard {
    fn activate() -> Result<Self, String> {
        let mut current = EXPORT_LEASE
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        EXPORT_ACTIVE
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| "Export already in progress".to_string())?;
        EXPORT_CANCELLED.store(false, Ordering::SeqCst);
        EXPORT_USER_CANCELLED.store(false, Ordering::SeqCst);
        let lease = Arc::new(ExportLease);
        *current = Arc::downgrade(&lease);
        Ok(Self { _lease: lease })
    }
}

fn export_lease() -> Option<Arc<ExportLease>> {
    EXPORT_LEASE
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .upgrade()
}

fn run_ffmpeg(
    command: &crate::process_job::HelperCommand,
    resources: impl Send + 'static,
) -> Result<std::process::Output, String> {
    crate::process_output::run(
        command,
        (resources, export_lease()),
        || EXPORT_CANCELLED.load(Ordering::SeqCst),
        std::time::Duration::from_secs(24 * 60 * 60),
    )
    .map_err(|error| format!("Export helper failed: {error:#}"))
}

pub fn cancel_export() {
    let _admission = EXPORT_LEASE
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    crate::log_info!("[Cancel] Setting EXPORT_CANCELLED flag");
    EXPORT_USER_CANCELLED.store(true, Ordering::SeqCst);
    EXPORT_CANCELLED.store(true, Ordering::SeqCst);
    crate::log_info!("[Cancel] Cancellation signaled");
}

fn export_user_cancelled() -> bool {
    EXPORT_USER_CANCELLED.load(Ordering::SeqCst)
}

pub fn get_default_export_dir() -> String {
    dirs::download_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .to_string_lossy()
        .to_string()
}

pub fn pick_export_folder(initial_dir: Option<String>) -> Result<Option<String>, String> {
    util::pick_export_folder(initial_dir)
}

pub fn get_export_capabilities() -> serde_json::Value {
    capabilities::get_export_capabilities()
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;

    #[test]
    fn helper_lease_blocks_replacement_until_retirement_and_preserves_cancel() {
        let export = ExportActiveGuard::activate().unwrap();
        let helper = export_lease().unwrap();
        assert!(!EXPORT_CANCELLED.load(Ordering::SeqCst));
        // A failing stage fans its abort out through the shared flag; that
        // must not be mistaken for the user's cancel.
        EXPORT_CANCELLED.store(true, Ordering::SeqCst);
        assert!(!export_user_cancelled());
        assert!(
            pipeline_build::classify_unwound_export("disk full".into(), export_user_cancelled())
                .is_err()
        );
        cancel_export();
        assert!(EXPORT_CANCELLED.load(Ordering::SeqCst));
        assert!(export_user_cancelled());
        assert_eq!(
            pipeline_build::classify_unwound_export("aborted".into(), export_user_cancelled())
                .unwrap()["status"],
            "cancelled"
        );
        drop(export);
        assert!(EXPORT_ACTIVE.load(Ordering::SeqCst));
        assert!(ExportActiveGuard::activate().is_err());
        assert!(EXPORT_CANCELLED.load(Ordering::SeqCst));
        drop(helper);
        assert!(!EXPORT_ACTIVE.load(Ordering::SeqCst));
        let next = ExportActiveGuard::activate().unwrap();
        assert!(!EXPORT_CANCELLED.load(Ordering::SeqCst));
        assert!(!export_user_cancelled());
        cancel_export();
        assert!(EXPORT_CANCELLED.load(Ordering::SeqCst));
        drop(next);
        assert!(!EXPORT_ACTIVE.load(Ordering::SeqCst));
    }
}
