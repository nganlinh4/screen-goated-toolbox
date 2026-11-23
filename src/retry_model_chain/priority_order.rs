use crate::config::types::LiveModelOverrides;
use crate::model_feed::{CandidateRank, merge_into_chain_with_overrides};
use std::collections::HashSet;

use super::{RetryChainKind, resolve_auto_retry_model, resolve_configured_candidates};
use crate::{config::Config, model_config::ModelConfig};

pub(crate) const CHOSEN_MODEL_ROW: &str = "@chosen-model";

impl RetryChainKind {
    pub(crate) fn chosen_after(self, config: &Config) -> usize {
        match self {
            Self::ImageToText => config.model_priority_chains.image_chosen_after,
            Self::TextToText => config.model_priority_chains.text_chosen_after,
        }
    }
}

pub(super) fn merge_priority_rows(
    mut rows: Vec<String>,
    offered: &[String],
    overrides: &LiveModelOverrides,
    protected_leaders: usize,
    rank_for: impl Fn(&str) -> CandidateRank,
) -> Vec<String> {
    let Some(index) = rows.iter().position(|id| id == CHOSEN_MODEL_ROW) else {
        return merge_into_chain_with_overrides(
            &rows,
            offered,
            &overrides.pinned,
            &overrides.excluded,
            protected_leaders,
            rank_for,
        );
    };
    rows.remove(index);
    let prefix = &rows[..index];
    // An explicit first-attempt override owns its prefix. Live still ranks the
    // rest of the chain, using the ordinary count of protected model rows.
    let mut pinned = overrides.pinned.clone();
    pinned.extend_from_slice(prefix);
    let mut excluded = overrides.excluded.clone();
    excluded.retain(|id| !prefix.contains(id));
    let mut merged = merge_into_chain_with_overrides(
        &rows,
        offered,
        &pinned,
        &excluded,
        protected_leaders.max(index),
        rank_for,
    );
    merged.insert(index.min(merged.len()), CHOSEN_MODEL_ROW.to_string());
    merged
}

pub(crate) fn editable_priority_order(mut rows: Vec<String>, after: usize) -> Vec<String> {
    let index = if after <= rows.len() { after } else { 0 };
    rows.insert(index, CHOSEN_MODEL_ROW.to_string());
    rows
}

pub(crate) fn split_priority_order(mut rows: Vec<String>) -> (Vec<String>, usize) {
    let index = rows.iter().position(|row| row == CHOSEN_MODEL_ROW);
    if let Some(index) = index {
        rows.remove(index);
    }
    (rows, index.unwrap_or(0))
}

pub(crate) fn normalized_chosen_after(rows: &[String], after: usize) -> usize {
    let mut seen = HashSet::new();
    rows.get(..after)
        .unwrap_or_default()
        .iter()
        .filter(|id| !id.trim().is_empty() && seen.insert(id.as_str()))
        .count()
}

fn expand_chosen_model(rows: Vec<String>, selected: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let (models, after) = split_priority_order(rows);
    editable_priority_order(models, after)
        .into_iter()
        .filter_map(|row| {
            if row == CHOSEN_MODEL_ROW {
                Some(selected.to_string())
            } else {
                (row != selected).then_some(row)
            }
        })
        .filter(|id| seen.insert(id.clone()))
        .collect()
}

/// Snapshot the opt-in attempt order for a preset block after its saved model
/// has resolved. Legacy settings still use the normal selected-first path.
pub(crate) struct PresetRetryOrder {
    kind: RetryChainKind,
    ordered: Option<Vec<String>>,
}

impl PresetRetryOrder {
    pub(crate) fn new(kind: RetryChainKind, selected: &str, config: &Config) -> Self {
        let rows = kind.configured_chain(config);
        let after = kind.chosen_after(config);
        let ordered = (after > 0 && after <= rows.len()).then(|| {
            let editable = editable_priority_order(rows.to_vec(), after);
            let effective = if kind.adaptive_enabled(config) {
                kind.adaptive_chain(config, &editable, kind.live_overrides(config))
            } else {
                editable
            };
            expand_chosen_model(effective, selected)
        });
        Self { kind, ordered }
    }

