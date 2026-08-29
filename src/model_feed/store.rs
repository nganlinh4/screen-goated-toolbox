//! Fetching, caching and applying the availability feed.
//!
//! The feed is optional infrastructure: every failure path here leaves the app
//! exactly as it was. A missing feed, an unreachable host, a signature that does
//! not verify, or a schema this build does not understand all resolve to "no
//! offered models", never to a partial application.

use sha2::{Digest as _, Sha256};
#[cfg(not(feature = "recorder-worker"))]
use std::io::Read;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
#[cfg(not(feature = "recorder-worker"))]
use std::time::Duration;

#[cfg(not(feature = "recorder-worker"))]
use anyhow::{Result, bail};

use super::{AvailabilityFeed, parse_verified};
use crate::model_config::ModelType;

/// Published on a data-only branch rather than `main`, so a two-hourly bot commit
/// never collides with development history.
#[cfg(not(feature = "recorder-worker"))]
const FEED_URL: &str = "https://raw.githubusercontent.com/nganlinh4/screen-goated-toolbox/monitoring-feed/nvidia-availability.json";
#[cfg(not(feature = "recorder-worker"))]
const SIGNATURE_URL: &str = "https://raw.githubusercontent.com/nganlinh4/screen-goated-toolbox/monitoring-feed/nvidia-availability.json.sig";

/// How stale a cached feed may get before it is refetched.
///
/// Shorter than the two-hourly publish on purpose. The publisher only decides
/// *when a verdict changes*; this decides *how long a user keeps acting on the
/// old one*. When the monitor demotes a model, matching the publish interval
/// meant the app could go on offering it for another two hours on top of the
/// run that condemned it. The file is under two kilobytes, so checking eight
/// times as often costs nothing worth counting.
#[cfg(not(feature = "recorder-worker"))]
const REFRESH_INTERVAL: Duration = Duration::from_secs(15 * 60);

#[cfg(not(feature = "recorder-worker"))]
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);

/// A feed is small. Anything larger is not our file and is refused before parsing.
const MAX_FEED_BYTES: usize = 256 * 1024;
const SIGNATURE_BYTES: usize = 64;

fn cache_dir() -> PathBuf {
    crate::paths::app_runtime_local_data_dir().join("model-feed")
}

fn feed_path() -> PathBuf {
    cache_dir().join("nvidia-availability.json")
}

fn signature_path() -> PathBuf {
    cache_dir().join("nvidia-availability.json.sig")
}

/// The verified feed, held in memory once it has been read.
///
/// `None` means "no feed": absent, unreadable, unverifiable, or a schema this
/// build does not understand. It is never a partial application.
static FEED: RwLock<Option<Option<Arc<AvailabilityFeed>>>> = RwLock::new(None);

/// The cached feed, if one is present and verifies.
///
/// Read from disk at most once per refresh. This sits in the request path -- the
/// reasoning control for every call and the retry chain for every attempt come
/// through here -- and re-reading two files and recomputing a P-256 verification
/// each time would put disk I/O and elliptic-curve arithmetic on every request.
///
/// Verification is still performed on every *load*, never trusted from the time
/// the file was written, so a cache edited on disk cannot influence routing.
pub fn cached() -> Option<Arc<AvailabilityFeed>> {
    if let Ok(guard) = FEED.read()
        && let Some(resolved) = guard.as_ref()
    {
        return resolved.clone();
    }
    let resolved = load_from_disk().map(Arc::new);
    if let Ok(mut guard) = FEED.write() {
        *guard = Some(resolved.clone());
    }
    resolved
}

/// Reads and verifies the cache file, with no memoisation.
fn load_from_disk() -> Option<AvailabilityFeed> {
    let payload = std::fs::read(feed_path()).ok()?;
    let signature = std::fs::read(signature_path()).ok()?;
    if payload.len() > MAX_FEED_BYTES || signature.len() != SIGNATURE_BYTES {
        return None;
    }
    parse_verified(&payload, &signature).ok()
}

/// Drops the memoised feed so the next read reloads it.
///
/// Called after a successful refresh: without it the process would serve the
/// feed it started with until it exited, which is the whole reason the refresh
/// runs on a timer.
#[cfg(not(feature = "recorder-worker"))]
fn invalidate_memo() {
    if let Ok(mut guard) = FEED.write() {
        *guard = None;
    }
}

