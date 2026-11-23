use super::*;

#[test]
fn resource_checkpoint_uses_only_the_existing_world_without_its_url() {
    let observed = WorldState {
        identity: SurfaceIdentity::Native {
            hwnd: 10,
            pid: 20,
            generation: 30,
        },
        elements: Vec::new(),
        title: Some("Observed document".into()),
        url: Some("https://receiver.example/?private-token#fragment".into()),
    };
    let mut controller = Controller {
        last: Some(observed),
        ..Controller::default()
    };
    let context = controller.resource_checkpoint_context().unwrap();
    assert!(
        context["observed_controls"]
            .as_str()
            .unwrap()
            .contains("Observed document")
    );
    assert!(!context.to_string().contains("private-token"));
    assert!(
        controller
            .last
            .as_ref()
            .unwrap()
            .url
            .as_ref()
            .unwrap()
            .contains("private-token")
    );
    controller.invalidate();
    assert!(controller.resource_checkpoint_context().is_none());
}
