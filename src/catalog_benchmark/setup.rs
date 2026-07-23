use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};

use crate::model_config::{ModelConfig, ModelType, get_all_models, model_is_non_llm};

pub struct Credentials {
    pub groq: String,
    pub gemini: String,
    openrouter: String,
    cerebras: String,
}

#[derive(Clone, Copy)]
pub struct Suites {
    pub text: bool,
    pub coordinate: bool,
    pub ocr: bool,
}

pub fn model_filter() -> Option<HashSet<String>> {
    let values = std::env::var("CATALOG_BENCH_MODELS").ok()?;
    Some(
        values
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect(),
    )
}

pub fn resume_inputs() -> Vec<PathBuf> {
    std::env::var_os("CATALOG_BENCH_RESUME_INPUTS")
        .map(|value| {
            value
                .to_string_lossy()
                .split(';')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .collect()
        })
        .unwrap_or_default()
}

pub fn select_models(
    model_type: ModelType,
    filter: Option<&HashSet<String>>,
    credentials: &Credentials,
) -> Vec<ModelConfig> {
    get_all_models()
        .iter()
        .filter(|model| model.enabled && model.model_type == model_type)
        .filter(|model| model_type != ModelType::Vision || !model_is_non_llm(&model.id))
        .filter(|model| {
            filter.is_none_or(|ids| ids.contains(&model.id) || ids.contains(&model.full_name))
        })
        .filter(|model| {
            let available = credentials.supports(&model.provider);
            if !available {
                println!(
                    "BENCH_SKIP model={} provider={} reason=credential_or_runtime_unavailable",
                    model.id, model.provider
                );
            }
            available
        })
        .cloned()
        .collect()
}

pub fn ensure_selection(
    suites: Suites,
    text: &[ModelConfig],
    vision: &[ModelConfig],
) -> Result<()> {
    if suites.text && text.is_empty() {
        bail!("no available text models matched the benchmark selection");
    }
    if (suites.coordinate || suites.ocr) && vision.is_empty() {
        bail!("no available vision models matched the benchmark selection");
    }
    Ok(())
}

impl Credentials {
    pub fn load() -> Self {
        let config = crate::APP
            .lock()
            .ok()
            .map(|app| app.config.clone())
            .unwrap_or_default();
        Self {
            groq: crate::api::provider_credentials::resolve("GROQ_API_KEY", &config.api_key),
            gemini: crate::api::provider_credentials::resolve(
                "GEMINI_API_KEY",
                &config.gemini_api_key,
            ),
            openrouter: crate::api::provider_credentials::resolve(
                "OPENROUTER_API_KEY",
                &config.openrouter_api_key,
            ),
            cerebras: crate::api::provider_credentials::resolve(
                "CEREBRAS_API_KEY",
                &config.cerebras_api_key,
            ),
        }
    }

    fn supports(&self, provider: &str) -> bool {
        match provider {
            "google" | "gemini-live" => !self.gemini.is_empty(),
            "groq" => !self.groq.is_empty(),
            "openrouter" => !self.openrouter.is_empty(),
            "cerebras" => !self.cerebras.is_empty(),
            "google-gtx" | "taalas" | "ollama" => true,
            _ => false,
        }
    }
}

impl Suites {
    pub fn from_env() -> Result<Self> {
        let Some(value) = std::env::var("CATALOG_BENCH_SUITES").ok() else {
            return Ok(Self {
                text: true,
                coordinate: true,
                ocr: true,
            });
        };
        let selected: HashSet<_> = value
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .collect();
        for suite in &selected {
            ensure!(
                ["text", "coordinate", "ocr"].contains(suite),
                "unknown benchmark suite: {suite}"
            );
        }
        ensure!(!selected.is_empty(), "CATALOG_BENCH_SUITES cannot be empty");
        Ok(Self {
            text: selected.contains("text"),
            coordinate: selected.contains("coordinate"),
            ocr: selected.contains("ocr"),
        })
    }
}

pub struct Pacer {
    min_interval: Duration,
    last_call: HashMap<String, Instant>,
}

impl Pacer {
    pub fn from_env() -> Result<Self> {
        let milliseconds = std::env::var("CATALOG_BENCH_MIN_INTERVAL_MS")
            .unwrap_or_else(|_| "2500".to_string())
            .parse::<u64>()
            .context("parse CATALOG_BENCH_MIN_INTERVAL_MS")?;
        Ok(Self {
            min_interval: Duration::from_millis(milliseconds),
            last_call: HashMap::new(),
        })
    }

    pub fn wait(&mut self, provider: &str) {
        if let Some(previous) = self.last_call.get(provider) {
            std::thread::sleep(self.min_interval.saturating_sub(previous.elapsed()));
        }
        self.last_call.insert(provider.to_string(), Instant::now());
    }
}

pub fn request_timeout() -> Result<Option<Duration>> {
    std::env::var("CATALOG_BENCH_REQUEST_TIMEOUT_SECS")
        .ok()
        .map(|value| {
            value
                .parse::<u64>()
                .context("parse CATALOG_BENCH_REQUEST_TIMEOUT_SECS")
        })
        .transpose()
        .map(|seconds| seconds.map(Duration::from_secs))
}

pub fn output_dir() -> PathBuf {
    std::env::var_os("CATALOG_BENCH_OUTPUT").map_or_else(
        || {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("target/catalog-benchmark")
                .join(chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string())
        },
        PathBuf::from,
    )
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{Credentials, select_models};
    use crate::model_config::ModelType;

    fn empty_credentials() -> Credentials {
        Credentials {
            groq: String::new(),
            gemini: String::new(),
            openrouter: String::new(),
            cerebras: String::new(),
        }
    }

    #[test]
    fn selection_keeps_translation_service_but_excludes_non_llm_vision() {
        let credentials = empty_credentials();
        let text_filter = HashSet::from(["google-gtx-translate-text".to_string()]);
        let text = select_models(ModelType::Text, Some(&text_filter), &credentials);
        assert_eq!(text.len(), 1);
        assert_eq!(text[0].id, "google-gtx-translate-text");

        let vision_filter = HashSet::from(["qrserver-qr-scanner-vision".to_string()]);
        assert!(select_models(ModelType::Vision, Some(&vision_filter), &credentials).is_empty());
    }
}