/// Whether the cache is old enough to be worth refetching.
#[cfg(not(feature = "recorder-worker"))]
pub fn is_stale() -> bool {
    let Ok(metadata) = std::fs::metadata(feed_path()) else {
        return true;
    };
    metadata
        .modified()
        .ok()
        .and_then(|written| written.elapsed().ok())
        .is_none_or(|age| age >= REFRESH_INTERVAL)
}

#[cfg(not(feature = "recorder-worker"))]
fn get(url: &str, limit: usize) -> Result<Vec<u8>> {
    let response = crate::api::client::UREQ_AGENT
        .get(url)
        .config()
        .timeout_global(Some(FETCH_TIMEOUT))
        .build()
        .call()?;
    let mut body = Vec::new();
    response
        .into_body()
        .into_reader()
        .take(limit as u64 + 1)
        .read_to_end(&mut body)?;
    if body.len() > limit {
        bail!("availability feed is larger than expected");
    }
    Ok(body)
}

/// Downloads the feed and stores it only if it verifies.
///
/// An unverifiable download is discarded without touching the cache, so a bad
/// publish cannot evict a good feed that is already trusted.
#[cfg(not(feature = "recorder-worker"))]
pub fn refresh() -> Result<AvailabilityFeed> {
    let payload = get(FEED_URL, MAX_FEED_BYTES)?;
    let signature = get(SIGNATURE_URL, SIGNATURE_BYTES)?;
    if signature.len() != SIGNATURE_BYTES {
        bail!("availability feed signature has an unexpected length");
    }
    let feed = parse_verified(&payload, &signature)?;

    let directory = cache_dir();
    std::fs::create_dir_all(&directory)?;
    crate::atomic_json::write_bytes_atomic(&feed_path(), &payload)?;
    crate::atomic_json::write_bytes_atomic(&signature_path(), &signature)?;
    invalidate_memo();
    if let Ok(context) = crate::gui::GUI_CONTEXT.lock()
        && let Some(context) = context.as_ref()
    {
        context.request_repaint();
    }
    Ok(feed)
}

/// Keeps the cached feed fresh for as long as the app runs.
///
/// A single refresh at startup was not enough: a session left open all day acted
/// on whatever the feed said when it launched, so a model demoted at noon went on
/// being offered until the next restart. This rechecks on a timer instead.
///
/// Startup never waits on it and a failure is only logged. The feed is an
/// optimisation, and the configured chain works without it.
#[cfg(not(feature = "recorder-worker"))]
pub fn refresh_in_background() {
    std::thread::spawn(|| {
        loop {
            refresh_if_stale();
            std::thread::sleep(REFRESH_INTERVAL);
        }
    });
}

/// One refresh attempt, made only when the cache has aged out.
#[cfg(not(feature = "recorder-worker"))]
fn refresh_if_stale() {
    if !is_stale() {
        return;
    }
    match refresh() {
        Ok(feed) => crate::log_info!(
            "[Model feed] {} availability refreshed, {} offered",
            feed.provider,
            super::ranked_models(&feed).len()
        ),
        Err(error) => crate::log_info!("[Model feed] Availability refresh skipped: {error:#}"),
    }
}

/// Models the feed offers that the compiled catalog has never heard of.
///
/// This is what stops the product's freshness being tied to its release cadence.
/// A catalog row is a build-time decision; the monitor probes the live provider
/// every couple of hours and already knows the endpoint's modality, its working
/// reasoning control and its latency -- everything a request needs. Requiring a
/// release before any of that can be used means a model discovered on Tuesday is
/// unreachable until the next build ships.
///
/// A model that *does* have a catalog row is skipped, so the curated row keeps
/// its stable identity and reviewed presentation. Its live availability,
/// request control and latency still come from this feed.
#[cfg(not(feature = "recorder-worker"))]
pub fn discovered_models() -> Vec<crate::model_config::ModelConfig> {
    let Some(feed) = cached() else {
        return Vec::new();
    };
    discovered_models_from_feed(&feed)
}

/// Pure discovery projection used by routing and hermetic contract tests.
#[cfg(not(feature = "recorder-worker"))]
pub(super) fn discovered_models_from_feed(
    feed: &AvailabilityFeed,
) -> Vec<crate::model_config::ModelConfig> {
    super::ranked_models(feed)
        .into_iter()
        .filter(|model| {
            feed_modality(model)
                .is_some_and(|model_type| !catalog_knows(&feed.provider, &model.id, model_type))
        })
        .filter_map(|model| discovered_model(&feed.provider, model))
        .collect()
}

