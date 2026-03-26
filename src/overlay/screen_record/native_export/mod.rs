pub mod anim_cache;
mod audio_mix;
mod background_presets;
mod camera_path;
mod capabilities;
mod composition;
pub mod config;
mod cursor;
mod cursor_path;
mod gif;
mod overlay;
pub mod overlay_frames;
mod pipeline;
mod pipeline_build;
mod progress;
pub mod sampling;
pub mod staging;
mod util;
mod warmup;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use super::gpu_export;
use super::gpu_pipeline;
use super::mf_decode;
use super::mf_encode;
use super::SR_HWND;

pub use composition::start_composition_export;
pub(crate) use pipeline::run_native_export_with_staged;
pub use pipeline::start_native_export;
pub use progress::{export_replay_args_path, persist_export_result};
pub use warmup::warm_up_export_pipeline_when_idle;

pub fn prewarm_custom_background(url: &str) -> Result<(), String> {
    overlay::prewarm_custom_background(url)
}

/// Flag to signal export cancellation from the frontend.
static EXPORT_CANCELLED: AtomicBool = AtomicBool::new(false);
/// Ensures GPU export warm-up runs once per app session.
static EXPORT_GPU_WARMED: AtomicBool = AtomicBool::new(false);
/// Indicates an export is actively running.
static EXPORT_ACTIVE: AtomicBool = AtomicBool::new(false);

struct ExportActiveGuard;

impl ExportActiveGuard {
    fn activate() -> Self {
        EXPORT_ACTIVE.store(true, Ordering::SeqCst);
        Self
    }
}

impl Drop for ExportActiveGuard {
    fn drop(&mut self) {
        EXPORT_ACTIVE.store(false, Ordering::SeqCst);
    }
}

pub fn cancel_export() {
    println!("[Cancel] Setting EXPORT_CANCELLED flag");
    EXPORT_CANCELLED.store(true, Ordering::SeqCst);
    println!("[Cancel] Cancellation signaled");
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
