use super::super::contract::DetectedTextRegion;
use std::collections::HashSet;
use std::time::{Duration, Instant};

pub(super) struct Groups {
    pending: Vec<usize>,
    completed: HashSet<u16>,
    dispatched: bool,
    ready_since: Option<Instant>,
}

impl Groups {
    pub(super) fn new(candidates: &[DetectedTextRegion]) -> Self {
        // Inputs already own coherent source units. Scheduling never regroups them.
        let pending = (0..candidates.len()).collect();
        Self {
            pending,
            completed: HashSet::new(),
            dispatched: false,
            ready_since: None,
        }
    }
    pub(super) fn completed(&mut self, id: u16) {
        self.completed.insert(id);
    }
    pub(super) fn take_ready(
        &mut self,
        candidates: &[DetectedTextRegion],
        now: Instant,
    ) -> Vec<DetectedTextRegion> {
        // Coalesce ready units, but bound idle time independently
        // of capture size and slow unfinished OCR regions.
        let complete = self.completed.len() == candidates.len();
        let ready_count = self.observe_ready(candidates, now);
        if ready_count == 0 {
            return Vec::new();
        }
        let since = self.ready_since.unwrap();
        let ready_bytes: usize = self
            .pending
            .iter()
            .filter(|&&i| self.completed.contains(&candidates[i].id))
            .map(|&i| candidates[i].source_text.len())
            .sum();
        let budget = Duration::from_millis(600);
        let elapsed = now.saturating_duration_since(since);
        // Keep an early reveal, then amortize subsequent requests over actual
        // text volume rather than counts of tiny labels. A bounded deadline
        // still releases ready text if other OCR regions stall.
        if !complete
            && ready_bytes < 12_000
            && elapsed < budget
            && (self.dispatched || ready_count < 8 || elapsed < Duration::from_millis(100))
        {
            return Vec::new();
        }
        let mut ready = Vec::new();
        let mut bytes = 0;
        // A cached capture can make every region ready at once. Bound the
        // initial response as well as its wait: providers may buffer structured
        // output, so a whole-scene first request can delay every visible result.
        // Keep logical units intact, including an individually oversized unit.
        let byte_limit = if self.dispatched { 12_000 } else { 1_024 };
        let member_limit = if self.dispatched { usize::MAX } else { 16 };
        self.pending.retain(|&i| {
            let candidate = &candidates[i];
            let unit_bytes = candidate.source_text.len();
            let members = usize::from(unit_bytes > 0);
            if (!ready.is_empty()
                && (bytes + unit_bytes > byte_limit || ready.len() + members > member_limit))
                || !self.completed.contains(&candidate.id)
            {
                return true;
            }
            if unit_bytes > 0 {
                ready.push(candidate.clone());
            }
            bytes += unit_bytes;
            false
        });
        self.dispatched |= !ready.is_empty();
        self.ready_since = None;
        ready
    }