/// Builds a routable model from one feed entry.
///
/// The id is derived from the endpoint name rather than authored, because there
/// is nobody to author it: the feed is the only place this model exists. Deriving
/// it also makes it stable, so a preset pinned to a discovered model keeps
/// working when the model later earns a catalog row under the same endpoint.
#[cfg(not(feature = "recorder-worker"))]
fn discovered_model(
    provider: &str,
    model: &super::FeedModel,
) -> Option<crate::model_config::ModelConfig> {
    let model_type = feed_modality(model)?;
    let display = crate::model_config::compact_provider_endpoint_name(provider, &model.id);
    Some(crate::model_config::ModelConfig {
        id: discovered_id(provider, &model.id),
        provider: provider.to_string(),
        name_vi: display.clone(),
        name_ko: display.clone(),
        name_en: display,
        full_name: model.id.clone(),
        model_type,
        enabled: true,
        quota_limit_vi: "Không giới hạn".to_string(),
        quota_limit_ko: "무제한".to_string(),
        quota_limit_en: "Unlimited".to_string(),
        source: crate::model_config::ModelSource::Discovered,
        supports_search_override: Some(false),
        search_tool_enabled_by_default: false,
        intelligence_tier: None,
        // Measured from one datacentre, so it orders candidates and nothing more.
        typical_latency_ms: model.p50_ms,
        performance_source: Some("availability-feed".to_string()),
    })
}

/// A stable id for a model that exists only in the feed.
pub fn discovered_id(provider: &str, full_name: &str) -> String {
    let mut slug: String = full_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    slug = slug.trim_matches('-').chars().take(48).collect();
    let slug = slug.trim_end_matches('-');
    let slug = if slug.is_empty() { "model" } else { slug };
    let digest = Sha256::digest(format!("{provider}:{full_name}").as_bytes());
    let suffix = digest[..4]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("{provider}-{slug}-{suffix}")
}

/// The reasoning control the publisher last found working for an endpoint.
///
/// Read straight from the cached feed rather than from a catalog row, because
/// this is the fact most likely to go stale: the catalog fixes it when the build
/// is cut, while the monitor rediscovers it from the live endpoint. Sending a
/// control an endpoint does not accept turned a healthy model into HTTP 500
/// during evaluation, so a fresher answer is worth preferring.
#[cfg(not(feature = "recorder-worker"))]
pub fn control_for(provider: &str, full_name: &str) -> Option<super::FeedControl> {
    let feed = cached()?;
    if feed.provider != provider {
        return None;
    }
    feed.models
        .iter()
        .find(|model| model.id == full_name)
        .and_then(|model| model.control)
}

/// Models the feed currently offers, best first, with their newest measured
/// latency. Keeping the signal beside the resolved id prevents a curated model's
/// release-time latency from overriding a fresher live measurement.
///
/// Only ids for a provider the user has enabled and holds a credential for are
/// returned: an offer the user cannot use would only lengthen the chain with
/// entries that must fail before anything else is tried.
pub fn offered_models(config: &crate::config::Config, wanted: ModelType) -> Vec<(String, u32)> {
    let Some(feed) = cached() else {
        return Vec::new();
    };
    if !crate::retry_model_chain::provider_is_available(&feed.provider, config) {
        return Vec::new();
    }
    super::ranked_models(&feed)
        .into_iter()
        .filter(|model| feed_modality(model) == Some(wanted))
        .filter_map(|model| {
            catalog_id_for(&feed.provider, &model.id, wanted)
                .map(|id| (id, model.p50_ms.unwrap_or(u32::MAX)))
        })
        .collect()
}

/// Whether a valid feed currently owns NVIDIA's operational state for this user.
///
/// Without a usable provider credential the normal configured chain is retained:
/// it will be skipped by provider preflight, and signing/cache failures remain a
/// fail-open optimisation failure rather than silently rewriting user settings.
pub fn is_authoritative(config: &crate::config::Config) -> bool {
    cached()
        .is_some_and(|feed| crate::retry_model_chain::provider_is_available(&feed.provider, config))
}

