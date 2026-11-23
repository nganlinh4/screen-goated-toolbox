//! Preflighted multi-step execution with per-step re-resolution and receipts.

use serde_json::{Value, json};

use super::world::{ElHandle, IndexedElement, WorldState};
use super::{ActCtx, Controller, Surface, Verb, browser, gate, transition, validation, verify};

struct PlannedStep {
    requested_id: u32,
    verb: Verb,
    value: Option<String>,
    confirm: bool,
    target: IndexedElement,
}

impl Controller {
    /// Run a short preflighted sequence. Every step is resolved again in a fresh
    /// world, gated against current values, executed against the same surface
    /// identity, and followed by another observation before the next step.
    pub fn do_steps(&mut self, steps: &[Value], act: &ActCtx) -> Value {
        let mut surface = self.surface();
        let mut result = self.do_steps_on(steps, act, &mut *surface);
        if let (Some(tab_id), Some(object)) = (self.browser_tab_id, result.as_object_mut()) {
            object.insert("target_tab_id".to_string(), json!(tab_id));
        }
        result
    }

    pub(super) fn do_steps_on(
        &mut self,
        steps: &[Value],
        act: &ActCtx,
        surface: &mut dyn Surface,
    ) -> Value {
        let Some(planned_world) = self.last.take() else {
            let (current, published, coverage_error) = self.reobserve_for_delivery(surface);
            if let Some(error) = coverage_error {
                return super::coverage_before_dispatch(error);
            }
            let elements = current
                .as_ref()
                .map(WorldState::to_model_text)
                .unwrap_or_default();
            self.last = current;
            let _ = published;
            return json!({
                "ok": false,
                "code": "ERR_OBSERVATION_REQUIRED",
                "dispatch_ok": false,
                "effect_may_have_occurred": false,
                "error": "no current element list to plan against - observe() first, then do_steps with its @ids",
                "elements": elements,
            });
        };
        let plan = match plan_steps(steps, &planned_world) {
            Ok(plan) => plan,
            Err(error) => {
                self.last = Some(planned_world);
                return json!({
                    "ok": false,
                    "invalid_plan": true,
                    "dispatch_ok": false,
                    "effect_may_have_occurred": false,
                    "executed": false,
                    "error": error,
                });
            }
        };
        let mut current = match surface.observe() {
            Ok(world) if world.identity == planned_world.identity => world,
            Ok(world) => {
                let published = self.observations.publish(&world, false);
                self.last = Some(world);
                let mut result = json!({
                    "ok": false,
                    "code": "ERR_STALE_SURFACE",
                    "dispatch_ok": false,
                    "effect_may_have_occurred": false,
                    "stale": true,
                    "error": "the active surface changed after this sequence was planned; re-plan from the fresh list",
                });
                published.attach(&mut result);
                return result;
            }
            Err(error) => {
                self.last = None;
                if let Some(error) = super::super::browser::frame_coverage_error(&error) {
                    return super::coverage_before_dispatch(error);
                }
                return json!({
                    "ok": false,
                    "code": "ERR_OBSERVATION_UNAVAILABLE",
                    "dispatch_ok": false,
                    "effect_may_have_occurred": false,
                    "error": format!("could not refresh the planned surface: {error}"),
                });
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
            let prepared = surface.prepare_input(planned.verb, act, &current.identity);
            let before_surface = surface.input_snapshot();
            let execution = match prepared.and_then(|()| {
                surface.execute(&element, planned.verb, value, act, &current.identity)
            }) {
                Ok(execution) => execution,
                Err(error) => {
                    let mut receipt = browser::step_failure(
                        &error,
                        number,
                        planned.requested_id,
                        &element,
                        planned.verb,
                    );
                    stopped = Some(format!(
                        "step {number} ({} @{} {:?}) failed: {error}",
                        planned.verb.as_str(),
                        element.id,
                        element.name
                    ));
                    match surface.observe() {
                        Ok(world) => current = world,
                        Err(error) => {
                            cache_valid = false;
                            if let Some(error) = super::super::browser::frame_coverage_error(&error)
                            {
                                receipt["observation_error"] = error;
                            }
                        }
                    }
                    receipts.push(receipt);
                    break;
                }
            };
            let verification = planned.verb.verifies().then(|| {
                let read_back = surface.read_back(&element, &current.identity);
                if planned.verb == Verb::Select {
                    verify::verify_select(value.unwrap_or(""), &read_back)
                } else {
                    verify::verify_fill(value.unwrap_or(""), &read_back)
                }
            });
            let value_accepted = verification
                .as_ref()
                .is_none_or(|result| result.permits_progress());
            let value_unavailable = matches!(verification, Some(verify::VerifyOutcome::Unknown));
            let observed = transition::observe_after_action(
                surface,
                &current,
                &element,
                planned.verb,
                &before_surface,
            );
            let context_required =
                transition::requires_context_change(planned.verb, &element, &current.identity);
            let (activation_verified, effect_may_have_occurred) =
                transition::dispatched_effect_status(context_required, observed.context_changed);
            let accepted = value_accepted && activation_verified;
            let effect_verified = if planned.verb.verifies() {
                verification.as_ref().is_some_and(|result| result.is_ok())
            } else {
                observed.context_changed
            };
            let mut receipt = json!({
                "step": number,
                "ok": accepted,
                "effect_may_have_occurred": effect_may_have_occurred,
                "effect_verified": effect_verified,
                "requested_id": planned.requested_id,
                "resolved_id": element.id,
                "verb": planned.verb.as_str(),
                "target": {"role": element.role, "name": element.name},
                "execution": execution,
                "transition": {
                    "foreground_changed": observed.foreground_changed,
                    "title_changed": observed.title_changed,
                    "structure_changed": observed.structure_changed,
                    "context_changed": observed.context_changed,
                },
            });
            if let Some(result) = verification {
                if matches!(result, verify::VerifyOutcome::Protected) {
                    receipt["verification_status"] = json!("protected_unverified");
                }
                receipt["verify"] = json!(result.describe());
            }
            if !activation_verified {
                receipt["error"] = json!(
                    "the action was dispatched, but its required context change was not observed; inspect fresh or external evidence and do not repeat it blindly"
                );
            }
            let next_world = match observed.world {
                Some(world) => world,
                None => {
                    cache_valid = false;
                    if let Some(error) = observed.coverage_error {
                        receipt["observation_error"] = error;
                        receipt["effect_may_have_occurred"] = json!(true);
                    }
                    stopped = Some(format!(
                        "step {number}: effect dispatched but post-action observation failed: {}",
                        observed
                            .observation_error
                            .unwrap_or_else(|| "unknown observation error".to_string())
                    ));
                    receipts.push(receipt);
                    break;
                }
            };
            receipts.push(receipt);
            current = next_world;
            if !accepted {
                stopped = Some(if !value_accepted {
                    if value_unavailable {
                        format!("step {number}: read-back unavailable; effect is unverified")
                    } else {
                        format!("step {number}: read-back verification failed")
                    }
                } else {
                    format!("step {number}: activation verification failed")
                });
                break;
            }
            log.push(format!(
                "{} @{} {:?}",
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
        let published = cache_valid.then(|| self.observations.publish(&current, false));
        if cache_valid {
            self.last = Some(current);
        } else {
            self.last = None;
        }
        let effect_status = super::super::effect_receipt::batch_status(&receipts);
        let stale_target = receipts.iter().any(|receipt| {
            receipt.get("code").and_then(Value::as_str) == Some("ERR_BROWSER_STALE_TARGET")
        });
        let mut result = json!({
            "ok": stopped.is_none(),
            "completed": format!("{}/{}", log.len(), plan.len()),
            "did": log,
            "receipts": receipts,
            "effect_scope": "batch",
            "effect_status": effect_status.code(),
            "effect_verified": effect_status.is_verified(),
            "effect_may_have_occurred": effect_status.may_have_occurred(),
            "executed": effect_status.executed(),
        });
        if let Some(published) = published {
            published.attach(&mut result);
        }
        if stale_target {
            result["code"] = json!("ERR_BROWSER_STALE_TARGET");
            result["stale"] = json!(true);
            result["fresh_observation_attached"] = json!(result.get("observation").is_some());
            result["instruction"] = json!(
                "Use the attached current @ids for at most one retry. If the target churns again, change to a non-indexed current-frame or direct-provider route."
            );
        }
        if let Some(reason) = stopped {
            result["stopped"] = json!(reason);
        }
        result
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
                && element.protected == planned.protected
                && same_target_identity(&element.handle, &planned.handle)
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

/// Re-observation may renumber browser selectors or move native controls, but
/// a planned action remains bound to its original provider-owned identity.
fn same_target_identity(current: &ElHandle, planned: &ElHandle) -> bool {
    match (current, planned) {
        (
            ElHandle::Browser {
                session: left_session,
                tab_id: left_tab,
                document_id: left_document,
                element_id: left_element,
                ..
            },
            ElHandle::Browser {
                session: right_session,
                tab_id: right_tab,
                document_id: right_document,
                element_id: right_element,
                ..
            },
        ) => {
            !left_element.is_empty()
                && left_element == right_element
                && left_document == right_document
                && left_tab == right_tab
                && left_session == right_session
        }
        (
            ElHandle::Native {
                automation_id: left_automation,
                runtime_id: left_runtime,
                ..
            },
            ElHandle::Native {
                automation_id: right_automation,
                runtime_id: right_runtime,
                ..
            },
        ) => {
            !left_runtime.is_empty()
                && left_runtime == right_runtime
                && left_automation == right_automation
        }
        _ => false,
    }
}
