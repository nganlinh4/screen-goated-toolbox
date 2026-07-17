//! Structural lease for raw keyboard input after a window-selection attempt.

#[derive(Debug, Default)]
pub(super) struct KeyboardTargetGate {
    refocus_required: bool,
}

impl KeyboardTargetGate {
    pub(super) fn begin_focus_attempt(&mut self) {
        self.refocus_required = true;
    }

    pub(super) fn record_focus_result(&mut self, focused: bool) {
        self.refocus_required = !focused;
    }

    pub(super) fn reset(&mut self) {
        self.refocus_required = false;
    }

    pub(super) fn refocus_required(&self) -> bool {
        self.refocus_required
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_focus_latches_until_success_or_new_turn() {
        let mut gate = KeyboardTargetGate::default();
        assert!(!gate.refocus_required());
        gate.begin_focus_attempt();
        assert!(gate.refocus_required());
        gate.record_focus_result(false);
        assert!(gate.refocus_required());
        gate.record_focus_result(true);
        assert!(!gate.refocus_required());
        gate.begin_focus_attempt();
        gate.reset();
        assert!(!gate.refocus_required());
    }
}