/// Applies the feed's current availability to a configured chain.
///
/// Only rows belonging to the feed provider are reconciled. Local and other
/// provider rows retain their authored order, while an NVIDIA catalog row absent
/// from the signed offer set cannot survive merely because an older build knew it.
pub fn reconcile_configured_chain(
    config: &crate::config::Config,
    wanted: ModelType,
    configured: &[String],
) -> Vec<String> {
    let Some(feed) = cached().filter(|_| is_authoritative(config)) else {
        return configured.to_vec();
    };
    reconcile_configured_chain_from_feed(&feed, wanted, configured, &config.custom_models)
}

fn reconcile_configured_chain_from_feed(
    feed: &AvailabilityFeed,
    wanted: ModelType,
    configured: &[String],
    custom_models: &[crate::config::types::CustomModelDefinition],
) -> Vec<String> {
    configured
        .iter()
        .filter(|id| {
            crate::model_config::get_model_by_id_with_custom(id, custom_models).is_none_or(
                |model| {
                    model.provider != feed.provider
                        || endpoint_is_offered(feed, wanted, &model.full_name)
                },
            )
        })
        .cloned()
        .collect()
}

/// Whether this endpoint is part of the signed provider offer for the requested
/// generic modality. Reviewed withdrawals remain a separate product veto.
pub fn endpoint_is_offered(feed: &AvailabilityFeed, wanted: ModelType, full_name: &str) -> bool {
    if crate::model_config::is_withdrawn_endpoint(&feed.provider, full_name) {
        return false;
    }
    super::ranked_models(feed)
        .into_iter()
        .any(|model| model.id == full_name && feed_modality(model) == Some(wanted))
}

/// Projects one provider's dynamic inventory without disturbing any other
/// catalog source. This is shared by every selector-producing catalog path.
#[cfg(not(feature = "recorder-worker"))]
pub(crate) fn project_provider_inventory(
    models: &mut Vec<crate::model_config::ModelConfig>,
    feed: &AvailabilityFeed,
) {
    models.retain(|model| {
        model.provider != feed.provider
            || endpoint_is_offered(feed, model.model_type, &model.full_name)
    });
}

/// Prevents generic fallback discovery from bypassing the authoritative feed.
pub fn runtime_allows_model(
    config: &crate::config::Config,
    model: &crate::model_config::ModelConfig,
) -> bool {
    let Some(feed) = cached().filter(|_| is_authoritative(config)) else {
        return true;
    };
    model.provider != feed.provider
        || endpoint_is_offered(&feed, model.model_type, &model.full_name)
}

/// What the publisher verified this endpoint on.
///
/// Maps only general-purpose modalities into generic priority chains.
///
/// Schema-1 silence remains text for compatibility, but a dedicated capability
/// is never promoted merely because its endpoint happens to use an LLM. The
/// identity check also protects clients from older feeds that mislabeled a
/// dedicated translator as generic text.
fn feed_modality(model: &super::FeedModel) -> Option<ModelType> {
    if dedicated_translation_endpoint(&model.id) {
        return None;
    }
    match model.modality.as_deref() {
        Some("vision") => Some(ModelType::Vision),
        Some("text") | None => Some(ModelType::Text),
        Some(_) => None,
    }
}

fn dedicated_translation_endpoint(id: &str) -> bool {
    id.split(|character: char| !character.is_ascii_alphanumeric())
        .any(|token| matches!(token, "translate" | "translation" | "translator"))
}

/// Resolves a feed endpoint to its curated id or stable discovered identity.
/// A reviewed withdrawal remains excluded; operational enablement is owned by
/// the signed offer itself rather than a release-time `enabled` snapshot.
fn catalog_id_for(provider: &str, full_name: &str, model_type: ModelType) -> Option<String> {
    if crate::model_config::is_withdrawn_endpoint(provider, full_name) {
        return None;
    }
    let known = crate::model_config::get_all_models().iter().find(|model| {
        model.provider == provider && model.full_name == full_name && model.model_type == model_type
    });
    Some(known.map_or_else(
        || discovered_id(provider, full_name),
        |model| model.id.clone(),
    ))
}

/// Whether the catalog has an opinion about this endpoint at all.
///
/// A known row keeps its stable identity and presentation. Only the explicit
/// withdrawn-endpoint registry is a human-reviewed veto; release-time enabled
/// state does not override a newer signed NVIDIA availability decision.
#[cfg(not(feature = "recorder-worker"))]
fn catalog_knows(provider: &str, full_name: &str, model_type: ModelType) -> bool {
    crate::model_config::is_withdrawn_endpoint(provider, full_name)
        || crate::model_config::get_all_models().iter().any(|model| {
            model.provider == provider
                && model.full_name == full_name
                && model.model_type == model_type
        })
}

