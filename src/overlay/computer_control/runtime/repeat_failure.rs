//! Turn-scoped circuit for repeatedly equivalent structured tool failures.

use std::collections::VecDeque;

use serde_json::{Value, json};

use super::super::controller::world::SurfaceIdentity;

const MAX_FAILURE_KEYS: usize = 32;
const FAILURE_LIMIT: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FailureKey {
    request: u64,
    class: u64,
}

#[derive(Debug, Clone, Copy)]
struct FailureEntry {
    key: FailureKey,
    count: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DispatchOutcome {
    Succeeded,
    Failed(u64),
    Ambiguous,
}

/// Allows one retry of the same operation and arguments after a failure. A
/// third equivalent attempt is stopped only when the same structured failure
/// class was observed twice during the same user turn.
#[derive(Debug, Default)]
pub(super) struct RepeatFailureGuard {
    turn_id: Option<u64>,
    failures: VecDeque<FailureEntry>,
}

impl RepeatFailureGuard {
    pub(super) fn blocked_result(
        &mut self,
        turn_id: u64,
        operation: &str,
        arguments: &Value,
        surface: Option<&SurfaceIdentity>,
    ) -> Option<Value> {
        self.begin_turn(turn_id);
        let request = request_fingerprint(operation, arguments, surface);
        self.failures
            .iter()
            .any(|entry| entry.key.request == request && entry.count >= FAILURE_LIMIT)
            .then(|| {
                json!({
                    "ok": false,
                    "code": "ERR_EQUIVALENT_FAILURE_LIMIT",
                    "error": "An equivalent operation returned the same structured failure twice during this user turn.",
                    "executed": false,
                    "status": "blocked_repeat_failure",
                    "effect_may_have_occurred": false,
                    "retryable": false,
                    "instruction": "Do not repeat this operation with equivalent arguments again this turn. Change the operation or arguments only when current evidence justifies it; otherwise report the blocker once.",
                })
            })
    }

    /// Observe the direct dispatch result. Successful results clear prior
    /// failures for that exact request; failures without a public error class use
    /// a content-free structural fallback class.
    pub(super) fn observe(
        &mut self,
        turn_id: u64,
        operation: &str,
        arguments: &Value,
        surface: Option<&SurfaceIdentity>,
        result: &Value,
    ) -> bool {
        self.begin_turn(turn_id);
        let request = request_fingerprint(operation, arguments, surface);
        let class = match dispatch_outcome(result) {
            DispatchOutcome::Succeeded => {
                self.failures.retain(|entry| entry.key.request != request);
                return false;
            }
            DispatchOutcome::Failed(class) => class,
            DispatchOutcome::Ambiguous => return false,
        };
        let key = FailureKey { request, class };
        if let Some(index) = self.failures.iter().position(|entry| entry.key == key) {
            let mut entry = self.failures.remove(index).expect("located failure entry");
            entry.count = entry.count.saturating_add(1);
            let reached_limit = entry.count == FAILURE_LIMIT;
            self.failures.push_back(entry);
            reached_limit
        } else {
            self.failures.push_back(FailureEntry { key, count: 1 });
            while self.failures.len() > MAX_FAILURE_KEYS {
                self.failures.pop_front();
            }
            false
        }
    }

