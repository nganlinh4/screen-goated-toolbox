use super::*;
use crate::overlay::screen_translate::contract::NormalizedBounds;
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
fn bounded_fallback_preserves_only_the_unresolved_source_regions() {
    let candidates = vec![candidate(1, 20), candidate(2, 40)];
    let mut accepted = vec![translated(&candidates[0])];
    let mut covered = HashSet::from([1]);
    let mut streamed = Vec::new();

    let preserved =
        preserve_unresolved_candidates(&candidates, &mut accepted, &mut covered, &mut |region| {
            streamed.push(region)
        });

    assert_eq!(preserved, 1);
    assert_eq!(streamed[0].id, 2);
    assert_eq!(streamed[0].translated_segments, ["source-2"]);
    assert!(completed_document(&candidates, &accepted, &covered).is_some());
}
