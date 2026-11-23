//! Bounded FIFO for asynchronous WebView script application.
use std::collections::VecDeque;

const MAX_SCRIPTS: usize = 512;
const MAX_BYTES: usize = 8 * 1024 * 1024;

#[derive(Default)]
pub(super) struct ScriptQueue {
    pending: VecDeque<Script>,
    bytes: usize,
    in_flight: bool,
}

pub(super) struct Script {
    pub(super) source: String,
    pub(super) session_id: Option<u64>,
}

impl Script {
    pub(super) fn admitted(&self, current_session: u64) -> bool {
        self.session_id
            .is_none_or(|session| session != 0 && session == current_session)
    }
}

impl ScriptQueue {
    pub(super) fn push(&mut self, source: String, session_id: Option<u64>) -> bool {
        let Some(bytes) = self.bytes.checked_add(source.len()) else {
            return false;
        };
        if self.pending.len() >= MAX_SCRIPTS || bytes > MAX_BYTES {
            return false;
        }
        self.bytes = bytes;
        self.pending.push_back(Script { source, session_id });
        true
    }

    pub(super) fn next(&mut self) -> Option<Script> {
        if self.in_flight {
            return None;
        }
        let script = self.pending.pop_front()?;
        self.bytes -= script.source.len();
        self.in_flight = true;
        Some(script)
    }

    pub(super) fn complete(&mut self) -> bool {
        if !self.in_flight {
            return false;
        }
        self.in_flight = false;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_wait_for_completion_and_preserve_order() {
        let mut queue = ScriptQueue::default();
        assert!(queue.push("old".into(), Some(1)));
        assert!(queue.push("new".into(), Some(2)));
        assert_eq!(queue.next().unwrap().source, "old");
        assert!(queue.next().is_none());
        assert!(queue.complete());
        assert_eq!(queue.next().unwrap().source, "new");
        assert!(queue.complete());
        assert!(!queue.complete());
    }

    #[test]
    fn old_focus_script_is_skipped_without_losing_newer_scripts() {
        let mut queue = ScriptQueue::default();
        assert!(queue.push("old focus".into(), Some(1)));
        assert!(queue.push("new scene".into(), None));
        assert!(!queue.next().unwrap().admitted(2));
        assert!(queue.complete());
        assert!(queue.next().unwrap().admitted(2));
    }

    #[test]
    fn queue_rejects_excess_count_and_bytes_without_discarding_admitted_work() {
        let mut queue = ScriptQueue::default();
        assert!(queue.push("x".repeat(MAX_BYTES), None));
        assert!(!queue.push("y".into(), None));
        assert_eq!(queue.next().unwrap().source.len(), MAX_BYTES);
        assert!(queue.complete());
        for _ in 0..MAX_SCRIPTS {
            assert!(queue.push("x".into(), None));
        }
        assert!(!queue.push("y".into(), None));
        assert_eq!(queue.next().unwrap().source, "x");
    }
}