    pub(super) fn observe_ready(
        &mut self,
        candidates: &[DetectedTextRegion],
        now: Instant,
    ) -> usize {
        let ready_count = self
            .pending
            .iter()
            .filter(|&&i| self.completed.contains(&candidates[i].id))
            .count();
        if ready_count == 0 {
            self.ready_since = None;
        } else {
            self.ready_since.get_or_insert(now);
        }
        ready_count
    }
    pub(super) fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn region(id: u16, left: u16, top: u16, text: &str) -> DetectedTextRegion {
        DetectedTextRegion {
            id,
            bounds: super::super::super::contract::NormalizedBounds {
                left,
                top,
                right: left + 100,
                bottom: top + 10,
            },
            source_text: text.into(),
            source_alternatives: vec![text.into()],
            recognition: Default::default(),
            appearance: None,
        }
    }
    #[test]
    fn small_capture_keeps_full_scene_context() {
        let candidates = [
            region(1, 0, 0, "one"),
            region(2, 0, 12, "two"),
            region(3, 300, 0, "other"),
        ];
        let mut groups = Groups::new(&candidates);
        groups.completed(1);
        groups.completed(3);
        assert!(groups.take_ready(&candidates, Instant::now()).is_empty());
        groups.completed(2);
        assert_eq!(groups.take_ready(&candidates, Instant::now()).len(), 3);
        assert!(groups.is_empty());
    }
    #[test]
    fn completed_dense_capture_keeps_first_response_small_then_flushes_tail() {
        let candidates = (0..200)
            .map(|id| region(id, 0, 0, "label"))
            .collect::<Vec<_>>();
        let mut groups = Groups::new(&candidates);
        for candidate in &candidates {
            groups.completed(candidate.id);
        }
        let first = groups.take_ready(&candidates, Instant::now());
        assert_eq!(first.len(), 16);
        let tail = groups.take_ready(&candidates, Instant::now());
        assert_eq!(tail.len(), 184);
        assert_eq!(
            first.iter().chain(&tail).map(|r| r.id).collect::<Vec<_>>(),
            (0..200).collect::<Vec<_>>()
        );
        assert!(groups.is_empty());
    }

    #[test]
    fn oversized_logical_unit_is_never_split_or_starved() {
        let candidates = vec![region(0, 0, 0, &"x".repeat(3200)), region(1, 0, 40, "next")];
        let mut groups = Groups::new(&candidates);
        for candidate in &candidates {
            groups.completed(candidate.id);
        }
        assert_eq!(
            groups.take_ready(&candidates, Instant::now())[0]
                .source_text
                .len(),
            3200
        );
        assert_eq!(groups.take_ready(&candidates, Instant::now()).len(), 1);
        assert!(groups.is_empty());
    }

    #[test]
    fn empty_regions_do_not_consume_the_initial_response_budget() {
        let candidates = (0..40)
            .map(|id| region(id, 0, 0, if id < 30 { "" } else { "text" }))
            .collect::<Vec<_>>();
        let mut groups = Groups::new(&candidates);
        for candidate in &candidates {
            groups.completed(candidate.id);
        }
        assert_eq!(groups.take_ready(&candidates, Instant::now()).len(), 10);
        assert!(groups.is_empty());
    }

    #[test]
    fn large_text_is_split_by_bytes_without_losing_members() {
        let candidates = (0..20)
            .map(|id| region(id, 0, 0, &"x".repeat(1000)))
            .collect::<Vec<_>>();
        let mut groups = Groups::new(&candidates);
        for candidate in &candidates {
            groups.completed(candidate.id);
        }
        let mut emitted = groups.take_ready(&candidates, Instant::now());
        assert_eq!(emitted.len(), 1);
        emitted.extend(groups.take_ready(&candidates, Instant::now()));
        assert_eq!(emitted.len(), 13);
        emitted.extend(groups.take_ready(&candidates, Instant::now()));
        assert_eq!(
            emitted.iter().map(|r| r.id).collect::<Vec<_>>(),
            (0..20).collect::<Vec<_>>()
        );
        assert!(groups.is_empty());
    }

    #[test]
    fn large_capture_batches_progressively_and_flushes_the_tail_once() {
        let candidates = (0..107)
            .map(|id| region(id, 0, 0, "text"))
            .collect::<Vec<_>>();
        let mut groups = Groups::new(&candidates);
        let now = Instant::now();
        for id in 0..7 {
            groups.completed(id);
        }
        assert!(groups.take_ready(&candidates, now).is_empty());
        groups.completed(7);
        assert!(groups.take_ready(&candidates, now).is_empty());
        assert_eq!(
            groups
                .take_ready(&candidates, now + Duration::from_millis(100))
                .len(),
            8
        );
        for id in 8..39 {
            groups.completed(id);
        }
        assert!(groups.take_ready(&candidates, Instant::now()).is_empty());
        groups.completed(39);
        assert!(
            groups
                .take_ready(&candidates, now + Duration::from_millis(500))
                .is_empty()
        );
        for id in 40..107 {
            groups.completed(id);
        }
        assert_eq!(
            groups
                .take_ready(&candidates, now + Duration::from_secs(3))
                .len(),
            99
        );
        assert!(groups.is_empty());
    }

