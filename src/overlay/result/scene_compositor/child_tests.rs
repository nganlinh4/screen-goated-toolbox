use super::command_requires_region_redraw;
use crate::overlay::result::scene_compositor::protocol::{
    HostCommand, SceneGeometry, SceneRect, SceneTheme,
};

#[test]
fn only_commands_that_change_native_clipping_or_content_redraw_the_region() {
    let command = HostCommand::Geometry {
        cards: vec![SceneGeometry {
            id: 42,
            rect: SceneRect {
                x: 10,
                y: 20,
                width: 300,
                height: 200,
            },
            control_rect: SceneRect {
                x: 6,
                y: 18,
                width: 308,
                height: 204,
            },
            visible: true,
        }],
    };

    assert!(!command_requires_region_redraw(&command));
    assert!(!command_requires_region_redraw(&HostCommand::Theme {
        theme: SceneTheme {
            css: String::new(),
            controls_css: String::new(),
            cards: Vec::new(),
        },
    }));
    assert!(command_requires_region_redraw(&HostCommand::Raise {
        id: 42,
        stack_order: 9,
    }));
    assert!(!command_requires_region_redraw(&HostCommand::Opacity {
        id: 42,
        opacity: 71,
    }));
    assert!(command_requires_region_redraw(&HostCommand::Remove {
        id: 42
    }));
}

#[test]
fn authored_html_acceptance_is_captured_from_the_shared_compositor() {
    let source = include_str!("child.rs");

    assert!(source.contains("phase == \"interactive_document_alive\""));
    assert!(source.contains("acceptance_capture::capture_for_card(webview, *id)"));
}

#[test]
fn drag_completion_reveals_controls_only_after_committed_geometry_is_applied() {
    let child = include_str!("child.rs");
    let controls = include_str!("button_scene_runtime.js");
    let pointer = crate::overlay::result::button_canvas::document_script();
    let resize = include_str!("resize_runtime.js");

    assert!(!child.contains("ChildEvent::DragFinished { .. } =>"));
    assert!(!pointer.contains("activeResultDragPreview = null;\n    setResultDraggingCursor(false);\n    window.__SGT_BUTTON_SCENE__?.setDragActive(false)"));
    assert!(
        !resize.contains("active = null;\n    window.__SGT_BUTTON_SCENE__?.setDragActive(false)")
    );
    let settled = controls.find("command.type === 'drag_settled'").unwrap();
    let merge = controls[settled..].find("mergeCard(card)").unwrap();
    let reveal = controls[settled..].find("setDragActive(false)").unwrap();
    assert!(merge < reveal);
    assert!(controls[settled..].contains("externalDrag = false"));
}
