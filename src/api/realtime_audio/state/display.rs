//! The transcript display: text frozen from earlier sessions followed by the
//! live transcript. Recognizer events change only the tail of the live
//! transcript, so the display is updated in place from the first changed byte
//! instead of being rebuilt from the whole session on every event.

use super::RealtimeState;

impl RealtimeState {
    /// Rebuilds the display from the frozen prefix and the live transcript.
    pub(super) fn update_display_transcript(&mut self) {
        self.set_display_transcript(" ");
    }

    /// Rebuilds the display with `separator` between the frozen prefix and the
    /// live transcript, reusing the display's buffer.
    pub fn set_display_transcript(&mut self, separator: &str) {
        let display = &mut self.display_transcript;
        display.clear();
        self.display_full_start = if self.frozen_prefix.is_empty() {
            display.push_str(&self.full_transcript);
            Some(0)
        } else if self.full_transcript.is_empty() {
            display.push_str(&self.frozen_prefix);
            None
        } else {
            display.push_str(&self.frozen_prefix);
            display.push_str(separator);
            let start = display.len();
            display.push_str(&self.full_transcript);
            Some(start)
        };
    }

    /// Brings the display up to date after the live transcript, which was
    /// `previous_len` bytes long, changed after its first `unchanged` bytes.
    /// Only the changed tail is copied while the display still holds the
    /// previous live transcript; any other layout is rebuilt.
    pub(super) fn refresh_display_transcript(&mut self, unchanged: usize, previous_len: usize) {
        let Some(start) = self.display_full_start.filter(|&start| {
            let holds_previous = self.display_transcript.len() == start + previous_len;
            let keeps_layout = start == 0 || !self.full_transcript.is_empty();
            holds_previous
                && keeps_layout
                && unchanged <= previous_len
                && self.full_transcript.is_char_boundary(unchanged)
                && self.display_transcript.is_char_boundary(start + unchanged)
        }) else {
            self.update_display_transcript();
            return;
        };
        self.display_transcript.truncate(start + unchanged);
        self.display_transcript
            .push_str(&self.full_transcript[unchanged..]);
    }
}

#[cfg(test)]
mod tests {
    use super::RealtimeState;

    fn expected_display(state: &RealtimeState) -> String {
        match (
            state.frozen_prefix.is_empty(),
            state.full_transcript.is_empty(),
        ) {
            (true, _) => state.full_transcript.clone(),
            (false, true) => state.frozen_prefix.clone(),
            (false, false) => format!("{} {}", state.frozen_prefix, state.full_transcript),
        }
    }

    #[test]
    fn a_growing_transcript_updates_the_display_in_place() {
        let mut state = RealtimeState::new();
        state.append_transcript("Earlier session text.");
        state.freeze_current_transcript();
        state.display_transcript.reserve(64 * 1024);
        let buffer = state.display_transcript.as_ptr();
        for index in 0..2_000 {
            state.append_transcript(&format!(" word{index}"));
            assert_eq!(state.display_transcript, expected_display(&state));
        }
        assert_eq!(
            state.display_transcript.as_ptr(),
            buffer,
            "appends extend the display instead of rebuilding it"
        );
    }

    #[test]
    fn recognizer_corrections_rewrite_only_the_changed_tail() {
        let mut state = RealtimeState::new();
        state.append_transcript("Frozen words.");
        state.freeze_current_transcript();
        state.display_transcript.reserve(64 * 1024);
        let buffer = state.display_transcript.as_ptr();
        let mut committed = String::new();
        for index in 0..500 {
            state.set_transcript_segments(&committed, &format!("draft {index} ăn"));
            assert_eq!(state.display_transcript, expected_display(&state));
            state.set_transcript_segments(&committed, &format!("draft {index} ăn corrected"));
            assert_eq!(state.display_transcript, expected_display(&state));
            committed.push_str(&format!("sentence {index}. "));
        }
        assert_eq!(state.display_transcript.as_ptr(), buffer);
    }

    #[test]
    fn a_transcript_replaced_outside_the_state_methods_is_rebuilt() {
        let mut state = RealtimeState::new();
        state.append_transcript("first");
        state.freeze_current_transcript();
        state.append_transcript("second");
        state.full_transcript = "replaced elsewhere".to_string();
        state.append_transcript(" and appended");
        assert_eq!(
            state.display_transcript,
            "first replaced elsewhere and appended"
        );

        state.set_display_transcript("\n\n");
        state.append_transcript(" again");
        assert_eq!(
            state.display_transcript,
            "first\n\nreplaced elsewhere and appended again"
        );

        state.set_transcript_segments("", "");
        assert_eq!(state.display_transcript, "first");
        state.append_transcript("next");
        assert_eq!(state.display_transcript, "first next");
    }
}
