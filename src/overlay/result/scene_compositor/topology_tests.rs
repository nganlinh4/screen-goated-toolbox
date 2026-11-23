use super::*;

fn card(id: isize, x: i32, anchor: [i32; 4]) -> SceneCard {
    let mut card: SceneCard = serde_json::from_value(serde_json::json!({
        "id": id,
        "rect": {"x": x, "y": 30, "width": 80, "height": 60},
        "control_rect": {"x": x, "y": 25, "width": 90, "height": 70},
        "body": "result", "document": null, "refining": false,
        "background": "transparent", "opacity": 100, "visible": true
    }))
    .unwrap();
    card.controls.control_anchor = Some(anchor);
    card
}

#[test]
fn cached_cards_rebase_together_and_keep_visibility_and_content() {
    let mut cards = HashMap::from([
        (1, card(1, 100, [90, 20, 40, 50])),
        (2, card(2, 220, [210, 20, 40, 50])),
    ]);
    cards.get_mut(&2).unwrap().visible = false;
    rebase_cards(
        &mut cards,
        Origin { x: -1920, y: -200 },
        Origin { x: 0, y: 0 },
    )
    .unwrap();
    assert_eq!((cards[&1].rect.x, cards[&1].rect.y), (-1820, -170));
    assert_eq!(
        cards[&1].controls.control_anchor,
        Some([-1830, -180, 40, 50])
    );
    assert_eq!((cards[&2].rect.x, cards[&2].rect.y), (-1700, -170));
    assert!(!cards[&2].visible);
    assert_eq!(cards[&1].body, "result");
}

#[test]
fn cached_card_overflow_does_not_partially_rebase_other_cards() {
    let mut cards = HashMap::from([
        (1, card(1, 100, [90, 20, 40, 50])),
        (2, card(2, i32::MAX, [210, 20, 40, 50])),
    ]);
    let before = cards.clone();
    assert!(rebase_cards(&mut cards, Origin { x: 1, y: 0 }, Origin { x: 0, y: 0 }).is_none());
    assert_eq!(cards, before);
}

#[test]
fn queued_screen_geometry_localizes_against_origin_at_dispatch() {
    let geometry = SceneGeometry {
        id: 7,
        rect: SceneRect {
            x: -1600,
            y: -100,
            width: 400,
            height: 200,
        },
        control_rect: SceneRect {
            x: -1604,
            y: -102,
            width: 408,
            height: 204,
        },
        visible: true,
    };
    let queued = HostCommand::Geometry {
        cards: vec![geometry],
    };
    let HostCommand::Geometry { cards: old } =
        localize(queued.clone(), Origin { x: -1920, y: -200 }).unwrap()
    else {
        panic!("expected geometry")
    };
    let HostCommand::Geometry { cards: new } = localize(queued, Origin { x: 0, y: 0 }).unwrap()
    else {
        panic!("expected geometry")
    };
    assert_eq!((old[0].rect.x, old[0].rect.y), (320, 100));
    assert_eq!((new[0].rect.x, new[0].rect.y), (-1600, -100));
    assert_eq!(
        (new[0].control_rect.x, new[0].control_rect.y),
        (-1604, -102)
    );
}

#[test]
fn queued_controls_use_dispatch_origin_and_leave_other_fields_intact() {
    let controls = SceneControls {
        control_anchor: Some([-1700, -120, 300, 200]),
        group_actions: true,
        ..Default::default()
    };
    let queued = HostCommand::Controls {
        cards: vec![super::super::super::protocol::SceneControlUpdate { id: 5, controls }],
    };
    let HostCommand::Controls { cards } = localize(queued, Origin { x: -1920, y: -200 }).unwrap()
    else {
        panic!("expected controls")
    };
    assert_eq!(cards[0].controls.control_anchor, Some([220, 80, 300, 200]));
    assert!(cards[0].controls.group_actions);
}

#[test]
fn topology_conversion_preserves_queued_non_geometry_operation_order() {
    let commands = vec![
        HostCommand::RefineText {
            id: 5,
            text: "first".into(),
            is_insert: true,
            edit_session: Some("8".into()),
        },
        HostCommand::Geometry {
            cards: vec![SceneGeometry {
                id: 5,
                rect: SceneRect {
                    x: -100,
                    y: 20,
                    width: 50,
                    height: 40,
                },
                control_rect: SceneRect {
                    x: -100,
                    y: 20,
                    width: 50,
                    height: 40,
                },
                visible: true,
            }],
        },
        HostCommand::ExternalDrag { active: true },
        HostCommand::RefineText {
            id: 5,
            text: "second".into(),
            is_insert: true,
            edit_session: Some("8".into()),
        },
    ];
    let converted: Vec<_> = commands
        .into_iter()
        .map(|command| localize(command, Origin { x: -1920, y: 0 }).unwrap())
        .collect();
    assert!(matches!(&converted[0], HostCommand::RefineText { text, .. } if text == "first"));
    assert!(matches!(&converted[1], HostCommand::Geometry { cards } if cards[0].rect.x == 1820));
    assert!(matches!(
        &converted[2],
        HostCommand::ExternalDrag { active: true }
    ));
    assert!(matches!(&converted[3], HostCommand::RefineText { text, .. } if text == "second"));
}
