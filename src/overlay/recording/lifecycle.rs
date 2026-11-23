#[derive(Default)]
pub(super) struct VisualOwner {
    session: Option<usize>,
    timer: Option<usize>,
    shown: bool,
}

impl VisualOwner {
    pub(super) fn begin(&mut self, session: usize) -> Option<usize> {
        let previous = self.timer.take();
        self.session = Some(session);
        self.shown = false;
        previous
    }

    pub(super) fn schedule(
        &mut self,
        session: usize,
        timer: usize,
        hidden: bool,
    ) -> Option<Option<usize>> {
        if self.session != Some(session) || self.shown || hidden {
            return None;
        }
        Some(self.timer.replace(timer))
    }

    pub(super) fn take_timer(&mut self, timer: usize) -> Option<usize> {
        if self.timer != Some(timer) || self.shown {
            return None;
        }
        self.timer = None;
        self.shown = true;
        self.session
    }

    pub(super) fn retire(&mut self, session: usize) -> Option<Option<usize>> {
        if self.session != Some(session) {
            return None;
        }
        self.session = None;
        self.shown = false;
        Some(self.timer.take())
    }

    pub(super) fn current(&self) -> Option<usize> {
        self.session
    }
}

pub(super) fn manual_close_allowed(stopped: bool, aborted: bool, elapsed_ms: u64) -> bool {
    stopped || aborted || elapsed_ms >= 2_000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_close_retires_pending_show_immediately() {
        let mut owner = VisualOwner::default();
        owner.begin(1);
        assert_eq!(owner.schedule(1, 100, false), Some(None));
        assert_eq!(owner.retire(1), Some(Some(100)));
        assert_eq!(owner.take_timer(100), None);
        assert_eq!(owner.retire(1), None);
    }

    #[test]
    fn stale_close_ready_and_timer_do_not_affect_replacement() {
        let mut owner = VisualOwner::default();
        owner.begin(1);
        owner.schedule(1, 100, false);
        assert_eq!(owner.begin(2), Some(100));
        assert_eq!(owner.schedule(2, 101, false), Some(None));
        assert_eq!(owner.retire(1), None);
        assert_eq!(owner.schedule(1, 102, false), None);
        assert_eq!(owner.take_timer(100), None);
        assert_eq!(owner.take_timer(101), Some(2));
        assert_eq!(owner.take_timer(101), None);
        assert_eq!(owner.schedule(2, 103, false), None);
    }

    #[test]
    fn accelerated_show_retires_the_older_timer() {
        let mut owner = VisualOwner::default();
        owner.begin(4);
        owner.schedule(4, 100, false);
        assert_eq!(owner.schedule(4, 101, false), Some(Some(100)));
        assert_eq!(owner.take_timer(100), None);
        assert_eq!(owner.take_timer(101), Some(4));
    }

    #[test]
    fn manual_close_retains_the_existing_debounce() {
        assert!(!manual_close_allowed(false, false, 1_999));
        assert!(manual_close_allowed(false, false, 2_000));
        assert!(manual_close_allowed(true, false, 0));
        assert!(manual_close_allowed(false, true, 0));
    }

    #[test]
    fn hidden_session_never_schedules_a_show() {
        let mut owner = VisualOwner::default();
        owner.begin(3);
        assert_eq!(owner.schedule(3, 100, true), None);
        assert_eq!(owner.take_timer(100), None);
    }

    #[test]
    fn compositor_ready_carries_the_exact_session_string() {
        use crate::overlay::status_compositor::protocol::ChildEvent;

        let event: ChildEvent =
            serde_json::from_str(r#"{"type":"recording_ready","session_id":"9007199254740993"}"#)
                .unwrap();
        assert_eq!(
            event,
            ChildEvent::RecordingReady {
                session_id: "9007199254740993".into(),
            }
        );
    }
}
