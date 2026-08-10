//! Managed Qwen3-ASR model components.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, LazyLock, Mutex};

use anyhow::Result;

use crate::api::realtime_audio::model_component_assets;
use crate::component_registry::models::ModelKind;

static LAST_NOTICE: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));

#[cfg(not(feature = "recorder-worker"))]
pub fn current_qwen3_model_notice() -> Option<String> {
    LAST_NOTICE.lock().unwrap().clone()
}

pub fn get_qwen3_model_dir() -> PathBuf {
    crate::component_registry::models::model_dir(ModelKind::QwenSmall)
}

pub fn get_qwen3_1_7b_model_dir() -> PathBuf {
    crate::component_registry::models::model_dir(ModelKind::QwenLarge)
}

pub fn is_qwen3_model_downloaded() -> bool {
    crate::component_registry::models::is_installed(ModelKind::QwenSmall)
}

pub fn is_qwen3_1_7b_model_downloaded() -> bool {
    crate::component_registry::models::is_installed(ModelKind::QwenLarge)
}

#[cfg(not(feature = "recorder-worker"))]
pub fn qwen3_model_installed_size() -> u64 {
    model_component_assets::installed_size(ModelKind::QwenSmall)
}

#[cfg(not(feature = "recorder-worker"))]
pub fn qwen3_1_7b_model_installed_size() -> u64 {
    model_component_assets::installed_size(ModelKind::QwenLarge)
}

pub fn download_qwen3_model(stop: Arc<AtomicBool>, use_badge: bool) -> Result<()> {
    download(ModelKind::QwenSmall, stop, use_badge)
}

pub fn download_qwen3_1_7b_model(stop: Arc<AtomicBool>, use_badge: bool) -> Result<()> {
    download(ModelKind::QwenLarge, stop, use_badge)
}

#[cfg(not(feature = "recorder-worker"))]
pub fn remove_qwen3_model() -> Result<()> {
    let result = model_component_assets::remove_model(ModelKind::QwenSmall);
    update_notice(&result);
    result
}

#[cfg(not(feature = "recorder-worker"))]
pub fn remove_qwen3_1_7b_model() -> Result<()> {
    let result = model_component_assets::remove_model(ModelKind::QwenLarge);
    update_notice(&result);
    result
}

fn download(kind: ModelKind, stop: Arc<AtomicBool>, use_badge: bool) -> Result<()> {
    let locale = {
        let app = crate::APP.lock().unwrap();
        crate::gui::locale::LocaleText::get(&app.config.ui_language)
    };
    let title = match kind {
        ModelKind::QwenSmall => locale.tool_runtime.qwen3_downloading_title,
        ModelKind::QwenLarge => locale.tool_runtime.qwen3_1_7b_downloading_title,
        _ => unreachable!("Qwen asset wrapper received another model kind"),
    };
    let result = model_component_assets::ensure_model(
        kind,
        stop,
        use_badge,
        title,
        locale.tool_runtime.qwen3_downloading_message,
    );
    update_notice(&result);
    result
}

fn update_notice(result: &Result<()>) {
    let mut notice = LAST_NOTICE.lock().unwrap();
    *notice = result
        .as_ref()
        .err()
        .filter(|error| !error.to_string().contains("cancelled"))
        .map(ToString::to_string);
}
