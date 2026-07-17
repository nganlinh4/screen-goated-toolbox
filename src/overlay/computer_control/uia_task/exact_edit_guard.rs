//! Turn-scoped resource guard for rejected exact-file mutations.
//!
//! A rejected exact transaction cannot be silently re-expressed by another
//! capability against that same resource. Unrelated computer work remains
//! available.

use serde_json::{Value, json};

const STRUCTURAL_REJECTION_LIMIT: u8 = 3;
const MATCH_MISS_REREAD_LIMIT: u8 = 2;

#[derive(Default)]
pub(super) struct ExactEditGuard {
    rejected_resources: Vec<String>,
    structural_rejections: Vec<StructuralRejection>,
    match_misses: Vec<StructuralRejection>,
}

struct StructuralRejection {
    resource: String,
    count: u8,
}

impl ExactEditGuard {
    pub(super) fn reset(&mut self) {
        self.rejected_resources.clear();
        self.structural_rejections.clear();
        self.match_misses.clear();
    }

    pub(super) fn before_action(&self, name: &str, args: &Value) -> Option<Value> {
        if name == "edit_text_file"
            && let Some(resource) = args
                .get("path")
                .and_then(Value::as_str)
                .map(normalize_resource)
                .filter(|path| !path.is_empty())
            && self
                .match_misses
                .iter()
                .any(|entry| entry.resource == resource && entry.count >= MATCH_MISS_REREAD_LIMIT)
        {
            return Some(json!({
                "ok": false,
                "code": "ERR_TEXT_FILE_REREAD_REQUIRED",
                "error": "two exact replacement attempts missed the current resource version",
                "effect_may_have_occurred": false,
                "effect_verified": false,
                "executed": false,
                "retryable": true,
                "instruction": "Read this exact file again, copy the current contiguous old_text literally, then retry once. Changing replacement text without a fresh read cannot clear this checkpoint."
            }));
        }
        let protected = self
            .rejected_resources
            .iter()
            .find(|resource| value_references_resource(args, resource));
        (protected.is_some()
            && !is_exact_edit_tool(name)
            && super::super::turn_policy::is_mutating_tool(name))
        .then(|| {
            json!({
                "ok": false,
                "code": "ERR_EXACT_EDIT_BYPASS_BLOCKED",
                "effect_may_have_occurred": false,
                "effect_verified": false,
                "blocked_operation": name,
                "protected_resource": protected,
                "instruction": "An exact text edit for this resource was rejected. Read that file again and repair it through the matching exact text capability, or report the typed blocker. Unrelated actions remain available."
            })
        })
    }

    pub(super) fn record_result(&mut self, name: &str, args: &Value, result: &mut Value) {
        if name == "read_text_file" {
            if result.get("ok").and_then(Value::as_bool) == Some(true)
                && let Some(resource) = result
                    .get("path")
                    .and_then(Value::as_str)
                    .or_else(|| args.get("path").and_then(Value::as_str))
                    .map(normalize_resource)
                    .filter(|path| !path.is_empty())
            {
                self.match_misses.retain(|entry| entry.resource != resource);
            }
            return;
        }
        if !is_exact_edit_tool(name) {
            return;
        }
        let Some(resource) = args
            .get("path")
            .and_then(Value::as_str)
            .map(normalize_resource)
            .filter(|path| !path.is_empty())
        else {
            return;
        };
        let succeeded = result.get("ok").and_then(Value::as_bool) == Some(true)
            && result.get("effect_verified").and_then(Value::as_bool) == Some(true);
        self.rejected_resources.retain(|path| path != &resource);
        if succeeded {
            self.structural_rejections
                .retain(|entry| entry.resource != resource);
            self.match_misses.retain(|entry| entry.resource != resource);
            return;
        }
        self.rejected_resources.push(resource.clone());

        let code = result
            .get("code")
            .and_then(Value::as_str)
            .map(str::to_string);
        if code.as_deref() == Some("ERR_TEXT_FILE_MATCH_MISSING") {
            increment_resource_count(&mut self.match_misses, &resource);
        }
        let contract_rejected =
            code.as_deref() == Some("ERR_TEXT_FILE_STRUCTURE_REQUEST_CONTRACT_REJECTED");
        let repeated_structural_effect = matches!(
            code.as_deref(),
            Some(
                "ERR_TEXT_FILE_STRUCTURE_CHANGE_REQUIRES_EXPLICIT_TOOL"
                    | "ERR_TEXT_FILE_STRUCTURE_CHANGE"
                    | "ERR_TEXT_FILE_FORMULA_MIXED_EDIT"
            )
        );
        let existing = self
            .structural_rejections
            .iter_mut()
            .find(|entry| entry.resource == resource);
        let count = if contract_rejected {
            match existing {
                Some(entry) => {
                    entry.count = entry.count.saturating_add(1);
                    entry.count
                }
                None => {
                    self.structural_rejections
                        .push(StructuralRejection { resource, count: 1 });
                    1
                }
            }
        } else if repeated_structural_effect {
            existing
                .map(|entry| {
                    entry.count = entry.count.saturating_add(1);
                    entry.count
                })
                .unwrap_or(0)
        } else {
            0
        };
        if count >= STRUCTURAL_REJECTION_LIMIT {
            let cause_code = code.as_deref().unwrap_or("ERR_TEXT_FILE_STRUCTURE_CHANGE");
            if let Some(fields) = result.as_object_mut() {
                fields.insert(
                    "code".to_string(),
                    json!("ERR_TEXT_FILE_STRUCTURE_REJECTION_LIMIT"),
                );
                fields.insert("cause_code".to_string(), json!(cause_code));
                fields.insert("terminal_blocker".to_string(), Value::Bool(true));
                fields.insert("retryable".to_string(), Value::Bool(false));
                fields.insert(
                    "error".to_string(),
                    json!(
                        "the same resource reached the bounded limit for structural or formula changes after independent request-contract rejection"
                    ),
                );
                fields.insert(
                    "instruction".to_string(),
                    json!(
                        "Stop this turn. Do not retry the structural effect or bypass it through another tool; report the blocker once."
                    ),
                );
            }
        }
    }
}

