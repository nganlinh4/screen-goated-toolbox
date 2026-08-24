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
