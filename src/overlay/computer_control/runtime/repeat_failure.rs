//! Turn-scoped circuit for repeatedly equivalent structured tool outcomes.

use std::collections::VecDeque;

use serde_json::{Value, json};

use super::super::controller::world::SurfaceIdentity;

const MAX_FAILURE_KEYS: usize = 32;
const MAX_STABLE_PROCESS_KEYS: usize = 32;
const MAX_STALE_SURFACE_KEYS: usize = 8;
const FAILURE_LIMIT: u8 = 2;
const STABLE_PROCESS_LIMIT: u8 = 3;
const STALE_SURFACE_LIMIT: u8 = 2;

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
struct StableProcessKey {
    request: u64,
    result: u64,
}

#[derive(Debug, Clone, Copy)]
struct StableProcessEntry {
    key: StableProcessKey,
    count: u8,
}

#[derive(Debug, Clone, Copy)]
struct StaleSurfaceEntry {
    key: u64,
    count: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DispatchOutcome {
    Succeeded,
    Failed(u64),
    Ambiguous,
}

/// Allows one retry of an equivalent structured failure. It also bounds an
/// exact process whose same unverified result was already observed three times.
/// Changed arguments, changed results, new turns, and verified progress retain
/// their own budgets.
#[derive(Debug, Default)]
pub(super) struct RepeatFailureGuard {
    turn_id: Option<u64>,
    failures: VecDeque<FailureEntry>,
    stable_processes: VecDeque<StableProcessEntry>,
    stale_surfaces: VecDeque<StaleSurfaceEntry>,
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
        if is_indexed_action(operation)
            && let Some(key) = indexed_browser_surface_key(surface)
            && self
                .stale_surfaces
                .iter()
                .any(|entry| entry.key == key && entry.count >= STALE_SURFACE_LIMIT)
        {
            return Some(json!({
                "ok": false,
                "code": "ERR_STALE_SURFACE_RETRY_LIMIT",
                "error": "Indexed browser targets changed before dispatch twice on this surface.",
                "executed": false,
                "status": "blocked_stale_surface_loop",
                "effect_may_have_occurred": false,
                "retryable": false,
                "instruction": "Do not use act or do_steps again on this unchanged surface this turn. Use a non-indexed current-frame or direct-provider route, or report the blocker once.",
            }));
        }
        if self
            .failures
            .iter()
            .any(|entry| entry.key.request == request && entry.count >= FAILURE_LIMIT)
        {
            return Some(json!({
                "ok": false,
                "code": "ERR_EQUIVALENT_FAILURE_LIMIT",
                "error": "An equivalent operation returned the same structured failure twice during this user turn.",
                "executed": false,
                "status": "blocked_repeat_failure",
                "effect_may_have_occurred": false,
                "retryable": false,
                "instruction": "Do not repeat this operation with equivalent arguments again this turn. Change the operation or arguments only when current evidence justifies it; otherwise report the blocker once.",
            }));
        }
        self.stable_processes
            .iter()
            .any(|entry| {
                entry.key.request == request && entry.count >= STABLE_PROCESS_LIMIT
            })
            .then(|| {
                json!({
                    "ok": false,
                    "code": "ERR_EQUIVALENT_UNVERIFIED_RESULT_LIMIT",
                    "error": "An equivalent exact process returned the same unverified result three times during this user turn.",
                    "executed": false,
                    "status": "blocked_repeat_result",
                    "effect_may_have_occurred": false,
                    "retryable": false,
                    "instruction": "Do not run this exact process again this turn. Use existing evidence, verify with a different authoritative source, or change the operation only when current evidence justifies it.",
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
        let stale_surface_limit = self.observe_stale_surface(operation, surface, result);
        let class = match dispatch_outcome(result) {
            DispatchOutcome::Succeeded => {
                self.failures.retain(|entry| entry.key.request != request);
                return self.observe_stable_process(request, result) || stale_surface_limit;
            }
            DispatchOutcome::Failed(class) => {
                self.stable_processes
                    .retain(|entry| entry.key.request != request);
                class
            }
            DispatchOutcome::Ambiguous => return stale_surface_limit,
        };
        let key = FailureKey { request, class };
        if let Some(index) = self.failures.iter().position(|entry| entry.key == key) {
            let mut entry = self.failures.remove(index).expect("located failure entry");
            entry.count = entry.count.saturating_add(1);
            let reached_limit = entry.count == FAILURE_LIMIT;
            self.failures.push_back(entry);
            reached_limit || stale_surface_limit
        } else {
            self.failures.push_back(FailureEntry { key, count: 1 });
            while self.failures.len() > MAX_FAILURE_KEYS {
                self.failures.pop_front();
            }
            stale_surface_limit
        }
    }

    /// A verified world-state change makes earlier failures stale. The same
    /// request may now have a different outcome, so it receives a fresh bounded
    /// retry budget.
    pub(super) fn clear_after_verified_progress(&mut self, turn_id: u64) -> bool {
        self.begin_turn(turn_id);
        let cleared = !self.failures.is_empty()
            || !self.stable_processes.is_empty()
            || !self.stale_surfaces.is_empty();
        self.failures.clear();
        self.stable_processes.clear();
        self.stale_surfaces.clear();
        cleared
    }

    fn observe_stale_surface(
        &mut self,
        operation: &str,
        surface: Option<&SurfaceIdentity>,
        result: &Value,
    ) -> bool {
        if !is_indexed_action(operation)
            || result.get("code").and_then(Value::as_str) != Some("ERR_BROWSER_STALE_TARGET")
        {
            return false;
        }
        let Some(key) = indexed_browser_surface_key(surface) else {
            return false;
        };
        if let Some(entry) = self
            .stale_surfaces
            .iter_mut()
            .find(|entry| entry.key == key)
        {
            entry.count = entry.count.saturating_add(1);
            return entry.count == STALE_SURFACE_LIMIT;
        }
        self.stale_surfaces
            .push_back(StaleSurfaceEntry { key, count: 1 });
        while self.stale_surfaces.len() > MAX_STALE_SURFACE_KEYS {
            self.stale_surfaces.pop_front();
        }
        false
    }

    fn observe_stable_process(&mut self, request: u64, result: &Value) -> bool {
        let Some(result) = stable_process_result_fingerprint(result) else {
            self.stable_processes
                .retain(|entry| entry.key.request != request);
            return false;
        };
        let key = StableProcessKey { request, result };
        if let Some(index) = self
            .stable_processes
            .iter()
            .position(|entry| entry.key == key)
        {
            let mut entry = self
                .stable_processes
                .remove(index)
                .expect("located stable process entry");
            entry.count = entry.count.saturating_add(1);
            let reached_limit = entry.count == STABLE_PROCESS_LIMIT;
            self.stable_processes.push_back(entry);
            return reached_limit;
        }
        self.stable_processes
            .retain(|entry| entry.key.request != request);
        self.stable_processes
            .push_back(StableProcessEntry { key, count: 1 });
        while self.stable_processes.len() > MAX_STABLE_PROCESS_KEYS {
            self.stable_processes.pop_front();
        }
        false
    }

    fn begin_turn(&mut self, turn_id: u64) {
        if self.turn_id != Some(turn_id) {
            self.turn_id = Some(turn_id);
            self.failures.clear();
            self.stable_processes.clear();
            self.stale_surfaces.clear();
        }
    }
}

fn is_indexed_action(operation: &str) -> bool {
    matches!(operation, "act" | "do_steps")
}

fn indexed_browser_surface_key(surface: Option<&SurfaceIdentity>) -> Option<u64> {
    let SurfaceIdentity::Browser {
        tab_id,
        document_id,
        window,
    } = surface?
    else {
        return None;
    };
    let hash = hash_tagged(0xcbf29ce484222325, b't', &tab_id.to_le_bytes());
    let hash = hash_tagged(hash, b'd', document_id.as_bytes());
    let hash = hash_tagged(hash, b'w', &window.browser_window_id.to_le_bytes());
    let hash = hash_tagged(hash, b'h', &window.hwnd.to_le_bytes());
    let hash = hash_tagged(hash, b'p', &window.pid.to_le_bytes());
    Some(hash_tagged(hash, b'g', &window.generation.to_le_bytes()))
}

fn stable_process_result_fingerprint(result: &Value) -> Option<u64> {
    (result.get("ok").and_then(Value::as_bool) == Some(true)
        && result.get("process_completed").and_then(Value::as_bool) == Some(true)
        && result.get("effect_verified").and_then(Value::as_bool) != Some(true)
        && result.get("timed_out").and_then(Value::as_bool) != Some(true))
    .then(|| hash_value(hash_tagged(0xcbf29ce484222325, b'u', &[]), result))
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
#[path = "repeat_failure_tests.rs"]
mod tests;
