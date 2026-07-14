use std::sync::{Mutex, OnceLock};

use anyhow::Result;
use ort::session::Session;
use ort::session::builder::GraphOptimizationLevel;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SessionKey {
    model_bytes: u64,
    model_modified_ms: u128,
    provider: &'static str,
}

struct CachedSession {
    key: SessionKey,
    session: Session,
    actual_provider: &'static str,
}

fn cache() -> &'static Mutex<Option<CachedSession>> {
    static CACHE: OnceLock<Mutex<Option<CachedSession>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

pub(super) fn requested_provider() -> &'static str {
    match std::env::var("CC_DETECTOR_PROVIDER") {
        Ok(value) if value.eq_ignore_ascii_case("cpu") => "cpu",
        _ => "directml",
    }
}

fn provider_is_explicit() -> bool {
    std::env::var("CC_DETECTOR_PROVIDER").is_ok()
}

pub(super) fn actual_provider() -> Option<&'static str> {
    cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ref()
        .map(|cached| cached.actual_provider)
}

pub(super) fn with_session<T>(
    path: &std::path::Path,
    work: impl FnOnce(&mut Session) -> Result<T>,
) -> Result<T> {
    let key = session_key(path)?;
    let mut guard = cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if guard.as_ref().is_none_or(|cached| cached.key != key) {
        *guard = Some(build_cached(path, key)?);
    }
    let cached = guard
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("detector session cache is empty"))?;
    work(&mut cached.session)
}

fn session_key(path: &std::path::Path) -> Result<SessionKey> {
    super::validate_model_file(path)?;
    if !super::runtime_ready() {
        anyhow::bail!("shared ONNX runtime is not installed");
    }
    let metadata = std::fs::metadata(path)?;
    let model_modified_ms = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|value| value.as_millis())
        .unwrap_or(0);
    Ok(SessionKey {
        model_bytes: metadata.len(),
        model_modified_ms,
        provider: requested_provider(),
    })
}

fn build_cached(path: &std::path::Path, key: SessionKey) -> Result<CachedSession> {
    let runtime = crate::unpack_dlls::private_bin_dir().join("onnxruntime.dll");
    ort::init_from(&runtime)
        .map_err(|error| anyhow::anyhow!("load {}: {error}", runtime.display()))?
        .commit();
    let requested = key.provider;
    let built = build_for(path, requested);
    let (session, actual_provider) = match built {
        Ok(session) => (session, requested),
        Err(error) if requested == "directml" && !provider_is_explicit() => {
            eprintln!("[cc-detector] DirectML unavailable ({error}); falling back to CPU");
            (build_for(path, "cpu")?, "cpu")
        }
        Err(error) => return Err(error),
    };
    eprintln!(
        "[cc-detector] loaded {} with provider={actual_provider}",
        path.display()
    );
    Ok(CachedSession {
        key,
        session,
        actual_provider,
    })
}

fn build_for(path: &std::path::Path, provider: &'static str) -> Result<Session> {
    let providers = if provider == "directml" {
        vec![
            ort::ep::DirectML::default().build().error_on_failure(),
            ort::ep::CPU::default().build().error_on_failure(),
        ]
    } else {
        vec![ort::ep::CPU::default().build().error_on_failure()]
    };
    eprintln!("[cc-detector] initializing provider={provider}");
    let builder =
        Session::builder().map_err(|error| anyhow::anyhow!("session builder: {error}"))?;
    let builder = builder
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|error| anyhow::anyhow!("opt level: {error}"))?;
    let builder = if provider == "directml" {
        builder
            .with_memory_pattern(false)
            .map_err(|error| anyhow::anyhow!("disable memory pattern: {error}"))?
            .with_parallel_execution(false)
            .map_err(|error| anyhow::anyhow!("sequential execution: {error}"))?
    } else {
        builder
    };
    builder
        .with_execution_providers(providers)
        .map_err(|error| anyhow::anyhow!("execution providers: {error}"))?
        .commit_from_file(path)
        .map_err(|error| anyhow::anyhow!("commit model: {error}"))
}
