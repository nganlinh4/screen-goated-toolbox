//! Private commit edges for independently authorized local text-file targets.

use super::*;

impl Brain {
    pub(crate) fn record_user_request(&mut self, turn_id: u64, text: &str) {
        self.structural_authorization.record_request(turn_id, text);
        self.resource_authorization.record_request(turn_id, text);
    }

    pub(super) fn dispatch_exact_process(
        &mut self,
        args: &Value,
        cancel: &AtomicBool,
        action: super::super::telemetry::ActionTrace,
        authorize_repair_process: bool,
    ) -> Value {
        if self.dry {
            return json!({"ok": true, "note": "dry"});
        }
        if !authorize_repair_process {
            return executor::execute_ex("run_command", args, &self.profile, cancel);
        }
        let scope = self
            .resource_authorization
            .evaluate("run_command", args, cancel, Some(action));
        if !scope.authorized {
            return scope.result;
        }
        let mut result = executor::execute_ex("run_command", args, &self.profile, cancel);
        result["repair_process_authorization"] = scope.result;
        result
    }

    pub(super) fn dispatch_text_edit(
        &mut self,
        args: &Value,
        cancel: &AtomicBool,
        action: super::super::telemetry::ActionTrace,
    ) -> Value {
        if self.dry {
            return json!({"ok": true, "note": "dry"});
        }
        let scope =
            self.resource_authorization
                .evaluate("edit_text_file", args, cancel, Some(action));
        if !scope.authorized {
            return scope.result;
        }
        let mut result = executor::execute_ex("edit_text_file", args, &self.profile, cancel);
        result["resource_scope_authorization"] = scope.result;
        result
    }

    pub(super) fn dispatch_artifact_save(
        &mut self,
        args: &Value,
        cancel: &AtomicBool,
        action: super::super::telemetry::ActionTrace,
    ) -> Value {
        let has_target = args
            .get("path")
            .and_then(Value::as_str)
            .is_some_and(|path| !path.trim().is_empty());
        if !has_target {
            return super::super::artifacts::dispatch_tool(
                "save_artifact",
                args,
                &self.profile,
                cancel,
                self.dry,
            )
            .unwrap_or_else(|| json!({"ok": false, "error": "unknown action"}));
        }
        if self.dry {
            return json!({"ok": true, "note": "dry"});
        }
        let scope =
            self.resource_authorization
                .evaluate("save_artifact", args, cancel, Some(action));
        if !scope.authorized {
            return scope.result;
        }
        let mut result = super::super::artifacts::dispatch_tool(
            "save_artifact",
            args,
            &self.profile,
            cancel,
            false,
        )
        .unwrap_or_else(|| json!({"ok": false, "error": "unknown action"}));
        result["resource_scope_authorization"] = scope.result;
        result
    }

    pub(super) fn dispatch_structural_edit(
        &mut self,
        args: &Value,
        cancel: &AtomicBool,
        action: super::super::telemetry::ActionTrace,
    ) -> Value {
        if self.dry {
            return json!({"ok": true, "note": "dry"});
        }
        let preflight =
            executor::execute_ex("edit_text_file_structure", args, &self.profile, cancel);
        let supplied = args.get("structural_change_token").and_then(Value::as_str);
        let expected = preflight
            .get("structural_change_token")
            .and_then(Value::as_str);
        if supplied.is_none() || supplied != expected {
            return preflight;
        }
        let scope = self.resource_authorization.evaluate(
            "edit_text_file_structure",
            args,
            cancel,
            Some(action),
        );
        if !scope.authorized {
            return scope.result;
        }
        let decision =
            self.structural_authorization
                .evaluate(args, &preflight, cancel, Some(action));
        if decision.authorized {
            let mut committed = executor::commit_text_file_structure(args);
            committed["request_contract_authorization"] = decision.result;
            committed["resource_scope_authorization"] = scope.result;
            committed
        } else {
            let mut blocked = decision.result;
            blocked["preflight"] = preflight;
            blocked["resource_scope_authorization"] = scope.result;
            blocked
        }
    }
}
