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
    {
        return;
    }
    let Some(claim) = ScanClaim::acquire() else {
        return;
    };

    if let Ok(mut last_scan) = LAST_SCAN.lock() {
        *last_scan = std::time::Instant::now();
    }

    crate::task_runtime::spawn_detached(
        crate::task_runtime::TaskClass::Interactive,
        "ollama-model-scan",
        scan_job(claim, base_url),
    );
}

/// Exclusive ownership of the single in-flight scan. Releasing on drop covers
/// every way the job can end: finished, panicked, or never run because the
/// task queue rejected or expired it.
struct ScanClaim;

impl ScanClaim {
    fn acquire() -> Option<Self> {
        (!SCAN_IN_PROGRESS.swap(true, Ordering::SeqCst)).then_some(Self)
    }
}

impl Drop for ScanClaim {
    fn drop(&mut self) {
        SCAN_IN_PROGRESS.store(false, Ordering::SeqCst);
    }
}

fn scan_job(claim: ScanClaim, base_url: String) -> impl FnOnce() + Send + 'static {
    move || {
        let _claim = claim;
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
    }
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
        search_tool_enabled_by_default: false,
        intelligence_tier: None,
        typical_latency_ms: None,
        performance_source: None,
    }
}

/// Lists one discovered text model while the guard lives, as a finished scan
/// would, so rendered settings can be checked without an Ollama server.
#[cfg(all(test, not(feature = "recorder-worker")))]
pub(crate) struct DiscoveredForTest(String);

#[cfg(all(test, not(feature = "recorder-worker")))]
impl DiscoveredForTest {
    pub(crate) fn text_model(id: &str, display_name: &str) -> Self {
        MODEL_CACHE.lock().unwrap().push(discovered_model(
            id.to_string(),
            display_name.to_string(),
            display_name.to_string(),
            ModelType::Text,
        ));
        Self(id.to_string())
    }
}

#[cfg(all(test, not(feature = "recorder-worker")))]
impl Drop for DiscoveredForTest {
    fn drop(&mut self) {
        if let Ok(mut models) = MODEL_CACHE.lock() {
            models.retain(|model| model.id != self.0);
        }
    }
}

#[cfg(all(test, not(feature = "recorder-worker")))]
mod tests {
    use super::*;

    #[test]
    fn a_scan_job_that_never_runs_releases_the_in_progress_claim() {
        let claim = ScanClaim::acquire().expect("no scan is running in tests");
        assert!(is_ollama_scan_in_progress());
        assert!(ScanClaim::acquire().is_none(), "the claim is exclusive");
        // A full task queue, an expired task, or a panic drops the job unrun.
        drop(scan_job(claim, "http://127.0.0.1:9".to_string()));
        assert!(!is_ollama_scan_in_progress());
        drop(ScanClaim::acquire().expect("a later scan can start"));
        assert!(!is_ollama_scan_in_progress());
    }

    #[test]
    fn previously_unresolved_id_resolves_after_discovery_and_rediscovery() {
        let id = "ollama-saved-model-discovery-fixture";
        MODEL_CACHE.lock().unwrap().retain(|model| model.id != id);
        assert!(super::super::get_model_by_id_with_custom(id, &[]).is_none());
        for _ in 0..2 {
            MODEL_CACHE.lock().unwrap().push(discovered_model(
                id.to_string(),
                "fixture".to_string(),
                "fixture".to_string(),
                ModelType::Text,
            ));
            assert_eq!(
                super::super::get_model_by_id_with_custom(id, &[])
                    .unwrap()
                    .id,
                id
            );
            MODEL_CACHE.lock().unwrap().retain(|model| model.id != id);
            assert!(super::super::get_model_by_id_with_custom(id, &[]).is_none());
        }
    }
}
