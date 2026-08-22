//! The published provider availability feed.
//!
//! A scheduled job probes every NVIDIA NIM endpoint and publishes which ones
//! answer, which general modality they accept, and with which reasoning control. Discovering that
//! from a user's machine would cost seventy-five models times three samples every
//! couple of hours, and it changes: three endpoints changed state within a single
//! day during evaluation.
//!
//! What the feed decides, and what it does not:
//!
//! - it may **offer** models, which appear as [`ModelSource::Discovered`];
//! - it may offer live candidates to the adaptive tail below position 0;
//! - it may never take position 0. That position is tied to
//!   `default_text_model_id`, carries every request before any fallback exists,
//!   and stays under local control.
//!
//! Feed eligibility is an operational reliability gate. Stable catalog quality
//! tiers and latency then form a bounded tradeoff for eligible models. A sample
//! from one preset never qualifies or disqualifies a whole generic catalog.
//! Latency comes from measured evidence but remains only a ranking hint because
//! the user's network can differ from the measurement environment.
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
/// Schema 3 separates operational availability from catalog quality. Schema 2
/// used preset-specific output checks as universal eligibility and is rejected.
/// Schema 1 remains readable only as the historical empty feed.
const SUPPORTED_SCHEMAS: &[u32] = &[1, 3];

/// Providers the feed is allowed to describe. A feed naming anything else is
/// rejected outright rather than filtered, because it means the publisher and
/// this client disagree about what is being published.
const ALLOWED_PROVIDERS: &[&str] = &["nvidia"];

const CONTROL_CONTRACT_VERSION: u32 = 1;
const AVAILABILITY_GATE_VERSION: u32 = 1;

fn default_control_contract_version() -> u32 {
    CONTROL_CONTRACT_VERSION
}

