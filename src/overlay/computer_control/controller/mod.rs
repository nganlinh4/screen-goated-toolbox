//! Deterministic model-decides, code-resolves controller.
//! Every indexed action is gated, dispatched, and checked against fresh state.

mod adoption;
mod browser;
mod gate;
mod native;
mod observation;
mod sequence;
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
    observations: observation::ObservationCache,
    browser_tab_id: Option<i64>,
    native_target: Option<String>,
}

impl Controller {
    pub fn new(native_target: Option<String>) -> Self {
        Controller {
            native_target,
            ..Controller::default()
        }
    }

    /// Drop the actionable world so a stale `@id` can never resolve after another tool.
    /// The last model-visible observation remains only as a payload-deduplication
    /// reference; it can never authorize input.
    pub fn invalidate(&mut self) {
        self.last = None;
    }

    pub fn observed_identity(&self) -> Option<&SurfaceIdentity> {
        self.last.as_ref().map(|world| &world.identity)
    }

    /// Bind browser perception and indexed actions to one explicit tab for this
    /// user turn. `None` preserves the ordinary foreground-surface behavior.
    pub fn set_browser_tab_target(&mut self, tab_id: Option<i64>) {
        if self.browser_tab_id == tab_id {
            return;
        }
        self.browser_tab_id = tab_id;
        self.invalidate();
    }

    /// Retire turn ownership without discarding the immutable observed world.
    /// No action can run between turns, and the next job must bind this cache to
    /// its exact captured surface before it becomes actionable again.
    pub fn release_turn_target(&mut self) {
        self.browser_tab_id = None;
    }

    /// Bind a new job to the exact model-visible source surface. Cached ids are
    /// retained only when every surface identity field still matches.
    pub fn bind_source_surface(&mut self, source: Option<&SurfaceIdentity>) -> bool {
        let compatible = source.is_some_and(|identity| {
            self.last
                .as_ref()
                .is_some_and(|world| &world.identity == identity)
        });
        self.browser_tab_id = source.and_then(|identity| match identity {
            SurfaceIdentity::Browser { tab_id, .. } => Some(*tab_id),
            SurfaceIdentity::Native { .. } => None,
        });
        if !compatible {
            self.invalidate();
        }
        compatible
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
                let n = ws.elements.len();
                let title = ws.title.clone();
                let url = ws.url.clone();
                let target_tab_id = match &ws.identity {
                    SurfaceIdentity::Browser { tab_id, .. } => Some(*tab_id),
                    SurfaceIdentity::Native { .. } => None,
                };
                let surface = if url.is_some() { "browser" } else { "native" };
                let published = self.observations.publish(&ws, true);
                self.last = Some(ws);
                let mut result = json!({
                    "ok": true,
                    "count": n,
                    "surface": surface,
                    "target_tab_id": target_tab_id,
                    "title": title,
                    "url": url,
                    "note": "Act by @id. click performs one ordinary click; activate performs the element's default action. Use fill/select/submit/toggle as named and re-observe after a view change."
                });
                published.attach(&mut result);
                result
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
            let (current, published) = self.reobserve_for_delivery(&mut *s);
            self.last = current;
            let mut result = json!({"ok": false,
                "code": "ERR_OBSERVATION_REQUIRED",
                "dispatch_ok": false,
                "effect_may_have_occurred": false,
                "error": "the view changed since your last observe() (another tool ran, or the screen moved) — re-synced below; act on an @id from THIS list, not an earlier one",
            });
            if let Some(published) = published {
                published.attach(&mut result);
            }
            return result;
        };
        if !s.identity().is_ok_and(|current| current == ws.identity) {
            let (current, published) = self.reobserve_for_delivery(&mut *s);
            self.last = current;
            let mut result = json!({
                "ok": false,
                "code": "ERR_STALE_SURFACE",
                "stale": true,
                "dispatch_ok": false,
                "effect_may_have_occurred": false,
                "error": "the active surface no longer matches the observation that produced this @id; re-synced below",
            });
            if let Some(published) = published {
                published.attach(&mut result);
            }
            return result;
        }
        let Some(el) = ws.get(id).cloned() else {
            let (current, published) = self.reobserve_for_delivery(&mut *s);
            self.last = current;
            let mut result = json!({"ok": false,
                "code": "ERR_STALE_ELEMENT_ID",
                "dispatch_ok": false,
                "effect_may_have_occurred": false,
                "error": format!("no element @{id} in the current view (it changed — act on an id from the latest list)"),
            });
            if let Some(published) = published {
                published.attach(&mut result);
            }
            return result;
        };
        if let Err(reason) = s.validate(&el, verb, value) {
            self.last = Some(ws);
            return json!({
                "ok": false,
                "invalid_action": true,
                "dispatch_ok": false,
                "effect_may_have_occurred": false,
                "executed": false,
                "error": reason,
            });
        }
        // GATE — structural invariants (consequential confirmation / required submit).
        if let gate::Gate::Block(reason) = gate::gate_action(&ws, &el, verb, confirm) {
            self.last = Some(ws);
            return json!({
                "ok": false,
                "blocked": reason,
                "dispatch_ok": false,
                "effect_may_have_occurred": false,
                "executed": false,
            });
        }
        let before_surface = super::uia::input_target_snapshot();
        // EXECUTE through the existing primitives, preserving its real input
        // receipt instead of treating dispatch as proof of the intended effect.
        let execution = match s.execute(&el, verb, value, act, &ws.identity) {
            Ok(evidence) => evidence,
            Err(e) => {
                let (after_world, published) = self.reobserve_for_delivery(&mut *s);
                self.last = after_world;
                let mut result = browser::action_failure(&e, verb, &el);
                if let Some(published) = published {
                    browser::mark_resynced(&mut result);
                    published.attach(&mut result);
                }
                return result;
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
        let published = transition
            .world
            .as_ref()
            .map(|world| self.observations.publish(world, false));
        self.last = transition.world;
        let mut r = json!({
            "ok": action_verified,
            "dispatch_ok": true,
            "effect_may_have_occurred": effect_may_have_occurred,
            "effect_verified": match verb {
                Verb::Fill | Verb::Select => verify.as_ref().map(|item| item.is_ok()),
                _ if context_required => Some(transition.context_changed),
                Verb::Click if collection_item => Some(selected),
                _ if transition.context_changed => Some(true),
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
        });
        if let Some(published) = published {
            published.attach(&mut r);
        }
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

    fn reobserve(&mut self, s: &mut dyn Surface) -> String {
        let (text, world) = self.reobserve_with_state(s);
        if let Some(world) = world.as_ref() {
            let _ = self.observations.publish(world, true);
        }
        self.last = world;
        text
    }

    fn reobserve_for_delivery(
        &mut self,
        surface: &mut dyn Surface,
    ) -> (
        Option<WorldState>,
        Option<observation::PublishedObservation>,
    ) {
        match surface.observe() {
            Ok(world) => {
                let published = self.observations.publish(&world, false);
                (Some(world), Some(published))
            }
            Err(_) => (None, None),
        }
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

#[cfg(test)]
mod tests;
