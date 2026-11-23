//! Turn-scoped evidence for rejected exact-file transactions.

use serde_json::{Value, json};
use std::path::Path;

mod identity;
use identity::Snapshot;

const STRUCTURAL_REJECTION_LIMIT: u8 = 3;
const MATCH_MISS_REREAD_LIMIT: u8 = 2;

#[derive(Default)]
pub(super) struct ExactEditGuard {
    resources: Vec<Resource>,
    pending: Option<Pending>,
}

struct Resource {
    path: String,
    snapshot: Option<Snapshot>,
    identity_error: Option<String>,
    tool: String,
    code: Option<String>,
    match_misses: u8,
    structural_rejections: u8,
}

struct Pending {
    tool: String,
    path: String,
    snapshot: Option<Snapshot>,
    identity_error: Option<String>,
}

impl ExactEditGuard {
    pub(super) fn reset(&mut self) {
        self.resources.clear();
        self.pending = None;
    }

    pub(super) fn before_action(&mut self, name: &str, args: &Value) -> Option<Value> {
        self.pending = None;
        if !is_dedicated_file_tool(name) && name != "read_text_file" {
            return None;
        }
        let path = file_target(args)?;
        let (snapshot, identity_error) = match Snapshot::capture(Path::new(path)) {
            Ok(snapshot) => (Some(snapshot), None),
            Err(error) => (None, Some(error.to_string())),
        };
        let misses = snapshot
            .as_ref()
            .map_or(0, |snapshot| self.related_counts(snapshot).0);
        let unknown_with_boundary = snapshot.is_none() && !self.resources.is_empty();
        self.pending = Some(Pending {
            tool: name.to_string(),
            path: path.to_string(),
            snapshot,
            identity_error,
        });
        if name != "read_text_file" && unknown_with_boundary {
            return Some(json!({
                "ok":false,"code":"ERR_EXACT_EDIT_RESOURCE_IDENTITY_UNAVAILABLE",
                "effect_may_have_occurred":false,"effect_verified":false,"executed":false,
                "instruction":"The active resource boundary could not be resolved for this exact transaction. Inspect the file's current availability; unavailable identity cannot retire earlier evidence."
            }));
        }
        if name == "edit_text_file" && misses >= MATCH_MISS_REREAD_LIMIT {
            return Some(json!({
                "ok":false,"code":"ERR_TEXT_FILE_REREAD_REQUIRED",
                "error":"two exact replacement attempts missed the current resource version",
                "effect_may_have_occurred":false,"effect_verified":false,"executed":false,"retryable":true,
                "instruction":"Read this exact file again, copy the current contiguous old_text literally, then retry once. Changing replacement text without a fresh read cannot clear this checkpoint."
            }));
        }
        None
    }

    pub(super) fn needs_scope_check(&self, name: &str) -> bool {
        !self.resources.is_empty()
            && !matches!(name, "edit_text_file" | "edit_text_file_structure")
            && super::super::turn_policy::is_mutating_tool(name)
    }

    pub(super) fn protected_resources(&self) -> Value {
        json!(self.resources.iter().map(|resource| {
            let current = Snapshot::capture(Path::new(&resource.path)).ok();
            json!({
                "requested_path":resource.path,"rejected_tool":resource.tool,"last_rejection_code":resource.code,
                "recorded_identity":resource.snapshot.as_ref().map(Snapshot::description),
                "current_identity":current.as_ref().map(Snapshot::description),
                "identity_error":resource.identity_error,
            })
        }).collect::<Vec<_>>())
    }

    pub(super) fn pending_identity(&self) -> Option<Value> {
        self.pending.as_ref().map(|pending| json!({"requested_path":pending.path,
            "identity":pending.snapshot.as_ref().map(Snapshot::description),"identity_error":pending.identity_error}))
    }

