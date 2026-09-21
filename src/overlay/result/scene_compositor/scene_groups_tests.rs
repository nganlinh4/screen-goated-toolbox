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
            input_passthrough: false,
            spec: SourceCardSpec {
                image_rect: Default::default(),
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
            input_passthrough: false,
            spec: SourceCardSpec {
                image_rect: Default::default(),
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

#[test]
fn image_snapshot_keeps_capture_alignment_and_excludes_unrevealed_cells() {
    use super::{CARD_GROUPS, GROUPS, SCENES, SceneRect, SourceGroupState, image_snapshot};
    use std::{collections::HashMap, sync::Arc};
    let root = NEXT_SCENE_ID.fetch_sub(1, Ordering::SeqCst);
    let id = NEXT_SCENE_ID.fetch_sub(1, Ordering::SeqCst);
    let hidden = NEXT_SCENE_ID.fetch_sub(1, Ordering::SeqCst);
    let image = Arc::new(image::RgbaImage::new(640, 480));
    let original = SceneRect {
        x: 25,
        y: 40,
        width: 200,
        height: 80,
    };
    let cell = |visible| SourceCardState {
        input_passthrough: false,
        spec: SourceCardSpec {
            image_rect: original.clone(),
            target_rect: RECT {
                left: 125,
                top: 240,
                right: 325,
                bottom: 320,
            },
            backdrop_data_url: String::new(),
            foreground_color: "#fff".into(),
            preferred_font_size: 20.0,
            source_vertical: false,
            source_regions: vec![],
        },
        text: "Translated".into(),
        segments: vec!["Translated".into()],
        visible,
        stack_order: 1,
    };
    let visible = cell(true);
    SCENES
        .lock()
        .unwrap()
        .insert(id, source_card(id, &visible, (0, 0), 73));
    GROUPS.lock().unwrap().insert(
        root,
        SourceGroupState {
            controller_id: root,
            controller_stack_order: 1,
            card_order: vec![id, hidden],
            cards: HashMap::from([(id, visible), (hidden, cell(false))]),
            offset: (0, 0),
            opacity: 73,
            controls_visible: true,
            source_image: Some(image.clone()),
        },
    );
    CARD_GROUPS.lock().unwrap().insert(id, root);
    super::move_group(id, -800, 900).unwrap();
    super::resize_card(
        id,
        SceneRect {
            x: 400,
            y: 500,
            width: 300,
            height: 160,
        },
    )
    .unwrap();
    let snapshot = image_snapshot(root).unwrap();
    assert!(Arc::ptr_eq(&image, &snapshot.image));
    assert_eq!(snapshot.opacity, 73);
    assert_eq!(snapshot.cells.len(), 1);
    assert_eq!(snapshot.cells[0].rect, original);
    assert_eq!(snapshot.cells[0].id, id);
    CARD_GROUPS.lock().unwrap().remove(&id);
    GROUPS.lock().unwrap().remove(&root);
    SCENES.lock().unwrap().remove(&id);
}