    pub(crate) fn before_chosen(
        &self,
        selected: &str,
        search_enabled: bool,
        config: &Config,
    ) -> Option<ModelConfig> {
        let ordered = self.ordered.as_ref()?;
        let index = ordered.iter().position(|id| id == selected)?;
        resolve_configured_candidates(
            &ordered[..index],
            selected,
            &[],
            &HashSet::new(),
            self.kind,
            search_enabled,
            config,
        )
    }

    pub(crate) fn next(
        &self,
        current: &str,
        failed: &[String],
        blocked: &HashSet<String>,
        search_enabled: bool,
        config: &Config,
    ) -> Option<ModelConfig> {
        let Some(ordered) = self.ordered.as_ref() else {
            return super::resolve_next_retry_model(
                current,
                failed,
                blocked,
                self.kind,
                search_enabled,
                config,
            );
        };
        resolve_configured_candidates(
            ordered,
            current,
            failed,
            blocked,
            self.kind,
            search_enabled,
            config,
        )
        .or_else(|| {
            resolve_auto_retry_model(current, failed, blocked, self.kind, search_enabled, config)
        })
    }
}

#[cfg(all(test, not(feature = "recorder-worker")))]
mod tests {
    use super::*;

    fn fixture() -> serde_json::Value {
        serde_json::from_str(include_str!(
            "../../parity-fixtures/preset-system/preset-attempt-order.json"
        ))
        .unwrap()
    }

    #[test]
    fn chosen_model_insertion_matches_shared_order() {
        for case in fixture()["cases"].as_array().unwrap() {
            let rows = serde_json::from_value(case["rows"].clone()).unwrap();
            let expected: Vec<String> = serde_json::from_value(case["attempts"].clone()).unwrap();
            assert_eq!(
                expand_chosen_model(rows, case["selected"].as_str().unwrap()),
                expected
            );
        }
    }

    #[test]
    fn live_offer_stays_below_the_explicit_prefix() {
        let fixture = fixture();
        let case = &fixture["live"];
        let rows = serde_json::from_value(case["rows"].clone()).unwrap();
        let offered: Vec<String> = serde_json::from_value(case["offered"].clone()).unwrap();
        let expected: Vec<String> = serde_json::from_value(case["order"].clone()).unwrap();
        assert_eq!(
            merge_priority_rows(rows, &offered, &LiveModelOverrides::default(), 2, |id| {
                CandidateRank {
                    quality_tier: 4,
                    latency_ms: if id == "live-auto" { 100 } else { 2000 },
                }
            }),
            expected
        );
    }

    #[test]
    fn preset_attempts_follow_the_moved_row_and_respect_request_compatibility() {
        let fixture = fixture();
        let case = &fixture["runtime"];
        let selected = case["selected"].as_str().unwrap();
        let first = case["first"].as_str().unwrap();
        let last = case["last"].as_str().unwrap();
        let mut config = Config {
            api_key: "test-groq".into(),
            gemini_api_key: "test-gemini".into(),
            use_groq: true,
            use_gemini: true,
            use_openrouter: false,
            ..Default::default()
        };
        config.adaptive_model_priority.text_to_text = false;
        (
            config.model_priority_chains.text_to_text,
            config.model_priority_chains.text_chosen_after,
        ) = split_priority_order(serde_json::from_value(case["rows"].clone()).unwrap());
        let order = PresetRetryOrder::new(RetryChainKind::TextToText, selected, &config);
        assert_eq!(
            order.before_chosen(selected, false, &config).unwrap().id,
            first
        );
        assert!(order.before_chosen(selected, true, &config).is_none());
        let blocked = HashSet::new();
        let failed = [first.to_string()];
        assert_eq!(
            order
                .next(first, &failed, &blocked, false, &config)
                .unwrap()
                .id,
            selected
        );
        assert_eq!(
            order
                .next(selected, &failed, &blocked, false, &config)
                .unwrap()
                .id,
            last
        );
    }
}
