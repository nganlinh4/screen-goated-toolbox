use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};

use crate::model_config::{ModelConfig, ModelType, get_all_models, model_is_non_llm};

pub struct Credentials {
    groq: CredentialPool,
    gemini: CredentialPool,
    openrouter: CredentialPool,
    nvidia: CredentialPool,
}

struct CredentialPool {
    values: Vec<String>,
    next: AtomicUsize,
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
            groq: CredentialPool::load("GROQ_API_KEY", &config.api_key),
            gemini: CredentialPool::load("GEMINI_API_KEY", &config.gemini_api_key),
            openrouter: CredentialPool::load("OPENROUTER_API_KEY", &config.openrouter_api_key),
            nvidia: CredentialPool::load("NVIDIA_API_KEY", &config.nvidia_api_key),
        }
    }

    pub fn supports(&self, provider: &str) -> bool {
        match provider {
            "google" | "gemini-live" => !self.gemini.is_empty(),
            "groq" => !self.groq.is_empty(),
            "openrouter" => !self.openrouter.is_empty(),
            "nvidia" => !self.nvidia.is_empty(),
            "google-gtx" | "taalas" | "ollama" => true,
            _ => false,
        }
    }

    /// The Groq key for this attempt. Only a Groq model consumes one, and for
    /// those the rotated value arrives through [`Self::with_provider_key`].
    pub fn groq_key_for<'a>(provider: &str, provider_key: &'a str) -> &'a str {
        if provider == "groq" { provider_key } else { "" }
    }

    pub fn with_provider_key<T>(&self, provider: &str, operation: impl FnOnce(&str) -> T) -> T {
        match provider {
            "google" | "gemini-live" => {
                let key = self.gemini.next();
                crate::api::provider_credentials::with_override("GEMINI_API_KEY", key, || {
                    operation(key)
                })
            }
            "groq" => {
                let key = self.groq.next();
                crate::api::provider_credentials::with_override("GROQ_API_KEY", key, || {
                    operation(key)
                })
            }
            "openrouter" => {
                let key = self.openrouter.next();
                crate::api::provider_credentials::with_override("OPENROUTER_API_KEY", key, || {
                    operation("")
                })
            }
            "nvidia" => {
                let key = self.nvidia.next();
                crate::api::provider_credentials::with_override("NVIDIA_API_KEY", key, || {
                    operation(key)
                })
            }
            _ => operation(self.gemini.first()),
        }
    }
}

impl CredentialPool {
    fn load(primary_name: &str, saved: &str) -> Self {
        let mut values = Vec::new();
        let primary =
            environment_or_dotenv(primary_name).unwrap_or_else(|| saved.trim().to_string());
        if !primary.is_empty() {
            values.push(primary);
        }
        for suffix in credential_slots(primary_name) {
            let name = format!("{primary_name}_{suffix}");
            if let Some(value) = environment_or_dotenv(&name)
                && !values.contains(&value)
            {
                values.push(value);
            }
        }
        Self::from_values(values)
    }

    fn empty() -> Self {
        Self::from_values(Vec::new())
    }

    fn from_values(values: Vec<String>) -> Self {
        Self {
            values,
            next: AtomicUsize::new(0),
        }
    }

    fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    fn first(&self) -> &str {
        self.values.first().map(String::as_str).unwrap_or("")
    }

    fn next(&self) -> &str {
        let index = self.next.fetch_add(1, Ordering::Relaxed);
        &self.values[index % self.values.len()]
    }
}

fn credential_slots(primary_name: &str) -> BTreeSet<usize> {
    let mut slots = std::env::vars()
        .filter_map(|(name, _)| credential_slot(&name, primary_name))
        .collect::<BTreeSet<_>>();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".env");
    if let Ok(contents) = std::fs::read_to_string(path) {
        slots.extend(dotenv_credential_slots(&contents, primary_name));
    }
    slots
}

fn dotenv_credential_slots(contents: &str, primary_name: &str) -> BTreeSet<usize> {
    contents
        .lines()
        .filter_map(|line| {
            let line = line.trim().strip_prefix("export ").unwrap_or(line.trim());
            let (name, _) = line.split_once('=')?;
            credential_slot(name.trim(), primary_name)
        })
        .collect()
}

fn credential_slot(name: &str, primary_name: &str) -> Option<usize> {
    let suffix = name.strip_prefix(primary_name)?.strip_prefix('_')?;
    let slot = suffix.parse::<usize>().ok()?;
    (slot >= 2 && suffix == slot.to_string()).then_some(slot)
}

