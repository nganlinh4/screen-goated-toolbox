use super::state::WINDOW_STATES;
use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, Mutex};
use std::time::Instant;
use windows::Win32::Foundation::HWND;

const MAX_ACTIVE_TRACES: usize = 512;

struct Trace {
    started: Instant,
    phases: HashMap<&'static str, f64>,
    window_phases: HashMap<&'static str, HashSet<isize>>,
}

static TRACES: LazyLock<Mutex<HashMap<String, Trace>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub(crate) fn begin(trace_id: &str) {
    let mut traces = TRACES.lock().unwrap();
    if traces.len() >= MAX_ACTIVE_TRACES {
        let mut oldest: Vec<_> = traces
            .iter()
            .map(|(id, trace)| (id.clone(), trace.started))
            .collect();
        oldest.sort_unstable_by_key(|(_, started)| *started);
        for (id, _) in oldest.into_iter().take(MAX_ACTIVE_TRACES / 4) {
            traces.remove(&id);
        }
    }
    traces.insert(
        trace_id.to_string(),
        Trace {
            started: Instant::now(),
            phases: HashMap::new(),
            window_phases: HashMap::new(),
        },
    );
    drop(traces);
    mark(trace_id, "input_committed");
}

pub(crate) fn mark(trace_id: &str, phase: &'static str) {
    let elapsed_ms = {
        let mut traces = TRACES.lock().unwrap();
        let Some(trace) = traces.get_mut(trace_id) else {
            return;
        };
        if trace.phases.contains_key(phase) {
            return;
        }
        let elapsed_ms = trace.started.elapsed().as_secs_f64() * 1000.0;
        trace.phases.insert(phase, elapsed_ms);
        elapsed_ms
    };
    crate::debug_log::log_debug(&format!(
        "[OverlayPerf] trace={trace_id} phase={phase} elapsed_ms={elapsed_ms:.1}"
    ));
}

pub(crate) fn mark_window(hwnd: HWND, phase: &'static str) {
    mark_id(hwnd.0 as isize, phase);
}

pub(crate) fn mark_id(id: isize, phase: &'static str) {
    let trace_id = WINDOW_STATES
        .lock()
        .unwrap()
        .get(&id)
        .and_then(|state| state.latency_trace_id.clone());
    if let Some(trace_id) = trace_id {
        mark(&trace_id, phase);
    }
}

fn mark_id_after(id: isize, phase: &'static str, prerequisite: &'static str) {
    let trace_id = WINDOW_STATES
        .lock()
        .unwrap()
        .get(&id)
        .and_then(|state| state.latency_trace_id.clone());
    let Some(trace_id) = trace_id else {
        return;
    };
    if record_window_phase_after(&trace_id, id, phase, prerequisite) {
        mark(&trace_id, phase);
    }
}

fn record_window_phase_after(
    trace_id: &str,
    id: isize,
    phase: &'static str,
    prerequisite: &'static str,
) -> bool {
    let mut traces = TRACES.lock().unwrap();
    let Some(trace) = traces.get_mut(trace_id) else {
        return false;
    };
    if !trace.phases.contains_key(prerequisite) {
        return false;
    }
    trace.window_phases.entry(phase).or_default().insert(id)
}

pub(crate) fn mark_card_phase(id: isize, phase: &str, payload_len: usize, text_len: usize) {
    let has_content = text_len > 0 || payload_len > 0;
    let trace_phase = match phase {
        "document_load_requested" => Some("document_load_requested"),
        "document_loaded" => Some("document_loaded"),
        "content_queued" => {
            if has_content {
                mark_id_after(id, "compositor_received", "provider_first_output");
            }
            None
        }
        "stream_applied" | "finalize_applied" => {
            if has_content {
                mark_id_after(id, "content_applied", "provider_first_output");
            }
            None
        }
        "stream_painted" => {
            if has_content {
                mark_id_after(id, "first_painted", "provider_first_output");
            }
            None
        }
        "final_painted" => {
            if has_content {
                mark_id_after(id, "first_painted", "provider_first_output");
                mark_id_after(id, "final_painted", "provider_first_output");
            }
            None
        }
        "final_fit_completed" => {
            mark_id_after(id, "final_fit_completed", "provider_first_output");
            None
        }
        "fit_timeout" => Some("fit_timeout"),
        _ => None,
    };
    if let Some(trace_phase) = trace_phase {
        mark_id(id, trace_phase);
    }
}

pub(crate) fn wait_for_phase(
    trace_id: &str,
    phase: &'static str,
    timeout: std::time::Duration,
) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if TRACES
            .lock()
            .unwrap()
            .get(trace_id)
            .is_some_and(|trace| trace.phases.contains_key(phase))
        {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    false
}

#[cfg(debug_assertions)]
pub(crate) fn wait_for_window_phase_count(
    trace_id: &str,
    phase: &'static str,
    expected: usize,
    timeout: std::time::Duration,
) -> usize {
    let deadline = Instant::now() + timeout;
    loop {
        let count = TRACES
            .lock()
            .unwrap()
            .get(trace_id)
            .and_then(|trace| trace.window_phases.get(phase))
            .map_or(0, HashSet::len);
        if count >= expected || Instant::now() >= deadline {
            return count;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[cfg(debug_assertions)]
pub(crate) fn snapshot(trace_id: &str) -> Vec<(&'static str, f64)> {
    let mut phases = TRACES
        .lock()
        .unwrap()
        .get(trace_id)
        .map(|trace| {
            trace
                .phases
                .iter()
                .map(|(phase, elapsed_ms)| (*phase, *elapsed_ms))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    phases.sort_unstable_by(|left, right| left.1.total_cmp(&right.1));
    phases
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_phase_is_recorded_only_once() {
        let trace_id = "latency-test:0";
        begin(trace_id);
        mark(trace_id, "provider_dispatch");
        mark(trace_id, "provider_dispatch");
        let traces = TRACES.lock().unwrap();
        let trace = traces.get(trace_id).unwrap();
        assert_eq!(trace.phases.len(), 2);
    }

    #[test]
    fn window_phase_counts_are_unique_per_result_window() {
        let trace_id = "latency-window-count-test:0";
        begin(trace_id);
        mark(trace_id, "provider_first_output");
        assert!(record_window_phase_after(
            trace_id,
            10,
            "final_painted",
            "provider_first_output"
        ));
        assert!(!record_window_phase_after(
            trace_id,
            10,
            "final_painted",
            "provider_first_output"
        ));
        assert!(record_window_phase_after(
            trace_id,
            11,
            "final_painted",
            "provider_first_output"
        ));
        assert_eq!(
            wait_for_window_phase_count(trace_id, "final_painted", 2, std::time::Duration::ZERO),
            2
        );
    }
}
