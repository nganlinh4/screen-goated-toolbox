use super::*;

#[test]
fn control_anchor_is_relative_to_the_virtual_desktop() {
    assert_eq!(
        relative_selection_anchor((-1200, 300), (640, 480), (-1920, -200)),
        [720, 500, 640, 480]
    );
}

#[test]
fn source_equivalent_regions_do_not_require_visual_replacement() {
    let region = TranslationRegion {
        id: 1,
        member_ids: vec![1],
        member_joins: Vec::new(),
        selections: Vec::new(),
        semantic_role: super::super::contract::SemanticRole::Value,
        source_text: "example.com/path".to_string(),
        translated_segments: vec!["example.com/path".to_string()],
        bounds: [0, 0, 10, 10].into(),
        background_color: None,
        text_color: None,
    };
    assert!(!should_render_segment(
        &region.source_text,
        &region.translated_segments[0]
    ));
}

#[test]
fn model_cell_is_recorded_as_individual_detector_segments() {
    let region = TranslationRegion {
        id: 4,
        member_ids: vec![4, 6, 7, 9],
        member_joins: vec![
            super::super::contract::MemberJoin::SameColumn,
            super::super::contract::MemberJoin::SameColumn,
            super::super::contract::MemberJoin::SameColumn,
        ],
        selections: vec![
            super::super::contract::TranslationSelection {
                region_id: 4,
                candidate_id: "4:0".to_string(),
                source_text: "one".to_string(),
                bounds: [0, 0, 10, 10].into(),
            },
            super::super::contract::TranslationSelection {
                region_id: 6,
                candidate_id: "6:0".to_string(),
                source_text: "two".to_string(),
                bounds: [0, 0, 10, 10].into(),
            },
            super::super::contract::TranslationSelection {
                region_id: 7,
                candidate_id: "7:0".to_string(),
                source_text: "three".to_string(),
                bounds: [0, 0, 10, 10].into(),
            },
            super::super::contract::TranslationSelection {
                region_id: 9,
                candidate_id: "9:0".to_string(),
                source_text: "four".to_string(),
                bounds: [0, 0, 10, 10].into(),
            },
        ],
        semantic_role: super::super::contract::SemanticRole::Dialogue,
        source_text: "source cell".to_string(),
        translated_segments: vec![
            "first".to_string(),
            "second".to_string(),
            "third".to_string(),
            "fourth".to_string(),
        ],
        bounds: [0, 0, 10, 10].into(),
        background_color: None,
        text_color: None,
    };
    let mut translations = HashMap::new();
    record_translations(region, &mut translations);
    assert_eq!(translations.get(&4).unwrap().translated_text, "first");
    assert_eq!(translations.get(&9).unwrap().translated_text, "fourth");
}

#[test]
fn completed_stream_regions_are_presented_before_document_completion() {
    let source = include_str!("render.rs");
    let region = source.find("Ok(RenderCommand::Region(region))").unwrap();
    let complete = source
        .find("Ok(RenderCommand::Complete(document))")
        .unwrap();
    let region_branch = &source[region..complete];

    assert!(region_branch.contains("refresh_blocks("));
    assert!(region_branch.contains("&trace_id, true"));
    assert!(!source.contains("receiver.try_recv()"));
    assert!(source.contains("prewarm_source_group("));
    assert!(source.contains("reveal_source_card("));
    assert!(source.contains("configure_deferred_text_only_result_window("));
    assert_eq!(
        source
            .matches("create_deferred_result_window_shell(")
            .count(),
        1
    );
    assert!(!source.contains("prewarm_region_window_shell("));
    assert!(!source.contains("sync_deferred_windows_batch("));
    assert!(!source.contains("ShowWindow("));
}
