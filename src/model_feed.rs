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
//! - it may offer live candidates below the chain's protected authored prefix;
//! - it may never displace that prefix: four text or two image choices remain
//!   ahead of automatic remote offers when the configured chain contains them.
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
use chrono::{DateTime, TimeDelta, Utc};
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

/// How long after publication a signed feed may still steer routing.
///
/// The publisher runs every two hours. A feed this old means publication has
/// stopped, so its verdicts no longer describe the live provider; the client
/// then behaves exactly as when no feed exists and keeps the configured chain.
const MAX_FEED_AGE_HOURS: i64 = 24;

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
    /// RFC 3339 publication time. It bounds how long the feed may be acted on
    /// and orders feeds so an older signed publication cannot replace a newer one.
    #[serde(rename = "generatedAt")]
    pub generated_at: String,
    #[serde(default)]
    pub models: Vec<FeedModel>,
}

impl AvailabilityFeed {
    /// The publication instant, when `generatedAt` is a valid RFC 3339 time.
    pub fn published_at(&self) -> Option<DateTime<Utc>> {
        DateTime::parse_from_rfc3339(&self.generated_at)
            .ok()
            .map(|published| published.with_timezone(&Utc))
    }

    /// Whether the feed was published recently enough to act on at `now`.
    pub fn is_current_at(&self, now: DateTime<Utc>) -> bool {
        self.published_at().is_some_and(|published| {
            now.signed_duration_since(published) <= TimeDelta::hours(MAX_FEED_AGE_HOURS)
        })
    }
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
    if feed.published_at().is_none() {
        bail!(
            "{LABEL} publication time is not RFC 3339: {}",
            feed.generated_at
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
        if feed.schema_version > 1 && (model.runs == 0 || model.p50_ms.is_none()) {
            bail!("{LABEL} offered model has no measurement: {}", model.id);
        }
    }
    Ok(())
}

/// The feed's models in the order the client should consider them, best first.
///
/// Publication is the admission decision. The signed publisher has already
/// applied the versioned availability gate and hysteresis; applying a second
/// client-side threshold would make two clients disagree about the same feed.
pub fn ranked_models(feed: &AvailabilityFeed) -> Vec<&FeedModel> {
    let mut usable: Vec<&FeedModel> = feed.models.iter().collect();
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

    /// The one total order every candidate ranking uses, lowest first:
    /// quality-adjusted cost, then the higher tier, then the lower latency.
    pub fn preference_key(self) -> (u32, std::cmp::Reverse<u8>, u32) {
        (
            self.priority_cost(),
            std::cmp::Reverse(self.quality_tier),
            self.latency_ms,
        )
    }

    fn outranks_or_ties(self, other: Self) -> bool {
        self.preference_key() <= other.preference_key()
    }
}

/// Interleaves adaptive candidates while keeping the configured protected
/// leaders and the relative order of every non-adaptive fallback intact.
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
    merge_into_chain_with_overrides(chain, offered, &[], &[], 2, rank_for)
}

pub fn protected_local_leaders(model_type: crate::model_config::ModelType) -> usize {
    match model_type {
        crate::model_config::ModelType::Text => 4,
        _ => 2,
    }
}

pub fn merge_into_chain_with_overrides(
    chain: &[String],
    offered: &[String],
    pinned: &[String],
    excluded: &[String],
    protected_local_leaders: usize,
    rank_for: impl Fn(&str) -> CandidateRank,
) -> Vec<String> {
    if chain.is_empty() {
        return chain.to_vec();
    }
    // Resolving a rank can mean a catalog or feed lookup, so each id is ranked
    // once per merge instead of on every comparison.
    let mut ranks = std::collections::HashMap::<String, CandidateRank>::new();
    let mut rank_of = |id: &str| -> CandidateRank {
        if let Some(rank) = ranks.get(id) {
            return *rank;
        }
        let rank = rank_for(id);
        ranks.insert(id.to_owned(), rank);
        rank
    };
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
    adaptive.sort_by_cached_key(|id| rank_of(id).preference_key());
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
    if merged.len() >= protected_local_leaders {
        for id in adaptive {
            if merged.iter().any(|existing| existing == id) {
                continue;
            }
            let candidate_rank = rank_of(id);
            let insert_at = merged
                .iter()
                .enumerate()
                .skip(protected_local_leaders.max(1))
                .find(|(_, existing)| !rank_of(existing).outranks_or_ties(candidate_rank))
                .map(|(index, _)| index)
                .unwrap_or(merged.len());
            merged.insert(insert_at, id.clone());
        }
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
    // Restoring a pin can pull an automatic row above the protected boundary.
    // Keep pin slots fixed and fill every other leading slot with authored rows.
    let is_automatic = |id: &String| {
        id != protected_head && !is_pinned(id) && offered.iter().any(|offered| offered == id)
    };
    let leading_slots = merged
        .iter()
        .take(protected_local_leaders)
        .filter(|id| !is_pinned(id))
        .count();
    let leading: Vec<usize> = merged
        .iter()
        .enumerate()
        .filter(|(_, id)| !is_pinned(id) && !is_automatic(id))
        .take(leading_slots)
        .map(|(index, _)| index)
        .collect();
    if leading.len() < leading_slots {
        // A shortened chain cannot fill the protected prefix around distant pins.
        merged.retain(|id| !is_automatic(id));
        return merged;
    }
    let mut unpinned = leading
        .iter()
        .map(|index| &merged[*index])
        .chain(
            merged
                .iter()
                .enumerate()
                .filter(|(index, id)| !is_pinned(id) && !leading.contains(index))
                .map(|(_, id)| id),
        )
        .cloned()
        .collect::<Vec<_>>()
        .into_iter();
    for id in &mut merged {
        if !is_pinned(id) {
            *id = unpinned.next().expect("one value per unpinned slot");
        }
    }
    merged
}

#[path = "model_feed/store.rs"]
pub mod store;

#[cfg(all(test, not(feature = "recorder-worker")))]
#[path = "model_feed/tests.rs"]
mod tests;

#[cfg(all(test, not(feature = "recorder-worker")))]
#[path = "model_feed/adaptive_merge_parity_tests.rs"]
mod adaptive_merge_parity_tests;