    #[test]
    fn unresolved_members_are_accounted_for_without_being_translated() {
        let candidates = [region(1, 0, 0, ""), region(2, 0, 12, "readable")];
        let mut groups = Groups::new(&candidates);
        groups.completed(2);
        assert!(groups.take_ready(&candidates, Instant::now()).is_empty());
        groups.completed(1);
        assert_eq!(groups.take_ready(&candidates, Instant::now())[0].id, 2);
        assert!(groups.is_empty());
    }

    #[test]
    fn ready_time_accrues_while_another_translation_is_in_flight() {
        let candidates = (0..20)
            .map(|id| region(id, 0, 0, "text"))
            .collect::<Vec<_>>();
        let mut groups = Groups::new(&candidates);
        let now = Instant::now();
        for id in 0..8 {
            groups.completed(id);
        }
        assert!(groups.take_ready(&candidates, now).is_empty());
        assert_eq!(
            groups
                .take_ready(&candidates, now + Duration::from_millis(100))
                .len(),
            8
        );
        groups.completed(8);
        groups.observe_ready(&candidates, now);
        assert_eq!(
            groups
                .take_ready(&candidates, now + Duration::from_millis(600))
                .len(),
            1
        );
    }

    #[test]
    fn final_reader_wave_coalesces_without_extending_its_deadline() {
        let candidates = (0..12)
            .map(|id| region(id, 0, 0, "text"))
            .collect::<Vec<_>>();
        let mut groups = Groups::new(&candidates);
        let now = Instant::now();
        for id in 0..8 {
            groups.completed(id);
        }
        assert!(groups.take_ready(&candidates, now).is_empty());
        groups.completed(8);
        assert!(
            groups
                .take_ready(&candidates, now + Duration::from_millis(80))
                .is_empty()
        );
        assert_eq!(
            groups
                .take_ready(&candidates, now + Duration::from_millis(100))
                .len(),
            9
        );
        for id in 9..12 {
            groups.completed(id);
        }
        assert_eq!(
            groups
                .take_ready(&candidates, now + Duration::from_millis(101))
                .len(),
            3
        );
        assert!(groups.is_empty());
    }

    #[test]
    fn completed_tail_flushes_immediately_in_one_request() {
        let candidates = (0..12)
            .map(|id| region(id, 0, 0, "text"))
            .collect::<Vec<_>>();
        let mut groups = Groups::new(&candidates);
        let now = Instant::now();
        for id in 0..8 {
            groups.completed(id);
        }
        assert!(groups.take_ready(&candidates, now).is_empty());
        for id in 8..12 {
            groups.completed(id);
        }
        assert_eq!(
            groups
                .take_ready(&candidates, now + Duration::from_millis(45))
                .len(),
            12
        );
        assert!(groups.is_empty());
    }

    #[test]
    fn slow_unfinished_ocr_cannot_hold_ready_text_indefinitely() {
        let candidates = [region(1, 0, 0, "one"), region(2, 300, 0, "two")];
        let mut groups = Groups::new(&candidates);
        let now = Instant::now();
        groups.completed(1);
        assert!(groups.take_ready(&candidates, now).is_empty());
        assert!(
            groups
                .take_ready(&candidates, now + Duration::from_millis(599))
                .is_empty()
        );
        assert_eq!(
            groups.take_ready(&candidates, now + Duration::from_millis(600))[0].id,
            1
        );
        groups.completed(2);
        assert_eq!(
            groups.take_ready(&candidates, now + Duration::from_millis(601))[0].id,
            2
        );
        assert!(groups.is_empty());
    }
}
