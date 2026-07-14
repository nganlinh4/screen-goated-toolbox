//! Deterministic model-decides, code-resolves controller.
//! Every indexed action is gated, dispatched, and checked against fresh state.

mod adoption;
mod browser;
mod gate;
mod native;
mod transition;
mod validation;
mod verify;
pub mod world;

use std::sync::atomic::AtomicBool;

use serde_json::{Value, json};

use super::human_input::HumanProfile;
use verify::ReadBack;
use world::{IndexedElement, SurfaceIdentity, WorldState};

/// Execution context the NATIVE surface needs for humanized `SendInput` (the
/// browser surface acts through trusted CDP events and ignores it).
pub struct ActCtx<'a> {
    pub profile: &'a HumanProfile,
    pub cancel: &'a AtomicBool,
    pub dry: bool,
}

/// A high-level action the controller knows how to resolve, execute, and verify.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verb {
    Click,
    Activate,
    Fill,
    Select,
    Submit,
    Toggle,
}

impl Verb {
    pub fn as_str(self) -> &'static str {
        match self {
            Verb::Click => "click",
            Verb::Activate => "activate",
            Verb::Fill => "fill",
            Verb::Select => "select",
            Verb::Submit => "submit",
            Verb::Toggle => "toggle",
        }
    }

    fn parse(s: &str) -> Option<Verb> {
        match s.trim().to_lowercase().as_str() {
            "click" | "press" => Some(Verb::Click),
            "activate" | "open" => Some(Verb::Activate),
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
    fn identity(&mut self) -> anyhow::Result<SurfaceIdentity>;
    fn observe(&mut self) -> anyhow::Result<WorldState>;
    fn validate(&self, el: &IndexedElement, verb: Verb, value: Option<&str>) -> Result<(), String> {
        validation::validate_action(el, verb, value)
    }
    fn execute(
        &mut self,
        el: &IndexedElement,
        verb: Verb,
        value: Option<&str>,
        act: &ActCtx,
        expected: &SurfaceIdentity,
    ) -> anyhow::Result<Value>;
    fn read_back(&mut self, el: &IndexedElement) -> ReadBack;
}

/// Per-task controller state: the last observed world (so `act(id, …)` resolves
/// the id the model picked back to a concrete element).
#[derive(Default)]
pub struct Controller {
    last: Option<WorldState>,
    browser_tab_id: Option<i64>,
    native_target: Option<String>,
}

struct PlannedStep {
    requested_id: u32,
    verb: Verb,
    value: Option<String>,
    confirm: bool,
    target: IndexedElement,
}

impl Controller {
    pub fn new(native_target: Option<String>) -> Self {
        Controller {
            native_target,
            ..Controller::default()
        }
    }

    /// Drop the cached world so a stale `@id` can never resolve after another tool.
    pub fn invalidate(&mut self) {
        self.last = None;
    }

    pub fn observed_identity(&self) -> Option<&SurfaceIdentity> {
        self.last.as_ref().map(|world| &world.identity)
    }

    /// Bind browser perception and indexed actions to one explicit tab for this
    /// user turn. `None` preserves the ordinary foreground-surface behavior.
    pub fn set_browser_tab_target(&mut self, tab_id: Option<i64>) {
        self.browser_tab_id = tab_id;
        self.invalidate();
    }

    /// Seed the native controller from a fresh, identity-bracketed foreground
    /// observation after frame rendering. Pre-render coordinates are never cached.
    pub fn prime_native(&mut self) -> String {
        let mut surface = native::NativeSurface::new(self.native_target.clone());
        self.reobserve(&mut surface)
    }

    /// The surface to drive right now: the browser only when the extension owns
    /// the exact foreground OS window, otherwise the native desktop surface.
    fn surface(&self) -> Box<dyn Surface> {
        if super::browser::input_active() {
            if let Some(tab_id) = self.browser_tab_id {
                Box::new(browser::BrowserSurface::on_tab(tab_id))
            } else {
                Box::new(browser::BrowserSurface::new())
            }
        } else {
            Box::new(native::NativeSurface::new(self.native_target.clone()))
        }
    }

    /// Read the active surface into the indexed world the model acts on.
    pub fn observe(&mut self) -> Value {
        let mut s = self.surface();
        match s.observe() {
            Ok(ws) => {
                let (text, n) = (ws.to_model_text(), ws.elements.len());
                let title = ws.title.clone();
                let url = ws.url.clone();
                let target_tab_id = match &ws.identity {
                    SurfaceIdentity::Browser { tab_id, .. } => Some(*tab_id),
                    SurfaceIdentity::Native { .. } => None,
                };
                let surface = if url.is_some() { "browser" } else { "native" };
                self.last = Some(ws);
                json!({
                    "ok": true,
                    "count": n,
                    "elements": text,
                    "surface": surface,
                    "target_tab_id": target_tab_id,
                    "title": title,
                    "url": url,
                    "note": "Act by @id. click performs one ordinary click; activate performs the element's default action. Use fill/select/submit/toggle as named and re-observe after a view change."
                })
            }
            Err(e) => {
                json!({"ok": false, "error": format!("could not read the current view: {e}")})
            }
        }
    }

    /// Resolve `id` → gate → execute → verify → re-observe. Returns the tool result
    /// the model reads: the verify verdict plus the fresh indexed world.
    pub fn act(
        &mut self,
        id: u32,
        verb_str: &str,
        value: Option<&str>,
        confirm: bool,
        act: &ActCtx,
    ) -> Value {
        let target_tab_id = self.browser_tab_id;
        let mut s = self.surface();
        let Some(verb) = Verb::parse(verb_str) else {
            return json!({"ok": false,
                "error": format!("unknown verb '{verb_str}' — use click, activate, fill, select, submit, or toggle")});
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
        if !s.identity().is_ok_and(|current| current == ws.identity) {
            let elements = self.reobserve(&mut *s);
            return json!({
                "ok": false,
                "stale": true,
                "error": "the active surface no longer matches the observation that produced this @id; re-synced below",
                "elements": elements,
            });
        }
        let Some(el) = ws.get(id).cloned() else {
            let elements = self.reobserve(&mut *s);
            return json!({"ok": false,
                "error": format!("no element @{id} in the current view (it changed — act on an id from the latest list)"),
                "elements": elements});
        };
        if let Err(reason) = s.validate(&el, verb, value) {
            self.last = Some(ws);
            return json!({"ok": false, "invalid_action": true, "error": reason});
        }
        // GATE — structural invariants (consequential confirmation / required submit).
        if let gate::Gate::Block(reason) = gate::gate_action(&ws, &el, verb, confirm) {
            self.last = Some(ws);
            return json!({"ok": false, "blocked": reason});
        }
        let before_surface = super::uia::input_target_snapshot();
        // EXECUTE through the existing primitives, preserving its real input
        // receipt instead of treating dispatch as proof of the intended effect.
        let execution = match s.execute(&el, verb, value, act, &ws.identity) {
            Ok(evidence) => evidence,
            Err(e) => {
                let (elements, after_world) = self.reobserve_with_state(&mut *s);
                self.last = after_world;
                return browser::action_failure(&e, verb, &el, elements);
            }
        };
        // VERIFY (fill/select) by reading the value back.
        let verify = verb
            .verifies()
            .then(|| verify::verify_fill(value.unwrap_or(""), &s.read_back(&el)));
        let transition = transition::observe_after_action(&mut *s, &ws, &el, verb, &before_surface);
        let selected = transition
            .world
            .as_ref()
            .and_then(|world| transition::matching_state(world, &el))
            .is_some_and(|state| state.contains("selected"));
        let collection_item = matches!(el.role.as_str(), "listitem" | "treeitem");
        let context_required = transition::requires_context_change(verb, &el, &ws.identity);
        let (action_verified, effect_may_have_occurred) =
            transition::dispatched_effect_status(context_required, transition.context_changed);
        let effect = match verb {
            _ if context_required && transition.context_changed => "context_changed",
            _ if context_required => "context_change_not_verified",
            Verb::Click if collection_item && selected => "selected_only",
            Verb::Click if collection_item => "selection_not_verified",
            Verb::Fill | Verb::Select => "readback_checked",
            _ if transition.context_changed => "context_changed",
            _ => "dispatch_only",
        };
        self.last = transition.world;
        let mut r = json!({
            "ok": action_verified,
            "dispatch_ok": true,
            "effect_may_have_occurred": effect_may_have_occurred,
            "effect_verified": match verb {
                Verb::Fill | Verb::Select => verify.as_ref().map(|item| item.is_ok()),
                _ if context_required => Some(transition.context_changed),
                Verb::Click if collection_item => Some(selected),
                _ => None,
            },
            "effect": effect,
            "did": verb.as_str(),
            "target": {"id": id, "role": el.role, "name": el.name},
            "target_tab_id": target_tab_id,
            "execution": execution,
            "transition": {
                "foreground_changed": transition.foreground_changed,
                "title_changed": transition.title_changed,
                "structure_changed": transition.structure_changed,
                "context_changed": transition.context_changed,
            },
            "elements": transition.elements,
        });
        if !action_verified {
            r["error"] = json!(
                "the action was dispatched, but its required context change was not observed; inspect fresh or external evidence and do not repeat it blindly"
            );
        }
        if let Some(v) = verify {
            if !v.is_ok() {
                r["ok"] = json!(false);
            }
            r["verify"] = json!(v.describe());
        }
        r
    }

    /// Run a short preflighted sequence. Every step is resolved again in a fresh
    /// world, gated against current values, executed against the same surface
    /// identity, and followed by another observation before the next step.
    pub fn do_steps(&mut self, steps: &[Value], act: &ActCtx) -> Value {
        let mut s = self.surface();
        let mut result = self.do_steps_on(steps, act, &mut *s);
        if let (Some(tab_id), Some(object)) = (self.browser_tab_id, result.as_object_mut()) {
            object.insert("target_tab_id".to_string(), json!(tab_id));
        }
        result
    }

    fn do_steps_on(&mut self, steps: &[Value], act: &ActCtx, surface: &mut dyn Surface) -> Value {
        let Some(planned_world) = self.last.take() else {
            let elements = self.reobserve(surface);
            return json!({"ok": false,
                "error": "no current element list to plan against - observe() first, then do_steps with its @ids",
                "elements": elements});
        };
        let plan = match plan_steps(steps, &planned_world) {
            Ok(plan) => plan,
            Err(error) => {
                self.last = Some(planned_world);
                return json!({"ok": false, "invalid_plan": true, "error": error});
            }
        };
        let mut current = match surface.observe() {
            Ok(world) if world.identity == planned_world.identity => world,
            Ok(world) => {
                let elements = world.to_model_text();
                self.last = Some(world);
                return json!({"ok": false, "stale": true,
                    "error": "the active surface changed after this sequence was planned; re-plan from the fresh list",
                    "elements": elements});
            }
            Err(error) => {
                self.last = None;
                return json!({"ok": false, "error": format!("could not refresh the planned surface: {error}")});
            }
        };
        let mut log = Vec::new();
        let mut receipts = Vec::new();
        let mut stopped = None;
        let mut cache_valid = true;
        for (index, planned) in plan.iter().enumerate() {
            let number = index + 1;
            if act.cancel.load(std::sync::atomic::Ordering::Relaxed) {
                stopped = Some(format!("interrupted at step {number}"));
                break;
            }
            if !surface
                .identity()
                .is_ok_and(|identity| identity == current.identity)
            {
                stopped = Some(format!(
                    "step {number}: active surface changed before dispatch"
                ));
                cache_valid = false;
                break;
            }
            let element = match refind_unique(&current, &planned.target) {
                Ok(element) => element,
                Err(error) => {
                    stopped = Some(format!("step {number}: {error}"));
                    break;
                }
            };
            let value = planned.value.as_deref();
            if let Err(error) = surface.validate(&element, planned.verb, value) {
                stopped = Some(format!("step {number}: {error}"));
                break;
            }
            if let gate::Gate::Block(reason) =
                gate::gate_action(&current, &element, planned.verb, planned.confirm)
            {
                stopped = Some(format!("step {number} blocked: {reason}"));
                break;
            }
            let before_surface = super::uia::input_target_snapshot();
            let execution =
                match surface.execute(&element, planned.verb, value, act, &current.identity) {
                    Ok(execution) => execution,
                    Err(error) => {
                        receipts.push(browser::step_failure(
                            &error,
                            number,
                            planned.requested_id,
                            &element,
                            planned.verb,
                        ));
                        stopped = Some(format!(
                            "step {number} ({} @{} {:?}) failed: {error}",
                            planned.verb.as_str(),
                            element.id,
                            element.name
                        ));
                        match surface.observe() {
                            Ok(world) => current = world,
                            Err(_) => cache_valid = false,
                        }
                        break;
                    }
                };
            let verification = planned
                .verb
                .verifies()
                .then(|| verify::verify_fill(value.unwrap_or(""), &surface.read_back(&element)));
            let value_verified = verification.as_ref().is_none_or(|result| result.is_ok());
            let transition = transition::observe_after_action(
                surface,
                &current,
                &element,
                planned.verb,
                &before_surface,
            );
            let context_required =
                transition::requires_context_change(planned.verb, &element, &current.identity);
            let (activation_verified, effect_may_have_occurred) =
                transition::dispatched_effect_status(context_required, transition.context_changed);
            let verified = value_verified && activation_verified;
            let mut receipt = json!({
                "step": number,
                "ok": verified,
                "effect_may_have_occurred": effect_may_have_occurred,
                "requested_id": planned.requested_id,
                "resolved_id": element.id,
                "verb": planned.verb.as_str(),
                "target": {"role": element.role, "name": element.name},
                "execution": execution,
                "transition": {
                    "foreground_changed": transition.foreground_changed,
                    "title_changed": transition.title_changed,
                    "structure_changed": transition.structure_changed,
                    "context_changed": transition.context_changed,
                },
            });
            if let Some(result) = verification {
                receipt["verify"] = json!(result.describe());
            }
            if !activation_verified {
                receipt["error"] = json!(
                    "the action was dispatched, but its required context change was not observed; inspect fresh or external evidence and do not repeat it blindly"
                );
            }
            let next_world = match transition.world {
                Some(world) => world,
                None => {
                    cache_valid = false;
                    stopped = Some(format!(
                        "step {number}: effect dispatched but post-action observation failed: {}",
                        transition
                            .observation_error
                            .unwrap_or_else(|| "unknown observation error".to_string())
                    ));
                    receipts.push(receipt);
                    break;
                }
            };
            receipts.push(receipt);
            current = next_world;
            if !verified {
                stopped = Some(if !value_verified {
                    format!("step {number}: read-back verification failed")
                } else {
                    format!("step {number}: activation verification failed")
                });
                break;
            }
            log.push(format!(
                "{} @{} {:?} ✓",
                planned.verb.as_str(),
                element.id,
                element.name
            ));
            if index + 1 < plan.len() && current.identity != planned_world.identity {
                stopped = Some(format!(
                    "step {number}: surface changed; remaining planned steps were not run"
                ));
                break;
            }
        }
        let elements = if cache_valid {
            current.to_model_text()
        } else {
            String::new()
        };
        if cache_valid {
            self.last = Some(current);
        } else {
            self.last = None;
        }
        let mut result = json!({
            "ok": stopped.is_none(),
            "completed": format!("{}/{}", log.len(), plan.len()),
            "did": log,
            "receipts": receipts,
            "elements": elements,
        });
        if let Some(reason) = stopped {
            result["stopped"] = json!(reason);
        }
        result
    }

    fn reobserve(&mut self, s: &mut dyn Surface) -> String {
        let (text, world) = self.reobserve_with_state(s);
        self.last = world;
        text
    }

    fn reobserve_with_state(&mut self, s: &mut dyn Surface) -> (String, Option<WorldState>) {
        match s.observe() {
            Ok(ws) => {
                let text = ws.to_model_text();
                (text, Some(ws))
            }
            Err(_) => {
                self.last = None;
                (String::new(), None)
            }
        }
    }
}

fn plan_steps(steps: &[Value], world: &WorldState) -> Result<Vec<PlannedStep>, String> {
    if steps.is_empty() {
        return Err("do_steps requires at least one step".to_string());
    }
    steps
        .iter()
        .enumerate()
        .map(|(index, step)| {
            let number = index + 1;
            let raw_id = step
                .get("id")
                .and_then(Value::as_u64)
                .ok_or_else(|| format!("step {number}: missing id"))?;
            let requested_id = u32::try_from(raw_id)
                .map_err(|_| format!("step {number}: id is outside the supported range"))?;
            let verb = step
                .get("verb")
                .and_then(Value::as_str)
                .and_then(Verb::parse)
                .ok_or_else(|| format!("step {number}: missing/unknown verb"))?;
            let value = step
                .get("value")
                .and_then(Value::as_str)
                .map(str::to_string);
            let target = world
                .get(requested_id)
                .cloned()
                .ok_or_else(|| format!("step {number}: no @{requested_id} in the planned view"))?;
            validation::validate_shape(&target, verb, value.as_deref())
                .map_err(|error| format!("step {number}: {error}"))?;
            Ok(PlannedStep {
                requested_id,
                verb,
                value,
                confirm: step
                    .get("confirm")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                target,
            })
        })
        .collect()
}

fn refind_unique(world: &WorldState, planned: &IndexedElement) -> Result<IndexedElement, String> {
    let candidates: Vec<_> = world
        .elements
        .iter()
        .filter(|element| {
            element.role == planned.role
                && element.name == planned.name
                && element.form == planned.form
                && element.submit == planned.submit
        })
        .cloned()
        .collect();
    match candidates.as_slice() {
        [element] => Ok(element.clone()),
        [] => Err(format!(
            "planned target {:?} is no longer present; re-observe and re-plan",
            planned.name
        )),
        _ => Err(format!(
            "planned target {:?} is now ambiguous; re-observe and use separate actions",
            planned.name
        )),
    }
}

#[cfg(test)]
mod tests;
