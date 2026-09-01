use super::{NEXT_SCENE_ID, SourceCardSpec, SourceCardState, move_scene_card, source_card};
use std::sync::atomic::Ordering;
use windows::Win32::Foundation::RECT;

#[test]
fn logical_scene_ids_never_overlap_native_window_handles() {
    let id = NEXT_SCENE_ID.fetch_sub(1, Ordering::SeqCst);
    assert!(id < 0);
}

#[test]
fn logical_translation_cells_never_own_result_controls() {
    let card = source_card(
        -42,
        &SourceCardState {
            spec: SourceCardSpec {
                target_rect: RECT {
                    left: 10,
                    top: 20,
                    right: 110,
                    bottom: 70,
                },
                backdrop_data_url: String::new(),
                foreground_color: "#ffffff".to_string(),
                preferred_font_size: 16.0,
                source_vertical: false,
                source_regions: Vec::new(),
            },
            text: "translated".to_string(),
            segments: vec!["translated".to_string()],
            visible: true,
            stack_order: 1,
        },
        (0, 0),
        100,
    );

    assert!(card.controls.hidden);
    assert!(card.controls.group_ids.is_empty());
}

#[test]
fn moving_geometry_leaves_the_control_cache_for_control_sync() {
    let mut card = source_card(
        -43,
        &SourceCardState {
            spec: SourceCardSpec {
                target_rect: RECT {
                    left: 10,
                    top: 20,
                    right: 110,
                    bottom: 70,
                },
                backdrop_data_url: String::new(),
                foreground_color: "#ffffff".to_string(),
                preferred_font_size: 16.0,
                source_vertical: false,
                source_regions: Vec::new(),
            },
            text: String::new(),
            segments: Vec::new(),
            visible: true,
            stack_order: 1,
        },
        (0, 0),
        100,
    );
    card.controls.control_anchor = Some([10, 20, 100, 50]);
    let controls_before_move = card.controls.clone();
    let origin_before_move = (card.rect.x, card.rect.y);

    let geometry = move_scene_card(&mut card, 35, -8);

    assert_eq!(
        (geometry.rect.x, geometry.rect.y),
        (
            origin_before_move.0.saturating_add(35),
            origin_before_move.1.saturating_sub(8)
        )
    );
    assert_eq!(card.controls, controls_before_move);
}
