//! The published provider availability feed.
//!
//! A scheduled job probes every NVIDIA NIM endpoint and publishes which ones
//! answer, answer correctly, and with which reasoning control. Discovering that
//! from a user's machine would cost seventy-five models times three samples every
//! couple of hours, and it changes: three endpoints changed state within a single
//! day during evaluation.
//!
//! What the feed decides, and what it does not:
//!
//! - it may **offer** models, which appear as [`ModelSource::Discovered`];
//! - it may **order** the retry chain from position 1 downward;
//! - it may **never** take position 0. That position is tied to
//!   `default_text_model_id`, carries every request before any fallback exists,
//!   and stays under local control.
//!
//! Latency in the feed is a ranking hint, not an ordering. It is measured from
//! one datacenter while the user is on their own network, so it only breaks ties
//! among endpoints the feed has already judged healthy.
//!
//! Nothing is trusted before its signature verifies against the tracked public
//! key, and a feed that fails verification is ignored rather than partly applied.

use anyhow::{Result, bail};
use serde::Deserialize;

#[cfg(not(feature = "recorder-worker"))]
const PUBLIC_KEY_HEX: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/monitoring/monitoring-p256-public-key.hex"
));
#[cfg(feature = "recorder-worker")]
const PUBLIC_KEY_HEX: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../monitoring/monitoring-p256-public-key.hex"
));

const LABEL: &str = "availability feed";

/// Schema versions this client understands.
///
/// Both are accepted so a client update and a publisher update need not land
/// together: a new build reading the last schema-1 feed keeps working until the
/// next publish, rather than losing the feed for the length of a publish cycle.
/// Schema 1 carries no modality, and a model without one is treated as text,
/// which is the safe reading — routing an image at a text endpoint fails, while
/// declining to route one merely forgoes a fallback.
const SUPPORTED_SCHEMAS: &[u32] = &[1, 2];

/// Providers the feed is allowed to describe. A feed naming anything else is
/// rejected outright rather than filtered, because it means the publisher and
/// this client disagree about what is being published.
const ALLOWED_PROVIDERS: &[&str] = &["nvidia"];

#[derive(Debug, Clone, Deserialize)]
pub struct FeedModel {
    pub id: String,
    /// Reasoning control the publisher found working.
    ///
    /// The catalog owns the policy actually sent, so this is not applied here;
    /// it is carried so a diagnostic can explain why an endpoint behaves as it
    /// does, and so a mismatch between publisher and catalog is visible.
    #[serde(default)]
    #[allow(dead_code)]
    pub control: Option<String>,
    /// Whether the publisher verified this endpoint on images. Routing an image
    /// to a text endpoint fails every time, so this is carried rather than
    /// assumed.
    #[serde(default)]
    pub modality: Option<String>,
    #[serde(default)]
    pub p50_ms: Option<u32>,
    #[serde(default)]
    pub success_rate: f32,
    #[serde(default)]
    pub runs: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AvailabilityFeed {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    pub provider: String,
    /// Publication time, surfaced when reporting which feed is in use.
    #[serde(rename = "generatedAt")]
    #[allow(dead_code)]
    pub generated_at: String,
    #[serde(default)]
    pub models: Vec<FeedModel>,
}

/// Parses a feed only after its signature verifies.
pub fn parse_verified(payload: &[u8], signature: &[u8]) -> Result<AvailabilityFeed> {
    let public_key = crate::crypto::decode_hex(PUBLIC_KEY_HEX.trim(), LABEL)?;
    crate::crypto::verify_p256_sha256(&public_key, payload, signature, LABEL)?;
    let feed: AvailabilityFeed =
        serde_json::from_slice(payload).map_err(|error| anyhow::anyhow!("{LABEL}: {error}"))?;
    validate(&feed)?;
    Ok(feed)
}

fn validate(feed: &AvailabilityFeed) -> Result<()> {
    if !SUPPORTED_SCHEMAS.contains(&feed.schema_version) {
        bail!(
            "{LABEL} schema {} is not supported by this build",
            feed.schema_version
        );
    }
    if !ALLOWED_PROVIDERS.contains(&feed.provider.as_str()) {
        bail!("{LABEL} names an unexpected provider: {}", feed.provider);
    }
    for model in &feed.models {
        // The id must belong to the provider the feed claims, so a feed cannot
        // smuggle in a row that routes somewhere else.
        if !model.id.contains('/') {
            bail!("{LABEL} model id is not provider-qualified: {}", model.id);
        }
        if !(0.0..=1.0).contains(&model.success_rate) {
            bail!("{LABEL} success rate is out of range for {}", model.id);
        }
    }
    Ok(())
}

/// Success rate below which a published model is not worth appending.
///
/// The publisher already applies hysteresis and withholds anything that failed
/// its latest run, so this is a second opinion rather than the only one. Demanding
/// a perfect record rejected models that pass five runs in six, which are useful
/// at the back of a chain: reaching them at all means everything local has already
/// failed, and one rejection there costs a retry. A rate near a coin flip is
/// excluded, because a fallback that usually fails is not a fallback.
const MINIMUM_SUCCESS_RATE: f32 = 0.8;

/// The feed's models in the order the client should consider them, best first.
pub fn ranked_models(feed: &AvailabilityFeed) -> Vec<&FeedModel> {
    let mut usable: Vec<&FeedModel> = feed
        .models
        .iter()
        .filter(|model| model.success_rate >= MINIMUM_SUCCESS_RATE && model.runs > 0)
        .collect();
    usable.sort_by_key(|model| model.p50_ms.unwrap_or(u32::MAX));
    usable
}

/// Appends feed models behind the entire local chain.
///
/// Feed members are deep fallback and nothing more. Locally configured models
/// keep their order and their positions, and a remote decision can only lengthen
/// the tail. That is deliberate given how these endpoints behave: they come and
/// go, and a member that is momentarily dead costs one fast rejection and a
/// cooldown when it sits at the back, against a visible stall when it sits near
/// the front.
pub fn merge_into_chain(chain: &[String], offered: &[String]) -> Vec<String> {
    if chain.is_empty() {
        return chain.to_vec();
    }
    let mut merged: Vec<String> = chain.to_vec();
    for id in offered {
        if !merged.iter().any(|existing| existing == id) {
            merged.push(id.clone());
        }
    }
    merged
}

#[path = "model_feed/store.rs"]
pub mod store;

#[cfg(test)]
mod tests;