fn environment_or_dotenv(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".env");
            let contents = std::fs::read_to_string(path).ok()?;
            dotenv_value(&contents, name)
        })
}

fn dotenv_value(contents: &str, name: &str) -> Option<String> {
    contents.lines().find_map(|line| {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return None;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let (candidate, value) = line.split_once('=')?;
        if candidate.trim() != name {
            return None;
        }
        let value = value.trim();
        let value = value
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .or_else(|| {
                value
                    .strip_prefix('\'')
                    .and_then(|value| value.strip_suffix('\''))
            })
            .unwrap_or(value)
            .trim();
        (!value.is_empty()).then(|| value.to_string())
    })
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

const OPENROUTER_FREE_MIN_INTERVAL: Duration = Duration::from_millis(3_100);

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
            std::thread::sleep(
                self.interval_for(provider)
                    .saturating_sub(previous.elapsed()),
            );
        }
        self.last_call.insert(provider.to_string(), Instant::now());
    }

    fn interval_for(&self, provider: &str) -> Duration {
        match provider {
            "openrouter" => self.min_interval.max(OPENROUTER_FREE_MIN_INTERVAL),
            _ => self.min_interval,
        }
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
            history_root().join("runs").join(format!(
                "{}-{}",
                chrono::Utc::now().format("%Y%m%d-%H%M%S-%3f"),
                std::process::id()
            ))
        },
        PathBuf::from,
    )
}

pub fn history_root() -> PathBuf {
    std::env::var_os("CATALOG_BENCH_HISTORY_ROOT").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/catalog-benchmark"),
        PathBuf::from,
    )
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::time::Duration;

    use super::{
        CredentialPool, Credentials, Pacer, dotenv_credential_slots, dotenv_value, select_models,
    };
    use crate::model_config::ModelType;

    fn empty_credentials() -> Credentials {
        Credentials {
            groq: CredentialPool::empty(),
            gemini: CredentialPool::empty(),
            openrouter: CredentialPool::empty(),
            nvidia: CredentialPool::empty(),
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

    #[test]
    fn groq_rotates_across_indexed_credentials_like_the_other_pools() {
        // Groq was the only provider read as a single key, so a second slot in
        // .env changed nothing. It now cycles like Gemini and OpenRouter.
        let pool = CredentialPool::from_values(vec!["a".to_string(), "b".to_string()]);
        assert_eq!(pool.next(), "a");
        assert_eq!(pool.next(), "b");
        assert_eq!(pool.next(), "a");

        // Only a Groq model consumes the rotated key.
        assert_eq!(Credentials::groq_key_for("groq", "rotated"), "rotated");
        assert_eq!(Credentials::groq_key_for("google", "rotated"), "");
    }

    #[test]
    fn pacer_respects_provider_free_tier_rates() {
        let default = Pacer {
            min_interval: Duration::from_millis(2_500),
            last_call: Default::default(),
        };
        assert_eq!(
            default.interval_for("openrouter"),
            Duration::from_millis(3_100)
        );
        let slower_override = Pacer {
            min_interval: Duration::from_millis(5_000),
            last_call: Default::default(),
        };
        assert_eq!(
            slower_override.interval_for("openrouter"),
            Duration::from_millis(5_000)
        );
    }

    #[test]
    fn credential_pool_rotates_in_stable_round_robin_order() {
        let pool = CredentialPool::from_values(vec![
            "first".to_string(),
            "second".to_string(),
            "third".to_string(),
        ]);
        assert_eq!(pool.next(), "first");
        assert_eq!(pool.next(), "second");
        assert_eq!(pool.next(), "third");
        assert_eq!(pool.next(), "first");
    }

    #[test]
    fn dotenv_parser_accepts_benchmark_slots_without_exposing_other_values() {
        let contents = "# local credentials\nGEMINI_API_KEY=primary\nexport GEMINI_API_KEY_2=\"second\"\nGEMINI_API_KEY_20=twentieth\nGEMINI_API_KEY_02=ignored\nOPENROUTER_API_KEY=other\n";
        assert_eq!(
            dotenv_value(contents, "GEMINI_API_KEY"),
            Some("primary".to_string())
        );
        assert_eq!(
            dotenv_value(contents, "GEMINI_API_KEY_2"),
            Some("second".to_string())
        );
        assert_eq!(dotenv_value(contents, "GEMINI_API_KEY_3"), None);
        assert_eq!(
            dotenv_value(contents, "GEMINI_API_KEY_20"),
            Some("twentieth".to_string())
        );
        assert_eq!(
            dotenv_credential_slots(contents, "GEMINI_API_KEY"),
            [2, 20].into_iter().collect()
        );
    }
}
