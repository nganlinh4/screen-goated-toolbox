//! The deterministic CONTROLLER: the model DECIDES, this code RESOLVES → EXECUTES
//! → VERIFIES → GATES. Reliability, targeting, and safety move out of the prompt
//! and into code invariants — fixing the failure family (wrong target, no verify,
//! destructive click, premature submit) while shrinking the system instruction.
//!
//! One harness over a surface-agnostic `Surface` (browser now; native in Stage 2).
//! The model sees an indexed world (`observe`) and acts by id (`act`); the
//! controller resolves the id, gates the action, executes it through the existing
//! primitives, and reads the result back to confirm it actually happened.

mod browser;
mod gate;
mod native;
mod verify;
pub mod world;

use std::sync::atomic::AtomicBool;

use serde_json::{Value, json};

use super::human_input::HumanProfile;
use verify::ReadBack;
use world::{IndexedElement, WorldState};

/// Execution context the NATIVE surface needs for humanized `SendInput` (the
/// browser surface acts through trusted CDP events and ignores it).
pub struct ActCtx<'a> {
    pub profile: &'a HumanProfile,
    pub cancel: &'a AtomicBool,
    pub dry: bool,
}

/// A high-level action the controller knows how to resolve, execute, and verify.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Verb {
    Click,
    Fill,
    Select,
    Submit,
    Toggle,
}

impl Verb {
    pub fn as_str(self) -> &'static str {
        match self {
            Verb::Click => "click",
            Verb::Fill => "fill",
            Verb::Select => "select",
            Verb::Submit => "submit",
            Verb::Toggle => "toggle",
        }
    }

    fn parse(s: &str) -> Option<Verb> {
        match s.trim().to_lowercase().as_str() {
            "click" | "press" | "open" => Some(Verb::Click),
            "fill" | "type" | "enter" => Some(Verb::Fill),
            "select" | "set" | "choose" => Some(Verb::Select),
            "submit" => Some(Verb::Submit),
            "toggle" | "check" | "uncheck" => Some(Verb::Toggle),
            _ => None,
        }
    }

    /// Verbs whose effect we confirm by reading the element's value back.
    fn verifies(self) -> bool {
        matches!(self, Verb::Fill | Verb::Select)
    }
}

/// A surface the controller can perceive and act on. One impl per surface.
trait Surface {
    fn observe(&mut self) -> anyhow::Result<WorldState>;
    fn execute(&mut self, el: &IndexedElement, verb: Verb, value: Option<&str>, act: &ActCtx) -> anyhow::Result<()>;
    fn read_back(&mut self, el: &IndexedElement) -> ReadBack;
}

/// Per-task controller state: the last observed world (so `act(id, …)` resolves
/// the id the model picked back to a concrete element).
#[derive(Default)]
pub struct Controller {
    last: Option<WorldState>,
}

impl Controller {
    pub fn new() -> Self {
        Controller::default()
    }

    /// Drop the cached world so the next `act` re-syncs. The harness calls this
    /// after any non-controller tool runs (which may have moved the screen), so a
    /// stale `@id` from an earlier observe can never resolve onto the wrong element.
    pub fn invalidate(&mut self) {
        self.last = None;
    }

    /// The surface to drive right now: the browser when a connected Chromium page
    /// is in front, otherwise the native desktop surface.
    fn surface(&self) -> Box<dyn Surface> {
        if super::browser::input_active() {
            Box::new(browser::BrowserSurface::new())
        } else {
            Box::new(native::NativeSurface::new())
        }
    }

    /// Read the active surface into the indexed world the model acts on.
    pub fn observe(&mut self) -> Value {
        let mut s = self.surface();
        match s.observe() {
            Ok(ws) => {
                let (text, n) = (ws.to_model_text(), ws.elements.len());
                self.last = Some(ws);
                json!({
                    "ok": true,
                    "count": n,
                    "elements": text,
                    "note": "Act on any element by its @id with act(id, verb[, value]). verbs: click, fill (value=text), select (value=option), submit, toggle. Re-observe after the view changes."
                })
            }
            Err(e) => json!({"ok": false, "error": format!("could not read the current view: {e}")}),
        }
    }

