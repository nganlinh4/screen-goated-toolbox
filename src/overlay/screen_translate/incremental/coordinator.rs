//! Scene scheduling balances source completeness against measured request cost.
use super::super::contract::DetectedTextRegion;
use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

pub(super) const LANES: usize = 2;
const MAX_BYTES: usize = 12_000;

#[derive(Clone, Copy)]
pub(super) struct Cost {
    pub first_ms: f64,
    pub bytes_per_ms: f64,
}

impl Default for Cost {
    fn default() -> Self {
        Self {
            first_ms: 450.0,
            bytes_per_ms: 4.0,
        }
    }
}

static COSTS: LazyLock<Mutex<HashMap<String, Cost>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

struct Flight {
    started: Instant,
    bytes: usize,
    first: Option<(Instant, usize)>,
    sources: HashSet<String>,
}

pub(super) struct Dispatch {
    pub lane: usize,
    pub candidates: Vec<DetectedTextRegion>,
}

pub(super) struct Coordinator {
    pending: Vec<usize>,
    completed: HashSet<u16>,
    ready_since: Option<Instant>,
    flights: [Option<Flight>; LANES],
    cost: Cost,
    dispatched: bool,
}

impl Coordinator {
    pub(super) fn new(candidates: &[DetectedTextRegion], model: &str) -> Self {
        Self::with_cost(
            candidates,
            COSTS
                .lock()
                .unwrap()
                .get(model)
                .copied()
                .unwrap_or_default(),
        )
    }

    pub(super) fn with_cost(candidates: &[DetectedTextRegion], cost: Cost) -> Self {
        Self {
            pending: (0..candidates.len()).collect(),
            completed: HashSet::new(),
            ready_since: None,
            flights: [None, None],
            cost,
            dispatched: false,
        }
    }

    pub(super) fn completed(&mut self, id: u16, now: Instant) {
        if self.completed.insert(id) {
            self.ready_since.get_or_insert(now);
        }
    }

    pub(super) fn first_output(&mut self, lane: usize, source_bytes: usize, now: Instant) {
        if let Some(flight) = &mut self.flights[lane] {
            flight.first.get_or_insert((now, source_bytes));
        }
    }

    pub(super) fn finished(&mut self, lane: usize, now: Instant) {
        if let Some(flight) = self.flights[lane].take()
            && let Some((first, first_bytes)) = flight.first
        {
            let first_ms = first
                .saturating_duration_since(flight.started)
                .as_secs_f64()
                * 1000.0;
            let tail_ms = now.saturating_duration_since(first).as_secs_f64() * 1000.0;
            let startup_ms = first_ms - first_bytes as f64 / self.cost.bytes_per_ms;
            self.cost.first_ms = self.cost.first_ms * 0.75 + startup_ms.clamp(80.0, 1500.0) * 0.25;
            let tail_bytes = flight.bytes.saturating_sub(first_bytes);
            if tail_ms >= 20.0 && tail_bytes > 0 {
                let rate = (tail_bytes as f64 / tail_ms).clamp(0.5, 30.0);
                self.cost.bytes_per_ms = self.cost.bytes_per_ms * 0.75 + rate * 0.25;
            }
        }
    }

    pub(super) fn remember(&self, model: &str) {
        let mut costs = COSTS.lock().unwrap();
        if costs.len() >= 64 && !costs.contains_key(model) {
            costs.clear();
        }
        costs.insert(model.to_string(), self.cost);
    }

    pub(super) fn idle(&self) -> bool {
        self.flights.iter().all(Option::is_none)
    }
    pub(super) fn done(&self) -> bool {
        self.pending.is_empty() && self.idle()
    }

    pub(super) fn take_ready(
        &mut self,
        candidates: &[DetectedTextRegion],
        now: Instant,
    ) -> Option<Dispatch> {
        let lane = self.flights.iter().position(Option::is_none)?;
        self.pending.retain(|&i| {
            !self.completed.contains(&candidates[i].id) || !candidates[i].source_text.is_empty()
        });
        let ready: Vec<_> = self
            .pending
            .iter()
            .copied()
            .filter(|&i| self.completed.contains(&candidates[i].id))
            .collect();
        if ready.is_empty() {
            self.ready_since = None;
            return None;
        }
        let age = now.saturating_duration_since(*self.ready_since.get_or_insert(now));
        let complete = self.completed.len() == candidates.len();
        let bytes: usize = ready.iter().map(|&i| candidates[i].source_text.len()).sum();
        let target_bytes = if self.dispatched {
            MAX_BYTES
        } else {
            (self.cost.first_ms * self.cost.bytes_per_ms * 2.0).clamp(2048.0, MAX_BYTES as f64)
                as usize
        };
        if !complete {
            // Give a likely single-request scene more time to finish OCR.
            // Large scenes begin progressively; stalled OCR never resets the
            // deadline for text that is already ready.
            let remaining = candidates.len().saturating_sub(self.completed.len());
            let projected = bytes as f64 + bytes as f64 / ready.len() as f64 * remaining as f64;
            let wait_fraction = if projected <= target_bytes as f64 {
                0.5
            } else {
                0.25
            };
            let deadline = Duration::from_secs_f64(
                (self.cost.first_ms * wait_fraction).clamp(100.0, 300.0) / 1000.0,
            );
            if age < deadline && bytes < target_bytes {
                return None;
            }
        }
        if !self.idle() {
            // Overlap only with complete source context and already accepted
            // terminology. Repeated source units wait for consistent wording.
            if !complete || self.flights.iter().flatten().any(|f| f.first.is_none()) {
                return None;
            }
            if ready.iter().any(|&i| {
                self.flights
                    .iter()
                    .flatten()
                    .any(|f| f.sources.contains(candidates[i].source_text.trim()))
            }) {
                return None;
            }
            let remaining_ms = self
                .flights
                .iter()
                .flatten()
                .map(|f| {
                    let expected = self.cost.first_ms + f.bytes as f64 / self.cost.bytes_per_ms;
                    let elapsed = now.saturating_duration_since(f.started).as_secs_f64() * 1000.0;
                    if elapsed > expected {
                        self.cost.first_ms
                    } else {
                        expected - elapsed
                    }
                })
                .fold(0.0_f64, f64::max);
            if remaining_ms < self.cost.first_ms * 0.25 {
                return None;
            }
        }
        let mut chosen = Vec::new();
        let mut selected_bytes = 0;
        let limit = target_bytes;
        for i in ready {
            let size = candidates[i].source_text.len();
            if !chosen.is_empty() && selected_bytes + size > limit {
                break;
            }
            selected_bytes += size;
            chosen.push(i);
        }
        let selected: HashSet<_> = chosen.iter().copied().collect();
        self.pending.retain(|i| !selected.contains(i));
        let candidates: Vec<_> = chosen.into_iter().map(|i| candidates[i].clone()).collect();
        self.flights[lane] = Some(Flight {
            started: now,
            bytes: selected_bytes,
            first: None,
            sources: candidates
                .iter()
                .map(|c| c.source_text.trim().to_string())
                .collect(),
        });
        self.ready_since = None;
        self.dispatched = true;
        Some(Dispatch { lane, candidates })
    }
}

#[cfg(test)]
#[path = "coordinator_tests.rs"]
mod tests;
