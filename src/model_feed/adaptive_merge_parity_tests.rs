//! Replays `parity-fixtures/preset-system/adaptive-merge.json`, the shared case
//! vectors that Android's `mergeAdaptiveModels` must reproduce.

use super::{CandidateRank, merge_into_chain_with_overrides};
use serde_json::Value;

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap().to_string())
        .collect()
}

#[test]
fn shared_adaptive_merge_cases_match_the_canonical_chain_merge() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../parity-fixtures/preset-system/adaptive-merge.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let ranks = case["ranks"].as_object().unwrap();
        let rank_for = |id: &str| {
            let rank = ranks
                .get(id)
                .unwrap_or_else(|| panic!("fixture rank missing for {id}"));
            CandidateRank {
                quality_tier: u8::try_from(rank[0].as_u64().unwrap()).unwrap(),
                latency_ms: u32::try_from(rank[1].as_u64().unwrap()).unwrap(),
            }
        };
        let merged = merge_into_chain_with_overrides(
            &strings(&case["chain"]),
            &strings(&case["offered"]),
            &strings(&case["pinned"]),
            &strings(&case["excluded"]),
            case["protectedLocalLeaders"].as_u64().unwrap() as usize,
            rank_for,
        );
        assert_eq!(merged, strings(&case["expected"]), "{}", case["name"]);
    }
}
