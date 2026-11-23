use super::*;

#[test]
fn text_documents_are_visual_targets_but_tab_containers_are_not() {
    assert!(is_clickable("Document"));
    assert!(is_clickable("Slider"));
    assert!(is_clickable("TabItem"));
    assert!(!is_clickable("Tab"));
    assert!(!is_clickable("Other"));
}

fn element(control_type: &'static str, name: String) -> UiElement {
    UiElement {
        name,
        automation_id: "identity".into(),
        runtime_id: vec![1],
        control_type,
        left: 10,
        top: 10,
        right: 30,
        bottom: 30,
        enabled: true,
        protected: false,
        state: None,
        value: None.into(),
        required: false,
    }
}

#[test]
fn oversized_readouts_title_and_indexed_controls_are_bounded() {
    let huge = "界".repeat(100_000);
    let elements = vec![
        element("Window", huge.clone()),
        element("Text", huge.clone()),
    ];
    let controls = format!("@7 button \"{huge}\"\n");
    let state = format_state(
        &elements,
        None,
        View {
            x: 0,
            y: 0,
            w: 100,
            h: 100,
        },
        Grid { cols: 2, rows: 2 },
        false,
        Some(&controls),
    );
    assert!(state.len() < 16 * 1024);
    assert!(state.contains("…"));
    assert!(state.contains("[additional items omitted]"));
    assert_eq!(elements[1].name, huge);
}

#[test]
fn aggregate_readouts_stop_at_a_whole_line_and_mark_omissions() {
    let mut elements = vec![element("Window", "Document".into())];
    elements.extend((0..100).map(|index| element("Text", format!("{index}: {}", "a".repeat(100)))));
    let state = format_state(
        &elements,
        None,
        View {
            x: 0,
            y: 0,
            w: 100,
            h: 100,
        },
        Grid { cols: 2, rows: 2 },
        true,
        None,
    );
    assert!(state.len() < 4 * 1024);
    assert!(state.contains("[additional items omitted]"));
    assert!(state.contains("@cell"));
}

#[test]
fn serialized_state_is_bounded_even_for_control_characters() {
    let mut elements = vec![element("Window", "Document".into())];
    elements.extend(
        (0..100).map(|index| element("Text", format!("{index}{}", "\u{0001}".repeat(200)))),
    );
    let state = format_state(
        &elements,
        None,
        View {
            x: 0,
            y: 0,
            w: 100,
            h: 100,
        },
        Grid { cols: 2, rows: 2 },
        false,
        None,
    );
    assert!(crate::overlay::computer_control::model_text_budget::wire_len(&state) < 4 * 1024);
    assert!(state.contains("[additional items omitted]"));
}

#[test]
fn different_controls_with_the_same_clipped_prefix_are_not_collapsed() {
    let prefix = "x".repeat(300);
    let elements = vec![
        element("Window", "Document".into()),
        element("Button", format!("{prefix} one")),
        element("Button", format!("{prefix} two")),
    ];
    let state = format_state(
        &elements,
        None,
        View {
            x: 0,
            y: 0,
            w: 100,
            h: 100,
        },
        Grid { cols: 2, rows: 2 },
        false,
        None,
    );
    assert_eq!(state.matches("- Button \"").count(), 2);
}
