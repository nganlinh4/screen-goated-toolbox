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
fn semantic_cell_order_is_not_rewritten_by_geometry() {
    let region = TranslationRegion {
        id: 4,
        member_ids: vec![4, 6, 7, 9],
        member_joins: vec![
            super::super::contract::MemberJoin::SameColumn,
            super::super::contract::MemberJoin::SameColumn,
            super::super::contract::MemberJoin::SameColumn,
        ],
        selections: Vec::new(),
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
    let translated = translations.get(&vec![4, 6, 7, 9]).unwrap();
    assert_eq!(translated.translated_text, "first second third fourth");
}