    fn begin_turn(&mut self, turn_id: u64) {
        if self.turn_id != Some(turn_id) {
            self.turn_id = Some(turn_id);
            self.failures.clear();
        }
    }
}

fn dispatch_outcome(result: &Value) -> DispatchOutcome {
    match result.get("ok").and_then(Value::as_bool) {
        Some(true) => return DispatchOutcome::Succeeded,
        Some(false) => {}
        None => return DispatchOutcome::Ambiguous,
    }
    let class = [
        "/code",
        "/error/code",
        "/error/class",
        "/error/kind",
        "/error/type",
        "/status",
    ]
    .into_iter()
    .find_map(|pointer| {
        result
            .pointer(pointer)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
    });
    let mut hash = match class {
        Some(value) => hash_tagged(0xcbf29ce484222325, b'c', value.as_bytes()),
        None => hash_tagged(0xcbf29ce484222325, b'f', b"semantic"),
    };
    let retryability = result.get("retryable").and_then(Value::as_bool);
    hash = hash_tagged(
        hash,
        b'r',
        &[match retryability {
            Some(false) => 0,
            Some(true) => 1,
            None => 2,
        }],
    );
    DispatchOutcome::Failed(hash)
}

fn request_fingerprint(
    operation: &str,
    arguments: &Value,
    surface: Option<&SurfaceIdentity>,
) -> u64 {
    let hash = hash_tagged(0xcbf29ce484222325, b'o', operation.as_bytes());
    let hash = hash_value(hash, arguments);
    hash_surface(hash, surface)
}

fn hash_surface(mut hash: u64, surface: Option<&SurfaceIdentity>) -> u64 {
    match surface {
        Some(SurfaceIdentity::Native {
            hwnd,
            pid,
            generation,
        }) => {
            hash = hash_tagged(hash, b'w', &hwnd.to_le_bytes());
            hash = hash_tagged(hash, b'p', &pid.to_le_bytes());
            hash_tagged(hash, b'g', &generation.to_le_bytes())
        }
        Some(SurfaceIdentity::Browser {
            tab_id,
            document_id,
            ..
        }) => {
            hash = hash_tagged(hash, b't', &tab_id.to_le_bytes());
            hash_tagged(hash, b'i', document_id.as_bytes())
        }
        None => hash_tagged(hash, b'?', &[]),
    }
}

fn hash_value(mut hash: u64, value: &Value) -> u64 {
    match value {
        Value::Null => hash_tagged(hash, b'n', &[]),
        Value::Bool(flag) => hash_tagged(hash, b'b', &[*flag as u8]),
        Value::Number(number) => hash_tagged(hash, b'd', number.to_string().as_bytes()),
        Value::String(text) => hash_tagged(hash, b's', text.as_bytes()),
        Value::Array(items) => {
            hash = hash_tagged(hash, b'[', &(items.len() as u64).to_le_bytes());
            for item in items {
                hash = hash_value(hash, item);
            }
            hash_tagged(hash, b']', &[])
        }
        Value::Object(object) => {
            let mut keys = object.keys().collect::<Vec<_>>();
            keys.sort_unstable();
            hash = hash_tagged(hash, b'{', &(keys.len() as u64).to_le_bytes());
            for key in keys {
                hash = hash_tagged(hash, b'k', key.as_bytes());
                hash = hash_value(hash, &object[key]);
            }
            hash_tagged(hash, b'}', &[])
        }
    }
}

fn hash_tagged(mut hash: u64, tag: u8, bytes: &[u8]) -> u64 {
    const FNV_PRIME: u64 = 0x100000001b3;
    hash ^= u64::from(tag);
    hash = hash.wrapping_mul(FNV_PRIME);
    for byte in (bytes.len() as u64).to_le_bytes().iter().chain(bytes) {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    fn browser_window() -> crate::overlay::computer_control::controller::world::BrowserWindowIdentity
    {
        crate::overlay::computer_control::controller::world::BrowserWindowIdentity {
            browser_window_id: 2,
            hwnd: 3,
            pid: 4,
            generation: 5,
        }
    }

    #[test]
    fn equivalent_failure_allows_one_retry_then_blocks() {
        let mut guard = RepeatFailureGuard::default();
        let args = json!({"target": 7, "options": {"mode": "x"}});
        let failure = json!({"ok": false, "code": "ERR_TEMPORARY"});

        assert!(
            guard
                .blocked_result(4, "future_operation", &args, None)
                .is_none()
        );
        assert!(!guard.observe(4, "future_operation", &args, None, &failure));
        assert!(
            guard
                .blocked_result(4, "future_operation", &args, None)
                .is_none()
        );
        assert!(guard.observe(4, "future_operation", &args, None, &failure));
        assert!(
            guard
                .blocked_result(4, "future_operation", &args, None)
                .is_some()
        );
    }

    #[test]
    fn different_arguments_error_classes_and_turns_remain_available() {
        let mut guard = RepeatFailureGuard::default();
        let first = json!({"slot": 1});
        let second = json!({"slot": 2});
        let class_a = json!({"ok": false, "error": {"class": "A"}});
        let class_b = json!({"ok": false, "error": {"class": "B"}});

        guard.observe(9, "future_operation", &first, None, &class_a);
        assert!(guard.observe(9, "future_operation", &first, None, &class_a));
        assert!(
            guard
                .blocked_result(9, "future_operation", &second, None)
                .is_none()
        );
        assert!(!guard.observe(9, "future_operation", &second, None, &class_b));
        assert!(
            guard
                .blocked_result(10, "future_operation", &first, None)
                .is_none()
        );
        guard.observe(10, "future_operation", &first, None, &class_a);
        guard.observe(10, "future_operation", &first, None, &class_b);
        assert!(
            guard
                .blocked_result(10, "future_operation", &first, None)
                .is_none()
        );
    }

    #[test]
    fn a_success_clears_the_equivalent_failure_streak() {
        let mut guard = RepeatFailureGuard::default();
        let args = json!({"slot": 3});
        let failure = json!({"ok": false, "error": "details are not retained"});
        guard.observe(2, "unknown_future_tool", &args, None, &failure);
        guard.observe(2, "unknown_future_tool", &args, None, &failure);
        assert!(
            guard
                .blocked_result(2, "unknown_future_tool", &args, None)
                .is_some()
        );

        assert!(!guard.observe(
            2,
            "unknown_future_tool",
            &args,
            None,
            &json!({"ok": true, "error": {"code": "advisory_metadata"}})
        ));
        assert!(
            guard
                .blocked_result(2, "unknown_future_tool", &args, None)
                .is_none()
        );

        guard.observe(2, "unknown_future_tool", &args, None, &failure);
        guard.observe(2, "unknown_future_tool", &args, None, &failure);
        guard.observe(
            2,
            "unknown_future_tool",
            &args,
            None,
            &json!({"ok": true, "error": ""}),
        );
        assert!(
            guard
                .blocked_result(2, "unknown_future_tool", &args, None)
                .is_none()
        );
    }

    #[test]
    fn ambiguous_error_metadata_neither_counts_nor_clears() {
        let mut guard = RepeatFailureGuard::default();
        let args = json!({"slot": 4});
        let failure = json!({"ok": false, "code": "ERR_TYPED"});
        assert!(!guard.observe(3, "future_operation", &args, None, &failure));
        assert!(!guard.observe(
            3,
            "future_operation",
            &args,
            None,
            &json!({"error": {"code": "ERR_TYPED"}}),
        ));
        assert!(guard.observe(3, "future_operation", &args, None, &failure));

        let other = json!({"slot": 5});
        guard.observe(
            3,
            "future_operation",
            &other,
            None,
            &json!({"error": "unclassified"}),
        );
        guard.observe(
            3,
            "future_operation",
            &other,
            None,
            &json!({"error": "unclassified"}),
        );
        assert!(
            guard
                .blocked_result(3, "future_operation", &other, None)
                .is_none()
        );
    }

    #[test]
    fn a_changed_document_or_window_generation_gets_a_fresh_retry_budget() {
        let mut guard = RepeatFailureGuard::default();
        let args = json!({"slot": 6});
        let failure = json!({"ok": false, "code": "ERR_STATE"});
        let first_document = SurfaceIdentity::Browser {
            tab_id: 17,
            document_id: "document-a".to_string(),
            window: browser_window(),
        };
        let next_document = SurfaceIdentity::Browser {
            tab_id: 17,
            document_id: "document-b".to_string(),
            window: browser_window(),
        };
        guard.observe(
            5,
            "future_operation",
            &args,
            Some(&first_document),
            &failure,
        );
        guard.observe(
            5,
            "future_operation",
            &args,
            Some(&first_document),
            &failure,
        );
        assert!(
            guard
                .blocked_result(5, "future_operation", &args, Some(&first_document))
                .is_some()
        );
        assert!(
            guard
                .blocked_result(5, "future_operation", &args, Some(&next_document))
                .is_none()
        );

        let first_window = SurfaceIdentity::Native {
            hwnd: 31,
            pid: 41,
            generation: 1,
        };
        let next_window = SurfaceIdentity::Native {
            hwnd: 31,
            pid: 41,
            generation: 2,
        };
        guard.observe(5, "future_operation", &args, Some(&first_window), &failure);
        guard.observe(5, "future_operation", &args, Some(&first_window), &failure);
        assert!(
            guard
                .blocked_result(5, "future_operation", &args, Some(&first_window))
                .is_some()
        );
        assert!(
            guard
                .blocked_result(5, "future_operation", &args, Some(&next_window))
                .is_none()
        );
    }

    #[test]
    fn typed_retryability_is_part_of_the_failure_class() {
        let mut guard = RepeatFailureGuard::default();
        let args = json!({"slot": 7});
        let retryable = json!({"ok": false, "code": "ERR_STATE", "retryable": true});
        let final_failure = json!({"ok": false, "code": "ERR_STATE", "retryable": false});
        guard.observe(7, "future_operation", &args, None, &retryable);
        guard.observe(7, "future_operation", &args, None, &final_failure);
        assert!(
            guard
                .blocked_result(7, "future_operation", &args, None)
                .is_none()
        );
        assert!(guard.observe(7, "future_operation", &args, None, &retryable));
        assert!(
            guard
                .blocked_result(7, "future_operation", &args, None)
                .is_some()
        );
    }

    #[test]
    fn fingerprints_do_not_retain_argument_or_error_content() {
        let mut guard = RepeatFailureGuard::default();
        let hidden_argument = "argument-content-must-not-survive";
        let hidden_class = "error-content-must-not-survive";
        guard.observe(
            8,
            "future_operation",
            &json!({"value": hidden_argument}),
            None,
            &json!({"ok": false, "error": {"class": hidden_class}}),
        );

        let debug = format!("{guard:?}");
        assert!(!debug.contains(hidden_argument));
        assert!(!debug.contains(hidden_class));
    }
}
