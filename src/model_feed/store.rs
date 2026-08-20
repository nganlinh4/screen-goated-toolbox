//! Fetching, caching and applying the availability feed.
//!
//! The feed is optional infrastructure: every failure path here leaves the app
//! exactly as it was. A missing feed, an unreachable host, a signature that does
//! not verify, or a schema this build does not understand all resolve to "no
//! offered models", never to a partial application.

use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Result, bail};

use super::{AvailabilityFeed, parse_verified};

const FEED_URL: &str = "https://raw.githubusercontent.com/nganlinh4/screen-goated-toolbox/main/monitoring/nvidia-availability.json";
const SIGNATURE_URL: &str = "https://raw.githubusercontent.com/nganlinh4/screen-goated-toolbox/main/monitoring/nvidia-availability.json.sig";

/// The publisher regenerates every two hours; refreshing faster only spends the
/// user's network for a file that has not changed.
const REFRESH_INTERVAL: Duration = Duration::from_secs(2 * 60 * 60);

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

/// The cached feed, if one is present and still verifies.
///
/// Verification is repeated on every load rather than trusted from the time it
/// was written, so a cache file edited on disk cannot influence routing.
pub fn cached() -> Option<AvailabilityFeed> {
    let payload = std::fs::read(feed_path()).ok()?;
    let signature = std::fs::read(signature_path()).ok()?;
    if payload.len() > MAX_FEED_BYTES || signature.len() != SIGNATURE_BYTES {
        return None;
    }
    parse_verified(&payload, &signature).ok()
}

/// Whether the cache is old enough to be worth refetching.
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
    Ok(feed)
}

/// Refreshes the cached feed in the background when it has gone stale.
///
/// Startup never waits on this and a failure is only logged: the feed is an
/// optimisation, and the configured chain works without it.
pub fn refresh_in_background() {
    if !is_stale() {
        return;
    }
    std::thread::spawn(|| match refresh() {
        Ok(feed) => crate::log_info!(
            "[Model feed] {} availability refreshed, {} offered",
            feed.provider,
            super::ranked_models(&feed).len()
        ),
        Err(error) => crate::log_info!("[Model feed] Availability refresh skipped: {error:#}"),
    });
}

/// Model ids the feed currently offers, best first.
///
/// Only ids for a provider the user has enabled and holds a credential for are
/// returned: an offer the user cannot use would only lengthen the chain with
/// entries that must fail before anything else is tried.
pub fn offered_ids(config: &crate::config::Config) -> Vec<String> {
    let Some(feed) = cached() else {
        return Vec::new();
    };
    if !crate::retry_model_chain::provider_is_available(&feed.provider, config) {
        return Vec::new();
    }
    super::ranked_models(&feed)
        .into_iter()
        .filter_map(|model| catalog_id_for(&feed.provider, &model.id))
        .collect()
}

/// Resolves a feed's provider-qualified name to a catalog row.
///
/// The feed can only surface models this build already knows how to talk to.
/// A name with no catalog row is ignored rather than synthesised, because the
/// request shape, reasoning control and output ceiling all live in the catalog.
fn catalog_id_for(provider: &str, full_name: &str) -> Option<String> {
    crate::model_config::get_all_models()
        .iter()
        .find(|model| model.provider == provider && model.full_name == full_name && model.enabled)
        .map(|model| model.id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_model_name_is_ignored_rather_than_invented() {
        assert!(catalog_id_for("nvidia", "nvidia/not-in-this-build").is_none());
    }

    #[test]
    fn a_known_model_resolves_to_its_catalog_row() {
        // Wired in the same change that added the feed, so this also guards the
        // pairing between the published name and the catalog row.
        assert_eq!(
            catalog_id_for("nvidia", "nvidia/nemotron-3.5-lightning-30b-a3b").as_deref(),
            Some("nvidia-nemotron-3-5-lightning-text")
        );
    }

    #[test]
    fn a_provider_the_user_has_not_enabled_offers_nothing() {
        let config = crate::config::Config {
            use_nvidia: false,
            ..Default::default()
        };
        assert!(offered_ids(&config).is_empty());
    }
}
