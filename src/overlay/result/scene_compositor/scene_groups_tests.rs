use super::{NEXT_SCENE_ID, SourceCardSpec, SourceCardState, move_scene_card, source_card};
use std::sync::atomic::Ordering;
use windows::Win32::Foundation::RECT;

#[test]
fn retirement_notifies_registered_waiters_and_registration_after_removal() {
    use super::{
        GROUPS, SourceGroupHandle, SourceGroupState, register_retirement_wake, remove_group,
    };
    use crate::overlay::wake_event::WakeEvent;
    use std::{collections::HashMap, sync::Arc};
    let root = NEXT_SCENE_ID.fetch_sub(1, Ordering::SeqCst);
    let wake = Arc::new(WakeEvent::new().unwrap());
    GROUPS.lock().unwrap().insert(
        root,
        SourceGroupState {
            controller_id: root,
            controller_stack_order: 1,
            card_order: vec![],
            cards: HashMap::new(),
            offset: (0, 0),
            opacity: 100,
            controls_visible: false,
            source_image: None,
            retirement_wake: None,
        },
    );
    let handle = SourceGroupHandle { root_id: root };
    register_retirement_wake(handle, Arc::clone(&wake));
    assert!(!wake.signalled_within(0));
    remove_group(root);
    assert!(wake.signalled_within(100));
    assert!(!GROUPS.lock().unwrap().contains_key(&root));
    register_retirement_wake(handle, Arc::clone(&wake));
    assert!(wake.signalled_within(100));
}

#[test]
fn source_rectangle_keeps_absolute_position_across_desktop_origin_changes() {
    let target = RECT {
        left: -1250,
        top: -170,
        right: -950,
        bottom: 80,
    };
    let scene = super::scene_rect(target);
    assert_eq!(
        (scene.x, scene.y, scene.width, scene.height),
        (-1250, -170, 300, 250)
    );
}

#[test]
fn first_reveal_keeps_the_source_group_move_in_its_controller_anchor() {
    let anchor = super::controller_anchor([-1200, -100, 500, 300], (320, -60));
    assert_eq!(
        (anchor.x, anchor.y, anchor.width, anchor.height),
        (-880, -160, 500, 300)
    );
}

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
    let mut controller = source_card(root, &visible, (0, 0), 73);
    controller.control_rect = SceneRect {
        x: -1200,
        y: -100,
        width: 500,
        height: 300,
    };
    SCENES.lock().unwrap().insert(root, controller);
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
            retirement_wake: None,
        },
    );
    CARD_GROUPS.lock().unwrap().insert(id, root);
    super::move_group(id, -800, 900).unwrap();
    let group_offset = GROUPS.lock().unwrap()[&root].offset;
    let rebuilt = super::controller_anchor([-1200, -100, 500, 300], group_offset);
    assert_eq!(rebuilt, SCENES.lock().unwrap()[&root].control_rect);
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
    {
        let groups = GROUPS.lock().unwrap();
        let target = groups[&root].cards[&id].spec.target_rect;
        assert_eq!(
            (target.left, target.top, target.right, target.bottom),
            (1200, -400, 1500, -240)
        );
    }
    let snapshot = image_snapshot(root).unwrap();
    assert!(Arc::ptr_eq(&image, &snapshot.image));
    assert_eq!(snapshot.opacity, 73);
    assert_eq!(snapshot.cells.len(), 1);
    assert_eq!(snapshot.cells[0].rect, original);
    assert_eq!(snapshot.cells[0].id, id);
    CARD_GROUPS.lock().unwrap().remove(&id);
    GROUPS.lock().unwrap().remove(&root);
    SCENES.lock().unwrap().remove(&id);
    SCENES.lock().unwrap().remove(&root);
}
