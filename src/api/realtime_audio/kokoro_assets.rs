//! Managed Kokoro 82M v1.0 model component.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, LazyLock, Mutex};

use anyhow::Result;

use super::model_component_assets;
use crate::component_registry::models::{ModelKind, ModelUse};

static LAST_NOTICE: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));

#[cfg(not(feature = "recorder-worker"))]
pub fn current_kokoro_model_notice() -> Option<String> {
    LAST_NOTICE.lock().unwrap().clone()
}

pub fn get_kokoro_model_dir() -> PathBuf {
    crate::component_registry::models::model_dir(ModelKind::Kokoro)
}

pub fn get_kokoro_espeak_data_dir() -> PathBuf {
    get_kokoro_model_dir().join("espeak-ng-data")
}

pub fn get_kokoro_lexicon_paths() -> Vec<PathBuf> {
    let root = get_kokoro_model_dir();
    ["lexicon-us-en.txt", "lexicon-zh.txt"]
        .into_iter()
        .map(|name| root.join(name))
        .collect()
}

pub fn get_kokoro_rule_fst_paths() -> Vec<PathBuf> {
    let root = get_kokoro_model_dir();
    ["date-zh.fst", "number-zh.fst"]
        .into_iter()
        .map(|name| root.join(name))
        .collect()
}

pub fn is_kokoro_model_downloaded() -> bool {
    crate::component_registry::models::is_installed(ModelKind::Kokoro)
}

#[cfg(not(feature = "recorder-worker"))]
pub fn kokoro_model_installed_size() -> u64 {
    model_component_assets::installed_size(ModelKind::Kokoro)
}

pub(crate) fn acquire_kokoro_model() -> Result<ModelUse> {
    model_component_assets::acquire_model(ModelKind::Kokoro)
}

#[cfg(not(feature = "recorder-worker"))]
pub fn remove_kokoro_model() -> Result<()> {
    let result = model_component_assets::remove_model(ModelKind::Kokoro);
    update_notice(&result);
    result
}

pub fn download_kokoro_model(stop: Arc<AtomicBool>, use_badge: bool) -> Result<()> {
    let locale = {
        let app = crate::APP.lock().unwrap();
        crate::gui::locale::LocaleText::get(&app.config.ui_language)
    };
    let result = model_component_assets::ensure_model(
        ModelKind::Kokoro,
        stop,
        use_badge,
        locale.tool_runtime.kokoro_downloading_title,
        locale.tool_runtime.kokoro_downloading_message,
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
