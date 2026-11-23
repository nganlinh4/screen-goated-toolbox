//! Preserve editor-owned indices across structural preset-list changes.
use crate::gui::settings_ui::{PresetListChange, owner_after_removal, selection_after_removal};

#[derive(Default)]
pub(super) struct PresetUiIndices {
    pub current: Option<usize>,
    pub recording: Option<usize>,
    pub graph: Option<usize>,
}

#[derive(Default)]
pub(super) struct ChangeEffects {
    pub clear_recording: bool,
    pub discard_graph: bool,
}

impl PresetUiIndices {
    pub fn apply(&mut self, change: PresetListChange) -> ChangeEffects {
        match change {
            PresetListChange::Moved(index_move) => {
                self.current = self.current.map(|index| index_move.remap(index));
                self.recording = self.recording.map(|index| index_move.remap(index));
                self.graph = self.graph.map(|index| index_move.remap(index));
                ChangeEffects::default()
            }
            PresetListChange::Removed { index, remaining } => {
                let had_recording = self.recording.is_some();
                let had_graph = self.graph.is_some();
                self.current = selection_after_removal(self.current, index, remaining);
                self.recording = owner_after_removal(self.recording, index, remaining);
                self.graph = owner_after_removal(self.graph, index, remaining);
                ChangeEffects {
                    clear_recording: had_recording && self.recording.is_none(),
                    discard_graph: had_graph && self.graph.is_none(),
                }
            }
            PresetListChange::ActiveProfileChanged => {
                self.current = None;
                self.recording = None;
                self.graph = None;
                ChangeEffects {
                    clear_recording: true,
                    discard_graph: true,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::settings_ui::PresetListChange;

    #[test]
    fn deleted_row_cannot_transfer_graph_or_hotkey_ownership_to_an_equal_id() {
        let mut state = PresetUiIndices {
            current: Some(2),
            recording: Some(2),
            graph: Some(2),
        };
        let effects = state.apply(PresetListChange::Removed {
            index: 2,
            remaining: 4,
        });
        assert_eq!(state.current, Some(1));
        assert_eq!(state.recording, None);
        assert_eq!(state.graph, None);
        assert!(effects.clear_recording && effects.discard_graph);
    }

    #[test]
    fn removing_another_row_remaps_owners_without_discarding_them() {
        let mut state = PresetUiIndices {
            current: Some(3),
            recording: Some(3),
            graph: Some(3),
        };
        let effects = state.apply(PresetListChange::Removed {
            index: 1,
            remaining: 4,
        });
        assert_eq!(
            (state.current, state.recording, state.graph),
            (Some(2), Some(2), Some(2))
        );
        assert!(!effects.clear_recording && !effects.discard_graph);
    }

    #[test]
    fn replacing_profile_cancels_capture_and_discards_graph() {
        let mut state = PresetUiIndices {
            current: Some(1),
            recording: Some(1),
            graph: Some(1),
        };
        let effects = state.apply(PresetListChange::ActiveProfileChanged);
        assert_eq!(
            (state.current, state.recording, state.graph),
            (None, None, None)
        );
        assert!(effects.clear_recording && effects.discard_graph);
    }
}
