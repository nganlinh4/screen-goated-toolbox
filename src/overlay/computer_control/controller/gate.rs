//! Safety gates — invariants enforced in CODE, not asked of the model in prose.
//!
//! Each returns a *reasoned refusal* the model must read and work around (confirm
//! with the user, fill the field, or restate intent), never a silent swallow.
//! Moving these out of the prompt is the de-pollution: the rules become
//! deterministic and the system instruction shrinks.
//!
//! The consequential-action gate follows the industry pattern (OpenAI Operator /
//! browser-use / agent-browser): a high-stakes act (submit a payment / signup,
//! sign out, delete an account, start a purchase) is a CHECKPOINT — it returns
//! "confirm first" until the user has explicitly approved THAT action (signalled
//! by `confirm`) or the task itself clearly asked for it. The *structural* risk
//! signal is language-neutral (see `IndexedElement::risk`); the semantic long tail
//! (send/post/publish a message) stays in the prompt JUDGMENT block.

use super::Verb;
use super::world::{IndexedElement, WorldState};

pub enum Gate {
    Allow,
    Block(String),
}

/// Decide whether to allow `verb` on `el`, given the world, the task context, and
/// whether the model has marked this act as user-confirmed.
pub fn gate_action(
    ws: &WorldState,
    el: &IndexedElement,
    verb: Verb,
    value: Option<&str>,
    ctx: &str,
    confirm: bool,
) -> Gate {
    // 1) Consequential / high-stakes — a CHECKPOINT. Allowed only if the user
    //    explicitly confirmed (confirm) or the task plainly asked for it.
    if matches!(verb, Verb::Click | Verb::Submit | Verb::Toggle)
        && let Some(reason) = el.risk.as_deref()
        && !confirm
        && !goal_authorizes(ctx, el)
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
    if matches!(verb, Verb::Submit | Verb::Click) && el.submit {
        let empty = ws.empty_required_in_form(el);
        if !empty.is_empty() {
            return Gate::Block(format!(
                "Did not submit - required field(s) still empty: {}. Fill them first; if you don't have a value, ASK \
the user (never invent personal data from what's on screen).",
                empty.join(", ")
            ));
        }
    }

    // 3) PII (opt-in via CC_GATE_PII) — a credential-shaped fill the user never gave.
    //    A soft backstop; the prompt JUDGMENT block is the primary defense.
    if std::env::var("CC_GATE_PII").is_ok()
        && matches!(verb, Verb::Fill)
        && let Some(v) = value
        && looks_like_credential(v)
        && !ctx_contains(ctx, v)
    {
        return Gate::Block(format!(
            "Did not fill {:?}: {:?} looks like personal data (a username / email / credential) the user never gave \
you. Ask the user for it instead of guessing.",
            el.name, v
        ));
    }

    Gate::Allow
}

/// Whether the task/intent text plainly asked for this high-stakes action — a
/// significant word of its label appears in the context (works within whatever
/// language both are written in; no hard-coded keyword list).
fn goal_authorizes(ctx: &str, el: &IndexedElement) -> bool {
    let ctx = ctx.to_lowercase();
    el.name
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 4)
        .any(|w| ctx.contains(&w.to_lowercase()))
}

/// Heuristic "this string is a handle/email/token, not prose" — no whitespace,
/// short-ish, has a letter. Deliberately simple; only consulted when opted in.
fn looks_like_credential(v: &str) -> bool {
    let v = v.trim();
    let len = v.chars().count();
    (3..=40).contains(&len) && !v.contains(char::is_whitespace) && v.chars().any(|c| c.is_alphabetic())
}

fn ctx_contains(ctx: &str, v: &str) -> bool {
    ctx.to_lowercase().contains(&v.trim().to_lowercase())
}
