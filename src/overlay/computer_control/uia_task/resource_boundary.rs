//! A fresh semantic effect checkpoint for active rejected resource boundaries.

use super::*;
use std::sync::atomic::Ordering;

const MAX_PROPOSAL_BYTES: usize = 96 * 1024;

impl Brain {
    pub(super) fn authorize_resource_boundary(
        &mut self,
        name: &str,
        args: &Value,
        ctx: &str,
        cancel: &AtomicBool,
        action: super::super::telemetry::ActionTrace,
    ) -> Result<Option<Value>, Value> {
        if self.dry || !self.exact_edit_guard.needs_scope_check(name) {
            return Ok(None);
        }
        if cancel.load(Ordering::Acquire) {
            return Err(unavailable("action cancelled before resource assessment"));
        }
        let declaration = super::super::engine::tool_declaration(name).ok_or_else(|| {
            unavailable("the selected tool's validated session declaration is unavailable")
        })?;
        let process = if name == "run_command" {
            Some(
                executor::command_scope_context(args)
                    .map_err(|error| unavailable(&error.to_string()))?,
            )
        } else {
            None
        };
        let evidence = json!({
            "tool_declaration": declaration, "process":process,
            "dedicated_file_target":self.exact_edit_guard.pending_identity(),
            "source_frame":self.source_frame,"controlled_tab_id":self.controlled_tab_id,
            "controlled_document_id":self.controlled_document_id,
            "observed_world":self.controller.resource_checkpoint_context(),
            "acting_model_context_untrusted":ctx,
        });
        let proposal = build_proposal(
            name,
            args,
            self.exact_edit_guard.protected_resources(),
            evidence,
        )?;
        let decision =
            self.resource_authorization
                .evaluate_boundary(&proposal, cancel, Some(action));
        if decision.authorized {
            Ok(Some(decision.result))
        } else {
            Err(decision.result)
        }
    }
}

fn build_proposal(
    name: &str,
    args: &Value,
    protected: Value,
    evidence: Value,
) -> Result<Value, Value> {
    if evidence
        .pointer("/tool_declaration/name")
        .and_then(Value::as_str)
        != Some(name)
    {
        return Err(unavailable(
            "tool declaration does not match the selected action",
        ));
    }
    let proposal = json!({
        "operation":"preserve_rejected_exact_resource_boundaries",
        "protected_resources":protected,"proposed_action":{"tool":name,"arguments":args},
        "evidence_untrusted":evidence,
    });
    let encoded = serde_json::to_vec(&proposal).map_err(|error| unavailable(&error.to_string()))?;
    if encoded.len() > MAX_PROPOSAL_BYTES {
        return Err(unavailable(
            "the complete resource-effect proposal exceeds the assessment budget; choose a narrower action",
        ));
    }
    Ok(proposal)
}

fn unavailable(reason: &str) -> Value {
    json!({"ok":false,"code":"ERR_EXACT_EDIT_EFFECT_SCOPE_UNVERIFIED","error":reason,
        "effect_may_have_occurred":false,"effect_verified":false,"executed":false,
        "instruction":"Use the dedicated exact mutation or an independently assessable unrelated action. Missing effect evidence cannot establish that a rejected resource is preserved."})
}

#[cfg(test)]
mod tests;
