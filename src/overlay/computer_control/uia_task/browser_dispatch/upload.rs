//! Request-scoped file disclosure at the dedicated browser mutation edge.

use super::*;

pub(super) fn dispatch(
    brain: &mut Brain,
    args: &Value,
    route: TabRoute,
    cancel: &AtomicBool,
) -> Value {
    let selector = args.get("selector").and_then(Value::as_str).unwrap_or("");
    let path = args.get("path").and_then(Value::as_str).unwrap_or("");
    let (tab, document) = match route {
        TabRoute::Current => (None, None),
        TabRoute::Exact(tab_id) => (Some(tab_id), brain.controlled_document_id.clone()),
    };
    let action = brain.active_action;
    super::super::super::browser::upload_file(
        selector,
        path,
        tab,
        document.as_deref(),
        cancel,
        |proposal| {
            let decision = brain
                .resource_authorization
                .evaluate_disclosure(proposal, cancel, action);
            if decision.authorized {
                Ok(decision.result)
            } else {
                Err(decision.result)
            }
        },
    )
}