    pub(super) fn record_result(&mut self, name: &str, args: &Value, result: &mut Value) {
        let Some(pending) = self.pending.take().filter(|pending| {
            pending.tool == name
                && args.get("path").and_then(Value::as_str) == Some(pending.path.as_str())
        }) else {
            return;
        };
        if name == "read_text_file" {
            if result.get("ok").and_then(Value::as_bool) == Some(true)
                && let Some(before) = pending.snapshot.as_ref()
                && let Ok(after) = Snapshot::capture(Path::new(&pending.path))
                && before.has_file()
                && before.same_entry(&after)
                && before.same_file(&after)
                && self.related_counts(before).0 > 0
            {
                let reconciled = after
                    .while_receipt_matches(result, || {
                        for resource in &mut self.resources {
                            if resource
                                .snapshot
                                .as_ref()
                                .is_some_and(|snapshot| snapshot.related(before))
                            {
                                resource.match_misses = 0;
                            }
                        }
                    })
                    .is_some();
                result["resource_boundary_reconciled"] = json!(reconciled);
            }
            return;
        }
        let succeeded = result.get("ok").and_then(Value::as_bool) == Some(true)
            && result.get("effect_verified").and_then(Value::as_bool) == Some(true);
        if succeeded {
            if let Some(before) = pending.snapshot.as_ref()
                && self.resources.iter().any(|resource| {
                    resource
                        .snapshot
                        .as_ref()
                        .is_some_and(|snapshot| snapshot.related(before))
                })
                && let Ok(after) = Snapshot::capture(Path::new(&pending.path))
                && after.has_file()
                && before.same_entry(&after)
                && (result.get("atomic").and_then(Value::as_bool) == Some(true)
                    || !before.has_file()
                    || before.same_file(&after))
            {
                let in_place = before.same_file(&after);
                let reconciled = after
                    .while_receipt_matches(result, || {
                        self.resources.retain(|resource| {
                            !resource.snapshot.as_ref().is_some_and(|snapshot| {
                                snapshot.same_entry(before)
                                    || (in_place
                                        && snapshot.same_file(before)
                                        && Snapshot::capture(Path::new(&resource.path)).is_ok_and(
                                            |current| {
                                                snapshot.same_entry(&current)
                                                    && current.same_file(&after)
                                            },
                                        ))
                            })
                        });
                    })
                    .is_some();
                result["resource_boundary_reconciled"] = json!(reconciled);
            }
            return;
        }
        let previous_structural = pending
            .snapshot
            .as_ref()
            .map_or(0, |snapshot| self.related_counts(snapshot).1);
        let index = self
            .resources
            .iter()
            .position(
                |resource| match (resource.snapshot.as_ref(), pending.snapshot.as_ref()) {
                    (Some(existing), Some(current)) => existing.same_entry(current),
                    (None, None) => resource.path == pending.path,
                    _ => false,
                },
            )
            .unwrap_or_else(|| {
                self.resources.push(Resource {
                    path: pending.path.clone(),
                    snapshot: None,
                    identity_error: None,
                    tool: name.to_string(),
                    code: None,
                    match_misses: 0,
                    structural_rejections: 0,
                });
                self.resources.len() - 1
            });
        let code = result
            .get("code")
            .and_then(Value::as_str)
            .map(str::to_string);
        let resource = &mut self.resources[index];
        resource.snapshot = pending.snapshot;
        resource.identity_error = pending.identity_error;
        resource.tool = name.to_string();
        resource.code = code.clone();
        if code.as_deref() == Some("ERR_TEXT_FILE_MATCH_MISSING") {
            resource.match_misses = resource.match_misses.saturating_add(1);
        }
        let denied = code.as_deref() == Some("ERR_TEXT_FILE_STRUCTURE_REQUEST_CONTRACT_REJECTED");
        let structural = matches!(
            code.as_deref(),
            Some(
                "ERR_TEXT_FILE_STRUCTURE_CHANGE_REQUIRES_EXPLICIT_TOOL"
                    | "ERR_TEXT_FILE_STRUCTURE_CHANGE"
                    | "ERR_TEXT_FILE_FORMULA_MIXED_EDIT"
            )
        );
        if denied || (structural && previous_structural > 0) {
            resource.structural_rejections = resource.structural_rejections.saturating_add(1);
        }
        let count = self.resources[index]
            .snapshot
            .as_ref()
            .map_or(0, |snapshot| self.related_counts(snapshot).1);
        if count >= STRUCTURAL_REJECTION_LIMIT {
            structural_limit(
                result,
                code.as_deref().unwrap_or("ERR_TEXT_FILE_STRUCTURE_CHANGE"),
            );
        }
    }

    fn related_counts(&self, current: &Snapshot) -> (u8, u8) {
        self.resources
            .iter()
            .filter(|resource| {
                resource
                    .snapshot
                    .as_ref()
                    .is_some_and(|snapshot| snapshot.related(current))
            })
            .fold((0u8, 0u8), |(misses, structural), resource| {
                (
                    misses.saturating_add(resource.match_misses),
                    structural.saturating_add(resource.structural_rejections),
                )
            })
    }
}

fn structural_limit(result: &mut Value, cause: &str) {
    if let Some(fields) = result.as_object_mut() {
        fields.insert(
            "code".to_string(),
            json!("ERR_TEXT_FILE_STRUCTURE_REJECTION_LIMIT"),
        );
        fields.insert("cause_code".to_string(), json!(cause));
        fields.insert("terminal_blocker".to_string(), json!(true));
        fields.insert("retryable".to_string(), json!(false));
        fields.insert("error".to_string(), json!("the same resource reached the bounded limit for structural or formula changes after independent request-contract rejection"));
        fields.insert("instruction".to_string(), json!("Stop this turn. Do not retry the structural effect or bypass it through another tool; report the blocker once."));
    }
}

fn file_target(args: &Value) -> Option<&str> {
    args.get("path").and_then(Value::as_str).filter(|path| {
        !path.contains('\0') && Path::new(path).is_absolute() && !Path::new(path).is_dir()
    })
}

fn is_dedicated_file_tool(name: &str) -> bool {
    matches!(
        name,
        "edit_text_file" | "edit_text_file_structure" | "save_artifact"
    )
}

#[cfg(test)]
#[path = "exact_edit_guard_tests.rs"]
mod tests;
