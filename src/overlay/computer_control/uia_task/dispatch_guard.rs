//! Structural guards for coarse visual actions.

use serde_json::{Value, json};

use super::{Grid, UiElement, View, uia};

pub(super) fn block_grid_click(
    view: View,
    grid: &Grid,
    cell: u32,
    target: Option<&str>,
) -> Option<Value> {
    let (x0, y0, x1, y1) = grid.frac_rect(cell, 0.0)?;
    let rect = (
        view.x + (x0 * f64::from(view.w)).round() as i32,
        view.y + (y0 * f64::from(view.h)).round() as i32,
        view.x + (x1 * f64::from(view.w)).round() as i32,
        view.y + (y1 * f64::from(view.h)).round() as i32,
    );
    let elements = uia::enumerate(target).ok()?;
    let candidates = interactive_candidates(&elements, rect);
    if candidates.is_empty() {
        return None;
    }
    Some(json!({
        "ok": false,
        "code": "ERR_COARSE_CLICK_HAS_NATIVE_TARGETS",
        "error": "grid-cell center is ambiguous because native elements occupy this cell",
        "candidates": candidates,
        "instruction": "Do not guess with pixel tools. Use observe/act for an exact native element; use click_target only when no semantic element exists.",
    }))
}

fn interactive_candidates(elements: &[UiElement], rect: (i32, i32, i32, i32)) -> Vec<String> {
    let mut names = elements
        .iter()
        .filter(|element| {
            element.enabled
                && !element.name.trim().is_empty()
                && matches!(
                    element.control_type,
                    "Button"
                        | "CheckBox"
                        | "ComboBox"
                        | "Edit"
                        | "Hyperlink"
                        | "ListItem"
                        | "MenuItem"
                        | "RadioButton"
                        | "SplitButton"
                        | "TabItem"
                        | "TreeItem"
                )
                && element.right > rect.0
                && element.left < rect.2
                && element.bottom > rect.1
                && element.top < rect.3
        })
        .map(|element| element.name.trim().to_string())
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    names.truncate(8);
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    fn element(name: &str, left: i32, right: i32) -> UiElement {
        UiElement {
            name: name.to_string(),
            automation_id: format!("element-{name}"),
            runtime_id: vec![left, right],
            control_type: "ListItem",
            left,
            top: 50,
            right,
            bottom: 150,
            enabled: true,
            state: None,
            value: None,
            required: false,
        }
    }

    #[test]
    fn native_items_make_a_grid_cell_ambiguous() {
        let elements = vec![element("one", 10, 80), element("two", 70, 140)];
        assert_eq!(
            interactive_candidates(&elements, (0, 0, 100, 200)),
            ["one", "two"]
        );
        assert!(interactive_candidates(&elements, (200, 0, 300, 200)).is_empty());
    }
}
