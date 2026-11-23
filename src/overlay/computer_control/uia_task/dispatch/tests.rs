use super::*;

#[test]
fn an_out_of_range_cell_is_rejected_instead_of_aliasing_a_valid_cell() {
    let mut brain = Brain::new(None);
    let before = brain.view;
    let cancel = Arc::new(AtomicBool::new(false));

    let result = brain.dispatch(
        "zoom",
        &json!({"cell": u64::from(u32::MAX) + 2}),
        "",
        &cancel,
        None,
        false,
    );

    assert_eq!(result["ok"], false, "{result}");
    assert_eq!(brain.view, before);
    assert!(!brain.zoomed);
}
