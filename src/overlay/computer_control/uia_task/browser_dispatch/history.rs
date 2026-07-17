//! Exact-tab browser history traversal and post-navigation rebinding.

use super::*;

pub(super) fn dispatch(brain: &mut Brain, args: &Value, route: TabRoute) -> Value {
    let direction = args.get("direction").and_then(Value::as_str).unwrap_or("");
    let result = match route {
        TabRoute::Current => super::super::super::browser::traverse_history(direction),
        TabRoute::Exact(tab_id) => {
            super::super::super::browser::traverse_history_on_tab(direction, tab_id)
        }
    };
    let tab_id = match route {
        TabRoute::Exact(tab_id) => Some(tab_id),
        TabRoute::Current => result
            .get("target_tab_id")
            .and_then(Value::as_i64)
            .filter(|tab_id| *tab_id > 0),
    };
    if let Some(tab_id) = tab_id {
        brain.controlled_tab_id = Some(tab_id);
        brain.controller.set_browser_tab_target(Some(tab_id));
        brain.bind_navigation_result(result, tab_id)
    } else {
        result
    }
}
