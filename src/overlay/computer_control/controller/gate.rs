//! Safety gates — invariants enforced in CODE, not asked of the model in prose.
//!
//! Each returns a *reasoned refusal* the model must read and work around (confirm
//! with the user, fill the field, or restate intent), never a silent swallow.
//! Moving these out of the prompt is the de-pollution: the rules become
//! deterministic and the system instruction shrinks.
//!
//! Consequential-action checkpoints use only structural metadata exposed by the
//! controlled surface. Semantic interpretation remains with the model contract;
//! code never infers permission from phrases in the request.

use super::Verb;
use super::world::{IndexedElement, WorldState};

pub enum Gate {
    Allow,
    Block(String),
}

/// Decide whether to allow `verb` on `el`, given the world, the task context, and
/// whether the model has marked this act as user-confirmed.
pub fn gate_action(ws: &WorldState, el: &IndexedElement, verb: Verb, confirm: bool) -> Gate {
    // 1) Consequential / high-stakes — a CHECKPOINT. Allowed only if the user
    //    explicitly confirmed (confirm) or the task plainly asked for it.
    if matches!(
        verb,
        Verb::Click | Verb::Activate | Verb::Submit | Verb::Toggle
    ) && let Some(reason) = el.risk.as_deref()
        && !confirm
    {
        return Gate::Block(format!(
            "Did NOT {} {:?}: this {reason}. It is a consequential/irreversible action the task did not clearly ask \
for - CONFIRM with the user that they want this exact action (name what it does), then retry the SAME act with \
confirm:true. Never set confirm:true unless the user explicitly agreed to it just now.",
            verb.as_str(),
            el.name
        ));
    }

    // 2) Premature submit — required fields still empty in the SAME form.
    if matches!(verb, Verb::Submit | Verb::Click | Verb::Activate) && el.submit {
        let empty = ws.empty_required_in_form(el);
        if !empty.is_empty() {
            return Gate::Block(format!(
                "Did not submit - required field(s) still empty: {}. Fill them first; if you don't have a value, ASK \
the user (never invent personal data from what's on screen).",
                empty.join(", ")
            ));
        }
    }

    Gate::Allow
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::computer_control::controller::world::{
        ElHandle, SurfaceIdentity, WorldState,
    };

    fn element(id: u32, name: &str, value: Option<&str>, required: bool) -> IndexedElement {
        IndexedElement {
            id,
            role: if required { "textbox" } else { "button" }.to_string(),
            name: name.to_string(),
            value: value.map(str::to_string),
            state: None,
            enabled: true,
            required,
            submit: !required,
            form: Some(1),
            risk: None,
            handle: ElHandle::Native {
                cx: 1,
                cy: 1,
                provider_name: name.into(),
                automation_id: "gate-id".into(),
                runtime_id: vec![1],
            },
        }
    }

    #[test]
    fn activate_cannot_bypass_required_submit_gate() {
        let field = element(1, "field", Some(""), true);
        let submit = element(2, "submit", None, false);
        let world = WorldState {
            elements: vec![field, submit.clone()],
            url: None,
            title: None,
            identity: SurfaceIdentity::Native {
                hwnd: 1,
                pid: 2,
                generation: 1,
            },
        };
        assert!(matches!(
            gate_action(&world, &submit, Verb::Activate, false),
            Gate::Block(_)
        ));
    }
}