/// Versioned request controls whose exact wire meaning is shared with the
/// publisher. Adding or changing a control requires a new feed contract.
#[cfg(not(feature = "recorder-worker"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FeedControl {
    Plain,
    EffortNone,
    EffortLow,
    TemplateKwargs,
    NoThink,
    ThinkingOff,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FeedModel {
    pub id: String,
    /// Reasoning control the publisher found working, and the one actually sent.
    ///
    /// This overrides the catalog policy for the endpoint. The catalog fixes a
    /// policy when the build is cut; the monitor rediscovers it from the live
    /// endpoint every couple of hours, and sending a control an endpoint has
    /// stopped accepting turns a healthy model into HTTP 500.
    #[cfg(not(feature = "recorder-worker"))]
    #[serde(default)]
    pub control: Option<FeedControl>,
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
    #[serde(
        rename = "controlVersion",
        default = "default_control_contract_version"
    )]
    pub control_version: u32,
    #[serde(rename = "availabilityGateVersion", default)]
    pub availability_gate_version: u32,
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
    if feed.control_version != CONTROL_CONTRACT_VERSION {
        bail!(
            "{LABEL} reasoning-control contract {} is not supported by this build",
            feed.control_version
        );
    }
    if feed.schema_version == 1 {
        if !feed.models.is_empty() {
            bail!("{LABEL} legacy schema may not offer models");
        }
    } else if feed.availability_gate_version != AVAILABILITY_GATE_VERSION {
        bail!(
            "{LABEL} availability gate {} is not supported by this build",
            feed.availability_gate_version
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
/// The publisher already applies hysteresis and withholds anything unavailable
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

/// Comparable evidence used to place adaptive candidates without reordering the
/// user's configured rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CandidateRank {
    pub quality_tier: u8,
    pub latency_ms: u32,
}

impl CandidateRank {
    /// Lower is better. Catalog quality has six tiers; each tier step may justify
    /// up to 1.5x the latency. Speed remains dominant enough that a proven-fast
    /// fallback can displace a much slower higher-tier model.
    fn priority_cost(self) -> u32 {
        const HIGHEST_QUALITY_TIER: u8 = 6;
        let tier = self.quality_tier.clamp(1, HIGHEST_QUALITY_TIER);
        let distance = u32::from(HIGHEST_QUALITY_TIER - tier);
        let numerator = u64::from(self.latency_ms).saturating_mul(3_u64.pow(distance));
        let denominator = 2_u64.pow(distance);
        u32::try_from(numerator / denominator).unwrap_or(u32::MAX)
    }

    fn outranks_or_ties(self, other: Self) -> bool {
        self.priority_cost() < other.priority_cost()
            || (self.priority_cost() == other.priority_cost()
                && (self.quality_tier > other.quality_tier
                    || (self.quality_tier == other.quality_tier
                        && self.latency_ms <= other.latency_ms)))
    }
}

/// Interleaves adaptive candidates while keeping the configured head and the
/// relative order of every non-adaptive configured fallback intact.
///
/// Each candidate lands before the first slower or weaker configured fallback.
/// Enabling adaptation explicitly hands currently offered rows back to the
/// formula, even when a previous manual edit persisted them into the chain.
/// This makes re-enabling Live and later feed refreshes capable of restoring the
/// measured order instead of mistaking yesterday's live rows for fixed choices.
#[cfg(all(test, not(feature = "recorder-worker")))]
pub fn merge_into_chain(
    chain: &[String],
    offered: &[String],
    rank_for: impl Fn(&str) -> CandidateRank,
) -> Vec<String> {
    merge_into_chain_with_overrides(chain, offered, &[], &[], rank_for)
}

pub fn merge_into_chain_with_overrides(
    chain: &[String],
    offered: &[String],
    pinned: &[String],
    excluded: &[String],
    rank_for: impl Fn(&str) -> CandidateRank,
) -> Vec<String> {
    if chain.is_empty() {
        return chain.to_vec();
    }
    const MAX_ADAPTIVE_OFFERS: usize = 5;
    let protected_head = &chain[0];
    let is_pinned = |id: &String| {
        pinned.iter().any(|candidate| candidate == id)
            && chain.iter().any(|configured| configured == id)
    };
    let is_excluded = |id: &String| excluded.iter().any(|candidate| candidate == id);
    let mut adaptive: Vec<&String> = offered
        .iter()
        .filter(|id| *id != protected_head && !is_pinned(id) && !is_excluded(id))
        .collect();
    adaptive.sort_by(|left, right| {
        let left = rank_for(left);
        let right = rank_for(right);
        left.priority_cost()
            .cmp(&right.priority_cost())
            .then_with(|| right.quality_tier.cmp(&left.quality_tier))
            .then_with(|| left.latency_ms.cmp(&right.latency_ms))
    });
    adaptive.truncate(MAX_ADAPTIVE_OFFERS);

    let mut merged: Vec<String> = chain
        .iter()
        .enumerate()
        .filter(|(index, id)| {
            *index == 0
                || (!is_excluded(id)
                    && (is_pinned(id) || !offered.iter().any(|candidate| candidate == *id)))
        })
        .map(|(_, id)| id.clone())
        .collect();
    for id in adaptive {
        if merged.iter().any(|existing| existing == id) {
            continue;
        }
        let candidate_rank = rank_for(id);
        let insert_at = merged
            .iter()
            .enumerate()
            .skip(1)
            .find(|(_, existing)| !rank_for(existing).outranks_or_ties(candidate_rank))
            .map(|(index, _)| index)
            .unwrap_or(merged.len());
        merged.insert(insert_at, id.clone());
    }
    for (authored_index, pinned_id) in chain
        .iter()
        .enumerate()
        .skip(1)
        .filter(|(_, id)| is_pinned(id) && !is_excluded(id))
    {
        let Some(current_index) = merged.iter().position(|id| id == pinned_id) else {
            continue;
        };
        let pinned_id = merged.remove(current_index);
        let target_index = authored_index.min(merged.len());
        merged.insert(target_index, pinned_id);
    }
    merged
}

#[path = "model_feed/store.rs"]
pub mod store;

#[cfg(all(test, not(feature = "recorder-worker")))]
#[path = "model_feed/tests.rs"]
mod tests;
