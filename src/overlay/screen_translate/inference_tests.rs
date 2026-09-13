use super::*;
use crate::overlay::screen_translate::contract::NormalizedBounds;
use crate::overlay::screen_translate::contract::parse_response;
use crate::overlay::screen_translate::request::{completion_budget, context};
use crate::overlay::screen_translate::stream_parser::TranslationStreamParser;

fn candidate(id: u16, top: u16) -> DetectedTextRegion {
    DetectedTextRegion {
        id,
        bounds: NormalizedBounds {
            left: id.saturating_mul(200),
            top,
            right: id.saturating_mul(200).saturating_add(80),
            bottom: top + 20,
        },
        source_text: format!("source-{id}"),
        source_alternatives: vec![format!("source-{id}")],
        recognition: Default::default(),
        appearance: None,
    }
}

fn translated(candidate: &DetectedTextRegion) -> TranslationRegion {
    TranslationRegion {
        id: candidate.id,
        member_ids: vec![candidate.id],
        member_joins: Vec::new(),
        selections: vec![super::super::contract::TranslationSelection {
            region_id: candidate.id,
            candidate_id: format!("r{}c0", candidate.id),
            source_text: candidate.source_text.clone(),
            bounds: candidate.bounds,
        }],
        semantic_role: super::super::contract::SemanticRole::Standalone,
        source_text: candidate.source_text.clone(),
        translated_segments: vec![format!("translated-{}", candidate.id)],
        bounds: candidate.bounds,
        background_color: None,
        text_color: None,
    }
}

#[test]
fn retry_and_incremental_requests_retain_read_only_context() {
    let scene = vec![candidate(1, 20), candidate(2, 40), candidate(3, 60)];
    let accepted = vec![translated(&scene[0])];
    let mut prompt = String::new();
    context::append(&mut prompt, &scene, &scene[1..2], &[], &accepted).unwrap();
    let data: serde_json::Value = serde_json::from_str(prompt.lines().last().unwrap()).unwrap();
    let sources = data["surroundingSource"].as_array().unwrap();
    assert_eq!(sources.len(), 2);
    assert!(sources.iter().all(|source| source["sourceId"] != 2));
    assert_eq!(
        data["acceptedTranslations"][0]["translation"][0],
        "translated-1"
    );
    assert!(!prompt.contains("\"slot\""));
}

#[test]
fn scene_context_is_bounded_and_does_not_split_source_text() {
    let mut scene = (1..=200).map(|id| candidate(id, 0)).collect::<Vec<_>>();
    for region in &mut scene {
        region.source_text = "x".repeat(2_000);
    }
    let mut prompt = String::new();
    context::append(&mut prompt, &scene, &scene[..1], &[], &[]).unwrap();
    let data: serde_json::Value = serde_json::from_str(prompt.lines().last().unwrap()).unwrap();
    let sources = data["surroundingSource"].as_array().unwrap();
    assert_eq!(sources.len(), 3);
    assert!(
        sources
            .iter()
            .all(|source| source["text"].as_str().unwrap().len() == 2_000)
    );
}

#[test]
fn terminology_context_deduplicates_without_removing_requested_slots() {
    let scene = vec![candidate(1, 20), candidate(2, 40)];
    let previous = translated(&scene[0]);
    let mut invariant = translated(&scene[1]);
    invariant.translated_segments = vec![invariant.source_text.clone()];
    let prior = vec![previous.clone(), previous, invariant];
    let mut prompt = String::new();
    context::append(&mut prompt, &scene, &scene[1..], &prior, &[]).unwrap();
    let data: serde_json::Value = serde_json::from_str(prompt.lines().last().unwrap()).unwrap();
    assert_eq!(data["acceptedTranslations"].as_array().unwrap().len(), 1);
    assert_eq!(data["surroundingSource"].as_array().unwrap().len(), 1);
    assert!(prompt.contains("every requested slot"));
}

