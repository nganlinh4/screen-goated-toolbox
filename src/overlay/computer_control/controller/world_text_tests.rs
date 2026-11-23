use super::*;
use crate::overlay::computer_control::model_text_budget::MAX_WORLD_BYTES;

#[test]
fn previews_and_unavailable_values_are_not_reported_as_empty_required_fields() {
    for preview in [None, Some(String::new()), Some("partial".into())] {
        let mut unknown = element(1, "unknown".into());
        unknown.required = true;
        unknown.value = None;
        unknown.value_preview = preview;
        let mut empty = element(2, "empty".into());
        empty.required = true;
        empty.value = Some(String::new());
        let submit = element(3, "submit".into());
        let world = WorldState {
            elements: vec![unknown, empty, submit.clone()],
            url: None,
            title: None,
            identity: SurfaceIdentity::Native {
                hwnd: 1,
                pid: 2,
                generation: 3,
            },
        };
        assert_eq!(world.empty_required_in_form(&submit), ["empty"]);
        let text = world.to_model_text();
        assert!(text.contains("required·unobserved"));
        assert!(text.contains("required·empty"));
    }
}

fn element(id: u32, name: String) -> IndexedElement {
    IndexedElement {
        id,
        role: "button".into(),
        name,
        value: Some("value".into()),
        value_preview: None,
        editable: false,
        protected: false,
        state: None,
        enabled: true,
        required: false,
        submit: false,
        form: None,
        risk: None,
        handle: ElHandle::Native {
            cx: 10,
            cy: 20,
            provider_name: "stable".into(),
            automation_id: "id".into(),
            runtime_id: vec![1],
        },
    }
}

#[test]
fn oversized_names_and_flags_do_not_change_target_identity_or_exceed_budget() {
    let huge = "界".repeat(100_000);
    let mut first = element(1, huge.clone());
    first.state = Some(huge.clone());
    first.risk = Some(huge.clone());
    let world = WorldState {
        elements: (1..=200)
            .map(|id| {
                if id == 1 {
                    first.clone()
                } else {
                    element(id, "界".repeat(400))
                }
            })
            .collect(),
        title: Some(huge.clone()),
        url: Some(huge.clone()),
        identity: SurfaceIdentity::Native {
            hwnd: 1,
            pid: 2,
            generation: 3,
        },
    };
    let text = world.to_model_text();
    assert!(text.len() <= MAX_WORLD_BYTES);
    assert!(
        crate::overlay::computer_control::model_text_budget::wire_len(&text) <= MAX_WORLD_BYTES
    );
    assert!(text.contains("@1 button"));
    assert!(text.contains("…"));
    assert!(text.contains("[additional items omitted]"));
    assert_eq!(world.elements[0].name, huge);
    assert_eq!(world.elements[0].handle, first.handle);
}

#[test]
fn escaped_control_characters_respect_the_serialized_budget() {
    let world = WorldState {
        elements: (1..=100)
            .map(|id| element(id, "\u{0001}".repeat(256)))
            .collect(),
        title: Some("\u{0001}".repeat(256)),
        url: None,
        identity: SurfaceIdentity::Native {
            hwnd: 1,
            pid: 2,
            generation: 3,
        },
    };
    let text = world.to_model_text();
    assert!(
        crate::overlay::computer_control::model_text_budget::wire_len(&text) <= MAX_WORLD_BYTES
    );
    assert!(text.contains("[additional items omitted]"));
}

#[test]
fn ordinary_short_controls_keep_actionable_ids_and_content() {
    let world = WorldState {
        elements: vec![element(7, "Save changes".into())],
        title: Some("Document".into()),
        url: None,
        identity: SurfaceIdentity::Native {
            hwnd: 1,
            pid: 2,
            generation: 3,
        },
    };
    let text = world.to_model_text();
    assert!(text.contains("PAGE: Document"));
    assert!(text.contains("@7 button \"Save changes\""));
    assert!(!text.contains("omitted"));
}