#[cfg(all(test, not(feature = "recorder-worker")))]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_healthy_model_routes_through_its_discovered_identity() {
        assert_eq!(
            catalog_id_for("nvidia", "nvidia/not-in-this-build", ModelType::Text),
            Some(discovered_id("nvidia", "nvidia/not-in-this-build"))
        );
    }

    #[test]
    fn discovered_names_are_short_provider_marked_and_deterministic() {
        assert_eq!(
            crate::model_config::compact_provider_endpoint_name(
                "nvidia",
                "nvidia/nemotron-mini-4b-instruct"
            ),
            "N nm4i"
        );
    }

    #[test]
    fn a_known_model_resolves_to_its_catalog_row() {
        // Wired in the same change that added the feed, so this also guards the
        // pairing between the published name and the catalog row.
        assert_eq!(
            catalog_id_for(
                "nvidia",
                "nvidia/nemotron-3.5-lightning-30b-a3b",
                ModelType::Text,
            )
            .as_deref(),
            Some("nvidia-nemotron-3-5-lightning-text")
        );
    }

    #[test]
    fn a_feed_modality_cannot_reuse_another_modalitys_catalog_id() {
        let endpoint = "openai/gpt-oss-120b";
        assert_eq!(
            catalog_id_for("nvidia", endpoint, ModelType::Text).as_deref(),
            Some("nvidia-gpt-oss-120b-text")
        );
        assert_eq!(
            catalog_id_for("nvidia", endpoint, ModelType::Vision),
            Some(discovered_id("nvidia", endpoint))
        );

        let feed = AvailabilityFeed {
            schema_version: 3,
            control_version: 1,
            availability_gate_version: 1,
            provider: "nvidia".to_string(),
            generated_at: "2026-08-28T00:00:00Z".to_string(),
            models: vec![crate::model_feed::FeedModel {
                id: endpoint.to_string(),
                control: Some(super::super::FeedControl::Plain),
                modality: Some("vision".to_string()),
                p50_ms: Some(1_063),
                success_rate: 0.33,
                runs: 24,
            }],
        };
        let discovered = discovered_models_from_feed(&feed);
        assert_eq!(discovered.len(), 1);
        assert_eq!(discovered[0].model_type, ModelType::Vision);
        assert_eq!(discovered[0].name_vi, "N go1");
        assert_eq!(discovered[0].typical_latency_ms, Some(1_063));
    }

    #[test]
    fn a_provider_the_user_has_not_enabled_offers_nothing() {
        let config = crate::config::Config {
            use_nvidia: false,
            ..Default::default()
        };
        assert!(offered_models(&config, ModelType::Text).is_empty());
    }

    #[test]
    fn signed_offer_removes_stale_nvidia_rows_without_touching_other_providers() {
        let feed = AvailabilityFeed {
            schema_version: 3,
            control_version: 1,
            availability_gate_version: 1,
            provider: "nvidia".to_string(),
            generated_at: "2026-08-28T00:00:00Z".to_string(),
            models: vec![crate::model_feed::FeedModel {
                id: "nvidia/nemotron-3.5-lightning-30b-a3b".to_string(),
                control: Some(super::super::FeedControl::Plain),
                modality: Some("text".to_string()),
                p50_ms: Some(300),
                success_rate: 0.5,
                runs: 3,
            }],
        };
        let configured = vec![
            "groq-qwen-3-8-27b-text".to_string(),
            "nvidia-nemotron-3-5-lightning-text".to_string(),
            "nvidia-nemotron-3-super-120b-text".to_string(),
        ];

        assert_eq!(
            reconcile_configured_chain_from_feed(&feed, ModelType::Text, &configured, &[]),
            [
                "groq-qwen-3-8-27b-text",
                "nvidia-nemotron-3-5-lightning-text"
            ]
        );

        let mut selectors = crate::model_config::get_all_models().to_vec();
        project_provider_inventory(&mut selectors, &feed);
        assert!(selectors.iter().any(|model| model.provider == "groq"));
        assert!(
            selectors
                .iter()
                .any(|model| model.id == "nvidia-nemotron-3-5-lightning-text")
        );
        assert!(
            selectors
                .iter()
                .all(|model| model.id != "nvidia-nemotron-3-super-120b-text")
        );
    }
}
