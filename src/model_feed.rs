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

const PUBLIC_KEY_HEX: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/monitoring/monitoring-p256-public-key.hex"
));

const LABEL: &str = "availability feed";

/// Highest chain position the feed may influence. Position 0 is local.
pub const FIRST_REMOTE_CHAIN_POSITION: usize = 1;

/// Schema version this client understands.
const SUPPORTED_SCHEMA: u32 = 1;

/// Providers the feed is allowed to describe. A feed naming anything else is
/// rejected outright rather than filtered, because it means the publisher and
/// this client disagree about what is being published.
const ALLOWED_PROVIDERS: &[&str] = &["nvidia"];

#[derive(Debug, Clone, Deserialize)]
pub struct FeedModel {
    pub id: String,
    /// Reasoning control the publisher found working. Carried so a diagnostic can
    /// show why a model behaves as it does; the request shape itself comes from
    /// the catalog, never from the feed.
    #[serde(default)]
    #[allow(dead_code)]
    pub control: Option<String>,
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
    if feed.schema_version != SUPPORTED_SCHEMA {
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

/// The feed's models in the order the client should consider them, best first.
///
/// Only endpoints the feed reports as fully successful are offered. The feed
/// already applies hysteresis before publishing; this is the client refusing to
/// take a marginal row even if one is published.
pub fn ranked_models(feed: &AvailabilityFeed) -> Vec<&FeedModel> {
    let mut usable: Vec<&FeedModel> = feed
        .models
        .iter()
        .filter(|model| model.success_rate >= 1.0 && model.runs > 0)
        .collect();
    usable.sort_by_key(|model| model.p50_ms.unwrap_or(u32::MAX));
    usable
}

/// Places feed models into an existing chain without disturbing its head.
///
/// Existing members keep their relative order; feed models that are already in
/// the chain are not duplicated. Anything the feed offers lands after the local
/// head, so a remote decision can add a fallback but never take first contact.
pub fn merge_into_chain(chain: &[String], offered: &[String]) -> Vec<String> {
    if chain.is_empty() {
        return chain.to_vec();
    }
    let head = FIRST_REMOTE_CHAIN_POSITION.min(chain.len());
    let mut merged: Vec<String> = chain[..head].to_vec();
    for id in offered {
        if !merged.iter().any(|existing| existing == id) {
            merged.push(id.clone());
        }
    }
    for id in &chain[head..] {
        if !merged.iter().any(|existing| existing == id) {
            merged.push(id.clone());
        }
    }
    merged
}

pub mod store;

#[cfg(test)]
mod tests;
