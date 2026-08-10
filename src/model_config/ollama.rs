#[cfg(not(feature = "recorder-worker"))]
use super::ModelSource;
use super::{ModelConfig, ModelType};
use std::sync::{
    LazyLock, Mutex,
    atomic::{AtomicBool, Ordering},
};

static MODEL_CACHE: LazyLock<Mutex<Vec<ModelConfig>>> = LazyLock::new(|| Mutex::new(Vec::new()));
static SCAN_IN_PROGRESS: LazyLock<AtomicBool> = LazyLock::new(|| AtomicBool::new(false));
static LAST_SCAN: LazyLock<Mutex<std::time::Instant>> = LazyLock::new(|| {
    Mutex::new(
        std::time::Instant::now()
            .checked_sub(std::time::Duration::from_secs(10))
            .unwrap_or_else(std::time::Instant::now),
    )
});

pub(super) fn find_cached_model(id: &str) -> Option<ModelConfig> {
    MODEL_CACHE
        .lock()
        .ok()
        .and_then(|models| models.iter().find(|model| model.id == id).cloned())
}

pub(super) fn cached_models() -> Vec<ModelConfig> {
    MODEL_CACHE
        .lock()
        .map(|models| models.clone())
        .unwrap_or_default()
}

#[cfg(not(feature = "recorder-worker"))]
pub fn is_ollama_scan_in_progress() -> bool {
    SCAN_IN_PROGRESS.load(Ordering::SeqCst)
}

#[cfg(not(feature = "recorder-worker"))]
pub fn ollama_cached_model_count() -> usize {
    MODEL_CACHE.lock().map(|models| models.len()).unwrap_or(0)
}

pub fn trigger_ollama_model_scan() {
    let (use_ollama, base_url) = if let Ok(app) = crate::APP.lock() {
        (app.config.use_ollama, app.config.ollama_base_url.clone())
    } else {
        return;
    };

    if !use_ollama {
        return;
    }

    if LAST_SCAN
        .lock()
        .is_ok_and(|last_scan| last_scan.elapsed().as_secs() < 5)
        || SCAN_IN_PROGRESS.swap(true, Ordering::SeqCst)
    {
        return;
    }

    if let Ok(mut last_scan) = LAST_SCAN.lock() {
        *last_scan = std::time::Instant::now();
    }

    std::thread::spawn(move || {
        if let Ok(ollama_models) = crate::api::ollama::fetch_ollama_models_with_caps(&base_url) {
            let mut new_models = Vec::new();

            for ollama_model in ollama_models {
                let model_id = format!(
                    "ollama-{}",
                    ollama_model.name.replace(":", "-").replace("/", "-")
                );
                let display_name = format!("{} (Local)", ollama_model.name);

                if ollama_model.has_vision {
                    new_models.push(discovered_model(
                        format!("{}-vision", model_id),
                        ollama_model.name.clone(),
                        display_name.clone(),
                        ModelType::Vision,
                    ));
                    new_models.push(discovered_model(
                        model_id,
                        ollama_model.name,
                        display_name,
                        ModelType::Text,
                    ));
                } else {
                    new_models.push(discovered_model(
                        model_id,
                        ollama_model.name,
                        display_name,
                        ModelType::Text,
                    ));
                }
            }

            if let Ok(mut cache) = MODEL_CACHE.lock() {
                *cache = new_models;
            }
        }

        SCAN_IN_PROGRESS.store(false, Ordering::SeqCst);
    });
}

fn discovered_model(
    id: String,
    full_name: String,
    display_name: String,
    model_type: ModelType,
) -> ModelConfig {
    ModelConfig {
        id,
        provider: "ollama".to_string(),
        name_vi: display_name.clone(),
        name_ko: display_name.clone(),
        name_en: display_name,
        full_name,
        model_type,
        enabled: true,
        #[cfg(not(feature = "recorder-worker"))]
        quota_limit_vi: "Không giới hạn".to_string(),
        #[cfg(not(feature = "recorder-worker"))]
        quota_limit_ko: "무제한".to_string(),
        #[cfg(not(feature = "recorder-worker"))]
        quota_limit_en: "Unlimited".to_string(),
        #[cfg(not(feature = "recorder-worker"))]
        source: ModelSource::Discovered,
        supports_search_override: None,
        #[cfg(not(feature = "recorder-worker"))]
        search_tool_enabled_by_default: false,
        intelligence_tier: None,
        typical_latency_ms: None,
        performance_source: None,
    }
}