#[test]
fn retry_requests_only_missing_regions_and_keeps_committed_output() {
    let candidates = vec![candidate(1, 80), candidate(2, 20)];
    let mut accepted = vec![translated(&candidates[0])];
    let mut covered = HashSet::from([1]);

    assert_eq!(
        pending_candidates(&candidates, &covered)
            .iter()
            .map(|candidate| candidate.id)
            .collect::<Vec<_>>(),
        vec![2]
    );
    assert!(completed_document(&candidates, &accepted, &covered).is_none());

    assert!(accept_region(
        &mut accepted,
        &mut covered,
        translated(&candidates[1]),
        &candidates,
    ));
    let completed = completed_document(&candidates, &accepted, &covered).unwrap();
    assert_eq!(
        completed
            .regions
            .iter()
            .map(|region| region.id)
            .collect::<Vec<_>>(),
        vec![2, 1]
    );
}

#[test]
fn malformed_stream_member_cannot_erase_valid_regions_before_fallback() {
    let candidates = vec![candidate(1, 20), candidate(2, 40), candidate(3, 60)];
    let mut accepted = Vec::new();
    let mut covered = HashSet::new();
    let mut first_attempt = TranslationStreamParser::new(&candidates);
    for (_, region) in first_attempt
        .push(r#"{"translations":[{"slot":0,"translation":"first"},{"slot":1,"translation":3}]}"#)
    {
        accept_region(&mut accepted, &mut covered, region, &candidates);
    }

    let pending = pending_candidates(&candidates, &covered);
    assert_eq!(
        pending
            .iter()
            .map(|candidate| candidate.id)
            .collect::<Vec<_>>(),
        vec![2, 3]
    );
    assert_eq!(accepted[0].translated_segments, ["first"]);

    let mut fallback = TranslationStreamParser::new(&pending);
    for (_, region) in fallback.push(
        r#"{"translations":[{"slot":0,"translation":"second"},{"slot":1,"translation":"third"}]}"#,
    ) {
        accept_region(&mut accepted, &mut covered, region, &candidates);
    }

    let completed = completed_document(&candidates, &accepted, &covered).unwrap();
    assert_eq!(completed.regions.len(), 3);
    assert_eq!(completed.regions[0].translated_segments, ["first"]);
}

#[test]
fn completion_requires_every_detected_member() {
    let candidates = (1..=19)
        .map(|id| candidate(id, id.saturating_mul(10)))
        .collect::<Vec<_>>();
    let accepted = candidates[..18].iter().map(translated).collect::<Vec<_>>();
    let covered = (1..=18).collect::<HashSet<_>>();
    assert!(completed_document(&candidates, &accepted, &covered).is_none());
}

#[test]
fn incomplete_envelope_only_completes_after_every_owned_value_is_validated() {
    let candidates = vec![candidate(1, 20), candidate(2, 40)];
    let mut parser = TranslationStreamParser::new(&candidates);
    let mut accepted = Vec::new();
    let mut covered = HashSet::new();
    for chunk in [r#"{"translations":{"0":"first","1":"sec"#, r#"ond"}"#] {
        for (_, region) in parser.push(chunk) {
            accept_region(&mut accepted, &mut covered, region, &candidates);
        }
        assert_eq!(
            completed_document(&candidates, &accepted, &covered).is_some(),
            covered.len() == 2
        );
    }
    assert_eq!(covered.len(), 2);
    assert_eq!(accepted[1].translated_segments, ["second"]);
    assert!(parse_response(r#"{"translations":{"0":"first","1":"second"}"#, &candidates).is_err());
}

#[test]
fn complete_stream_retains_the_same_copied_batch_confirmation() {
    let candidates = vec![candidate(1, 20), candidate(2, 40)];
    for (first, copied) in [("source-1", true), ("translated", false)] {
        let response = format!(r#"{{"translations":{{"0":"{first}","1":"source-2"}}"#);
        let mut parser = TranslationStreamParser::new(&candidates);
        let regions = parser
            .push(&response)
            .into_iter()
            .map(|(_, region)| region)
            .collect::<Vec<_>>();
        assert!(completed_response(&response, &candidates, regions[..1].to_vec()).is_err());
        let document = completed_response(&response, &candidates, regions).unwrap();
        assert_eq!(
            super::super::translation_validation::is_copied_batch(&document.regions),
            copied
        );
        assert_eq!(document.regions.len(), 2);
    }
}

#[test]
fn exhausted_chain_keeps_real_results_without_fabricating_translations() {
    let candidates = vec![candidate(1, 20), candidate(2, 40)];
    let accepted = vec![translated(&candidates[0])];
    let covered = HashSet::from([1]);
    let outcome = unresolved_outcome(&candidates, &accepted, &covered);
    assert_eq!(outcome.unresolved, [2]);
    assert_eq!(outcome.document.regions, accepted);
    assert!(outcome.warning().is_some());
    assert!(completed_document(&candidates, &accepted, &covered).is_none());
}

#[test]
fn short_structured_answers_leave_room_for_required_reasoning() {
    let candidates = vec![candidate(1, 20)];
    assert_eq!(completion_budget(&candidates, false), 256);
    assert_eq!(completion_budget(&candidates, true), 1024);
    let mut large = candidates;
    large[0].source_text = "text".repeat(10000);
    assert_eq!(completion_budget(&large, true), 8192);
}

#[test]
fn similarity_is_not_a_translation_failure() {
    for (source, output) in [
        (
            "Example Extended Product Name and",
            "Example Extended Product Name và",
        ),
        (
            "Example Extended Product Name",
            "Example Extended Product Name",
        ),
        (
            "This text is already in the target language",
            "This text is already in the target language",
        ),
        ("文字列の表示設定", "文字列の表示設定"),
        ("version 123.456", "versión 123.456"),
    ] {
        let mut input = candidate(1, 0);
        input.source_text = source.into();
        let mut region = translated(&input);
        region.translated_segments = vec![output.into()];
        let mut accepted = Vec::new();
        let mut covered = HashSet::new();
        assert!(accept_region(&mut accepted, &mut covered, region, &[input]));
        assert_eq!(accepted[0].translated_segments, [output]);
    }
}

#[test]
fn echo_observation_requires_distinct_text_not_high_overlap() {
    let mut first = translated(&candidate(1, 0));
    first.source_text = "Example Extended Product Name and".into();
    first.translated_segments = vec!["Example Extended Product Name và".into()];
    assert!(!super::super::translation_validation::is_copied_batch(&[
        first.clone()
    ]));
    first.translated_segments = vec![first.source_text.clone()];
    assert!(!super::super::translation_validation::is_copied_batch(&[
        first.clone(),
        first.clone()
    ]));
    let mut second = first.clone();
    second.source_text = "Another label".into();
    second.translated_segments = vec![second.source_text.clone()];
    assert!(super::super::translation_validation::is_copied_batch(&[
        first, second
    ]));
}

#[test]
fn unavailable_confirmation_retains_valid_copies_and_partial_success() {
    let candidates = vec![candidate(1, 0), candidate(2, 20)];
    for partial in [false, true] {
        let mut copied = candidates.iter().map(translated).collect::<Vec<_>>();
        for region in &mut copied {
            region.translated_segments = vec![region.source_text.clone()];
        }
        let mut accepted = Vec::new();
        let mut covered = HashSet::new();
        if partial {
            accept_region(
                &mut accepted,
                &mut covered,
                translated(&candidates[0]),
                &candidates,
            );
        }
        let mut events = Vec::new();
        let result = finish_confirmation(
            &mut copied,
            &mut accepted,
            &mut covered,
            &candidates,
            &mut |r| events.push(r),
        );
        assert!(result.unresolved.is_empty());
        assert!(result.warning().is_none());
        assert_eq!(result.document.regions.len(), 2);
        assert_eq!(events.len(), if partial { 1 } else { 2 });
        assert!(copied.is_empty());
        if partial {
            assert_eq!(
                result.document.regions[0].translated_segments,
                ["translated-1"]
            );
        }
        // Finalization is idempotent: timeout/chain exhaustion cannot re-emit.
        finish_confirmation(
            &mut copied,
            &mut accepted,
            &mut covered,
            &candidates,
            &mut |_| panic!("duplicate reveal"),
        );
    }
}

#[test]
fn mixed_complete_batch_does_not_add_a_review_request() {
    use super::super::translation_validation::copies_for_recovery;
    let changed = translated(&candidate(1, 0));
    let mut copied = translated(&candidate(2, 20));
    copied.translated_segments = vec![copied.source_text.clone()];
    assert!(copies_for_recovery(&[changed.clone(), copied.clone()], 2).is_empty());
    assert_eq!(copies_for_recovery(&[changed, copied.clone()], 3), [copied]);
}

#[test]
fn review_keeps_equivalent_draft_but_accepts_new_meaning() {
    use super::super::translation_validation::retain_equivalent_draft;
    let mut draft = translated(&candidate(1, 0));
    draft.translated_segments = vec!["@Account_42".into()];
    let mut review = draft.clone();
    review.translated_segments = vec!["@account-42".into()];
    assert_eq!(
        retain_equivalent_draft(review.clone(), std::slice::from_ref(&draft)),
        draft
    );
    review.translated_segments = vec!["translated words @Account_42".into()];
    assert_eq!(
        retain_equivalent_draft(review.clone(), std::slice::from_ref(&draft)),
        review
    );
    review.id = 2;
    review.translated_segments = vec!["@account-42".into()];
    assert_eq!(retain_equivalent_draft(review.clone(), &[draft]), review);
}

#[test]
fn failed_review_restores_copies_without_covering_missing_units() {
    let candidates = vec![candidate(1, 0), candidate(2, 20), candidate(3, 40)];
    let mut draft = translated(&candidates[1]);
    draft.translated_segments = vec![draft.source_text.clone()];
    let mut copied = vec![draft];
    let mut accepted = vec![translated(&candidates[0])];
    let mut covered = HashSet::from([1]);
    let mut events = Vec::new();
    let outcome = finish_confirmation(
        &mut copied,
        &mut accepted,
        &mut covered,
        &candidates,
        &mut |region| events.push(region),
    );
    assert_eq!(outcome.unresolved, [3]);
    assert_eq!(events.len(), 1);
    assert_eq!(pending_candidates(&candidates, &covered), candidates[2..]);
    assert!(copied.is_empty());
}

#[test]
fn review_drafts_use_current_local_slots_and_do_not_leak_context_slots() {
    use super::super::request::{PreparedTranslationRequest, append_copy_review};
    let candidates = vec![candidate(9, 0), candidate(4, 20)];
    let drafts = vec![
        translated(&candidate(4, 20)),
        translated(&candidate(99, 40)),
    ];
    let mut request = PreparedTranslationRequest {
        text: String::new(),
        instruction: String::new(),
        schema: serde_json::json!({}),
        max_output_tokens: 256,
    };
    append_copy_review(&mut request, &candidates, &drafts).unwrap();
    let data: serde_json::Value =
        serde_json::from_str(request.text.lines().last().unwrap()).unwrap();
    assert_eq!(
        data,
        serde_json::json!([{"slot":1,"translation":"translated-4"}])
    );
    assert!(
        request
            .text
            .contains("Do not change text merely to make it different")
    );
    let previous = request.text.clone();
    append_copy_review(&mut request, &candidates, &[]).unwrap();
    assert_eq!(request.text, previous);
}