    /// Resolve `id` → gate → execute → verify → re-observe. Returns the tool result
    /// the model reads: the verify verdict plus the fresh indexed world.
    pub fn act(&mut self, id: u32, verb_str: &str, value: Option<&str>, ctx: &str, confirm: bool, act: &ActCtx) -> Value {
        let mut s = self.surface();
        let Some(verb) = Verb::parse(verb_str) else {
            return json!({"ok": false,
                "error": format!("unknown verb '{verb_str}' — use click, fill, select, submit, or toggle")});
        };
        // The cached world is trusted only right after observe/act. If it's gone
        // (the harness invalidated it because another tool ran), the model's @id is
        // from a stale list — re-sync and make it pick from the CURRENT one rather
        // than acting on the wrong element.
        let Some(ws) = self.last.take() else {
            let elements = self.reobserve(&mut *s);
            return json!({"ok": false,
                "error": "the view changed since your last observe() (another tool ran, or the screen moved) — re-synced below; act on an @id from THIS list, not an earlier one",
                "elements": elements});
        };
        let Some(el) = ws.get(id).cloned() else {
            let elements = self.reobserve(&mut *s);
            return json!({"ok": false,
                "error": format!("no element @{id} in the current view (it changed — act on an id from the latest list)"),
                "elements": elements});
        };
        // GATE — code invariants (consequential-confirm / required-submit / opt-in PII).
        if let gate::Gate::Block(reason) = gate::gate_action(&ws, &el, verb, value, ctx, confirm) {
            self.last = Some(ws);
            return json!({"ok": false, "blocked": reason});
        }
        // EXECUTE through the existing primitives.
        if let Err(e) = s.execute(&el, verb, value, act) {
            self.last = Some(ws);
            return json!({"ok": false, "error": format!("could not {} {:?}: {e}", verb.as_str(), el.name)});
        }
        // VERIFY (fill/select) by reading the value back.
        let verify = verb.verifies().then(|| verify::verify_fill(value.unwrap_or(""), &s.read_back(&el)));
        // RE-OBSERVE so the model's next decision is from the post-action world.
        let elements = self.reobserve(&mut *s);
        let mut r = json!({
            "ok": true,
            "did": verb.as_str(),
            "target": {"id": id, "name": el.name},
            "elements": elements
        });
        if let Some(v) = verify {
            if !v.is_ok() {
                r["ok"] = json!(false);
            }
            r["verify"] = json!(v.describe());
        }
        r
    }

    /// PLAN-THEN-EXECUTE: run a SEQUENCE of steps against ONE observed world (the
    /// model planned against its last observe), each gated + verified, stopping at
    /// the first failure / gate-block / barge-in. One model round-trip drives N
    /// deterministic steps. Steps are NOT re-observed between each other (the
    /// browser's stamped selectors persist; a native form's coords stay put) so the
    /// planned @ids stay valid; the result re-observes once for the model's next move.
    pub fn do_steps(&mut self, steps: &[Value], ctx: &str, act: &ActCtx) -> Value {
        let mut s = self.surface();
        let Some(ws) = self.last.take() else {
            let elements = self.reobserve(&mut *s);
            return json!({"ok": false,
                "error": "no current element list to plan against - observe() first, then do_steps with its @ids",
                "elements": elements});
        };
        let mut log: Vec<String> = Vec::new();
        let mut stopped: Option<String> = None;
        for (i, step) in steps.iter().enumerate() {
            let n = i + 1;
            if act.cancel.load(std::sync::atomic::Ordering::Relaxed) {
                stopped = Some(format!("interrupted at step {n}"));
                break;
            }
            let id = step.get("id").and_then(Value::as_u64).unwrap_or(0) as u32;
            let value = step.get("value").and_then(Value::as_str);
            let confirm = step.get("confirm").and_then(Value::as_bool).unwrap_or(false);
            let Some(verb) = step.get("verb").and_then(Value::as_str).and_then(Verb::parse) else {
                stopped = Some(format!("step {n}: missing/unknown verb"));
                break;
            };
            let Some(el) = ws.get(id).cloned() else {
                stopped = Some(format!("step {n}: no @{id} in the planned view (re-observe and re-plan)"));
                break;
            };
            if let gate::Gate::Block(reason) = gate::gate_action(&ws, &el, verb, value, ctx, confirm) {
                stopped = Some(format!("step {n} blocked: {reason}"));
                break;
            }
            if let Err(e) = s.execute(&el, verb, value, act) {
                stopped = Some(format!("step {n} ({} @{id} {:?}) failed: {e}", verb.as_str(), el.name));
                break;
            }
            if verb.verifies() {
                let v = verify::verify_fill(value.unwrap_or(""), &s.read_back(&el));
                if !v.is_ok() {
                    stopped = Some(format!("step {n} ({} @{id} {:?}) {}", verb.as_str(), el.name, v.describe()));
                    break;
                }
            }
            log.push(format!("{} @{id} {:?} ✓", verb.as_str(), el.name));
        }
        let elements = self.reobserve(&mut *s);
        let mut r = json!({
            "ok": stopped.is_none(),
            "completed": format!("{}/{}", log.len(), steps.len()),
            "did": log,
            "elements": elements
        });
        if let Some(reason) = stopped {
            r["stopped"] = json!(reason);
        }
        r
    }

    fn reobserve(&mut self, s: &mut dyn Surface) -> String {
        match s.observe() {
            Ok(ws) => {
                let t = ws.to_model_text();
                self.last = Some(ws);
                t
            }
            Err(_) => {
                self.last = None;
                String::new()
            }
        }
    }
}