fn increment_resource_count(entries: &mut Vec<StructuralRejection>, resource: &str) -> u8 {
    if let Some(entry) = entries.iter_mut().find(|entry| entry.resource == resource) {
        entry.count = entry.count.saturating_add(1);
        entry.count
    } else {
        entries.push(StructuralRejection {
            resource: resource.to_string(),
            count: 1,
        });
        1
    }
}

fn value_references_resource(value: &Value, resource: &str) -> bool {
    match value {
        Value::String(text) => normalize_resource(text).contains(resource),
        Value::Array(items) => items
            .iter()
            .any(|item| value_references_resource(item, resource)),
        Value::Object(fields) => fields
            .values()
            .any(|item| value_references_resource(item, resource)),
        _ => false,
    }
}

fn normalize_resource(value: &str) -> String {
    value
        .trim()
        .trim_matches(['\'', '"'])
        .replace('/', "\\")
        .trim_start_matches("\\\\?\\")
        .to_lowercase()
}

fn is_exact_edit_tool(name: &str) -> bool {
    matches!(
        name,
        "edit_text_file" | "edit_text_file_structure" | "save_artifact"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejected_edit_blocks_only_mutations_referencing_the_same_resource() {
        let mut guard = ExactEditGuard::default();
        let edit = json!({"path": "C:/work/result.csv"});
        guard.record_result(
            "edit_text_file",
            &edit,
            &mut json!({"ok": false, "code": "ERR_PRECONDITION"}),
        );

        assert!(
            guard
                .before_action(
                    "run_command",
                    &json!({"command": "Set-Content C:\\work\\result.csv"})
                )
                .is_some()
        );
        assert!(
            guard
                .before_action("future_mutation", &json!({"path": "C:\\work\\result.csv"}))
                .is_some()
        );
        assert!(
            guard
                .before_action("run_command", &json!({"command": "Get-Date"}))
                .is_none()
        );
        assert!(
            guard
                .before_action("focus_window", &json!({"title": "Editor"}))
                .is_none()
        );
        assert!(
            guard
                .before_action("research_web", &json!({"query": "facts"}))
                .is_none()
        );
        assert!(guard.before_action("edit_text_file", &edit).is_none());

        guard.record_result(
            "edit_text_file",
            &edit,
            &mut json!({"ok": true, "effect_verified": true}),
        );
        assert!(
            guard
                .before_action(
                    "run_command",
                    &json!({"command": "Set-Content C:\\work\\result.csv"})
                )
                .is_none()
        );
    }

    #[test]
    fn each_resource_has_an_independent_boundary() {
        let mut guard = ExactEditGuard::default();
        let first = json!({"path": "C:/work/first.csv"});
        let second = json!({"path": "C:/work/second.csv"});
        guard.record_result("edit_text_file", &first, &mut json!({"ok": false}));
        guard.record_result("edit_text_file", &second, &mut json!({"ok": false}));
        guard.record_result(
            "edit_text_file",
            &first,
            &mut json!({"ok": true, "effect_verified": true}),
        );

        assert!(
            guard
                .before_action("run_command", &json!({"command": "write first.csv"}))
                .is_none()
        );
        assert!(
            guard
                .before_action(
                    "run_command",
                    &json!({"command": "write C:/work/second.csv"})
                )
                .is_some()
        );
    }

    #[test]
    fn turn_reset_releases_all_resource_boundaries() {
        let mut guard = ExactEditGuard::default();
        let edit = json!({"path": "C:/work/result.csv"});
        guard.record_result("edit_text_file", &edit, &mut json!({"ok": false}));
        guard.reset();
        assert!(guard.before_action("future_mutation", &edit).is_none());
    }

    #[test]
    fn rejected_artifact_target_cannot_be_reexpressed_through_a_command() {
        let mut guard = ExactEditGuard::default();
        let args = json!({"path": "C:/work/report.txt"});
        guard.record_result(
            "save_artifact",
            &args,
            &mut json!({"ok": false, "code": "ERR_FILE_TARGET_REQUEST_CONTRACT_REJECTED"}),
        );
        assert!(
            guard
                .before_action(
                    "run_command",
                    &json!({"command": "write C:/work/report.txt"})
                )
                .is_some()
        );
    }

    #[test]
    fn contract_denial_bounds_changed_argument_retries_by_resource_and_effect() {
        let mut guard = ExactEditGuard::default();
        let args = json!({"path": "C:/work/result.csv"});
        let mut denied = json!({
            "ok": false,
            "code": "ERR_TEXT_FILE_STRUCTURE_REQUEST_CONTRACT_REJECTED",
            "effect_may_have_occurred": false,
        });
        guard.record_result("edit_text_file_structure", &args, &mut denied);
        assert!(denied.get("terminal_blocker").is_none());

        let mut first_retry = json!({
            "ok": false,
            "code": "ERR_TEXT_FILE_STRUCTURE_CHANGE_REQUIRES_EXPLICIT_TOOL",
            "effect_may_have_occurred": false,
        });
        guard.record_result("edit_text_file", &args, &mut first_retry);
        assert!(first_retry.get("terminal_blocker").is_none());

        let mut final_retry = json!({
            "ok": false,
            "code": "ERR_TEXT_FILE_STRUCTURE_CHANGE_REQUIRES_EXPLICIT_TOOL",
            "effect_may_have_occurred": false,
        });
        guard.record_result("edit_text_file", &args, &mut final_retry);
        assert_eq!(
            final_retry["code"],
            "ERR_TEXT_FILE_STRUCTURE_REJECTION_LIMIT"
        );
        assert_eq!(final_retry["terminal_blocker"], true);
        assert_eq!(final_retry["effect_may_have_occurred"], false);
    }

    #[test]
    fn corrected_content_only_success_clears_the_structural_rejection_latch() {
        let mut guard = ExactEditGuard::default();
        let args = json!({"path": "C:/work/result.csv"});
        let mut denied = json!({
            "ok": false,
            "code": "ERR_TEXT_FILE_STRUCTURE_REQUEST_CONTRACT_REJECTED",
        });
        guard.record_result("edit_text_file_structure", &args, &mut denied);
        let mut retry = json!({
            "ok": false,
            "code": "ERR_TEXT_FILE_STRUCTURE_CHANGE_REQUIRES_EXPLICIT_TOOL",
        });
        guard.record_result("edit_text_file", &args, &mut retry);

        let mut corrected = json!({"ok": true, "effect_verified": true});
        guard.record_result("edit_text_file", &args, &mut corrected);
        let mut later_denial = json!({
            "ok": false,
            "code": "ERR_TEXT_FILE_STRUCTURE_REQUEST_CONTRACT_REJECTED",
        });
        guard.record_result("edit_text_file_structure", &args, &mut later_denial);
        assert!(later_denial.get("terminal_blocker").is_none());
    }

    #[test]
    fn two_match_misses_require_a_fresh_exact_read_before_another_edit() {
        let mut guard = ExactEditGuard::default();
        let args = json!({"path": "C:/work/result.csv"});
        for _ in 0..MATCH_MISS_REREAD_LIMIT {
            guard.record_result(
                "edit_text_file",
                &args,
                &mut json!({"ok": false, "code": "ERR_TEXT_FILE_MATCH_MISSING"}),
            );
        }
        assert_eq!(
            guard.before_action("edit_text_file", &args).unwrap()["code"],
            "ERR_TEXT_FILE_REREAD_REQUIRED"
        );

        guard.record_result(
            "read_text_file",
            &json!({"path": "C:/work/other.csv"}),
            &mut json!({"ok": true, "path": "C:/work/other.csv"}),
        );
        assert!(guard.before_action("edit_text_file", &args).is_some());

        guard.record_result(
            "read_text_file",
            &args,
            &mut json!({"ok": true, "path": "\\\\?\\C:\\work\\result.csv"}),
        );
        assert!(guard.before_action("edit_text_file", &args).is_none());
    }
}
