//! Managed Step Audio EditX and tokenizer model component.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use anyhow::Result;

use super::model_component_assets;
use crate::component_registry::models::{ModelKind, ModelUse};

static DOWNLOADING: AtomicBool = AtomicBool::new(false);
static LAST_NOTICE: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));

#[cfg(not(feature = "recorder-worker"))]
pub fn current_step_audio_notice() -> Option<String> {
    LAST_NOTICE.lock().unwrap().clone()
}

pub fn get_step_audio_model_dir() -> PathBuf {
    crate::component_registry::models::model_dir(ModelKind::StepAudio)
}

pub fn get_step_audio_editx_dir() -> PathBuf {
    get_step_audio_model_dir().join("editx_awq")
}

pub fn get_step_audio_tokenizer_dir() -> PathBuf {
    get_step_audio_model_dir().join("tokenizer")
}

pub fn is_step_audio_model_downloading() -> bool {
    DOWNLOADING.load(Ordering::Relaxed)
}

pub fn is_step_audio_model_downloaded() -> bool {
    crate::component_registry::models::is_installed(ModelKind::StepAudio)
}

#[cfg(not(feature = "recorder-worker"))]
pub fn step_audio_model_installed_size() -> u64 {
    model_component_assets::installed_size(ModelKind::StepAudio)
}

pub(crate) fn acquire_step_audio_model() -> Result<ModelUse> {
    model_component_assets::acquire_model(ModelKind::StepAudio)
}

#[cfg(not(feature = "recorder-worker"))]
pub fn remove_step_audio_model() -> Result<()> {
    let result = model_component_assets::remove_model(ModelKind::StepAudio);
    update_notice(&result);
    result
}

pub fn download_step_audio_model(stop: Arc<AtomicBool>, use_badge: bool) -> Result<()> {
    if is_step_audio_model_downloaded() {
        return Ok(());
    }
    let _downloading = DownloadFlag::acquire(&DOWNLOADING);
    let locale = {
        let app = crate::APP.lock().unwrap();
        crate::gui::locale::LocaleText::get(&app.config.ui_language)
    };
    let result = model_component_assets::ensure_model(
        ModelKind::StepAudio,
        stop,
        use_badge,
        locale.tool_runtime.step_audio_downloading_title,
        locale.tool_runtime.step_audio_downloading_message,
    );
    update_notice(&result);
    result
}

struct DownloadFlag(&'static AtomicBool);

impl DownloadFlag {
    fn acquire(flag: &'static AtomicBool) -> Self {
        flag.store(true, Ordering::SeqCst);
        Self(flag)
    }
}

impl Drop for DownloadFlag {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

fn update_notice(result: &Result<()>) {
    let mut notice = LAST_NOTICE.lock().unwrap();
    *notice = result
        .as_ref()
        .err()
        .filter(|error| !error.to_string().contains("cancelled"))
        .map(ToString::to_string);
}
