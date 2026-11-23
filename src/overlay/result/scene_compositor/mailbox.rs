use super::protocol::{HostCommand, SceneControlUpdate, SceneGeometry};
use std::collections::VecDeque;

pub(super) const MAX_BUFFERED_COMMANDS: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PushResult {
    Queued,
    Overflowed,
    AwaitingSnapshot,
}

/// Operations a scene snapshot cannot recreate: dictated refinement text and
/// source-image requests exist only as commands, so recovery replays them
/// after the snapshot instead of discarding them with the rest of the queue.
pub(super) fn survives_snapshot(command: &HostCommand) -> bool {
    matches!(
        command,
        HostCommand::RefineText {
            is_insert: true,
            ..
        } | HostCommand::SourceImage { .. }
            | HostCommand::SourceImageStatus { .. }
    )
}

#[derive(Default)]
pub(super) struct CommandBuffer {
    commands: VecDeque<HostCommand>,
    awaiting_snapshot: bool,
}

impl CommandBuffer {
    pub(super) fn push(&mut self, command: HostCommand) -> PushResult {
        if matches!(command, HostCommand::Snapshot { .. }) {
            self.restart_from(command);
            return PushResult::Queued;
        }
        if matches!(command, HostCommand::Shutdown) {
            self.commands.clear();
            self.commands.push_back(command);
            self.awaiting_snapshot = false;
            return PushResult::Queued;
        }
        if self.awaiting_snapshot {
            if survives_snapshot(&command) {
                self.retain_operation(command);
            }
            return PushResult::AwaitingSnapshot;
        }
        if self.replace_pending_stream(&command) || self.coalesce_adjacent(&command) {
            return PushResult::Queued;
        }
        if let HostCommand::Finalize { card } = &command {
            self.remove_pending_stream(card.id);
        }
        if self.commands.len() >= MAX_BUFFERED_COMMANDS {
            self.commands.retain(survives_snapshot);
            self.awaiting_snapshot = true;
            if survives_snapshot(&command) {
                self.retain_operation(command);
            }
            return PushResult::Overflowed;
        }
        self.commands.push_back(command);
        PushResult::Queued
    }

    pub(super) fn replace_with_snapshot(&mut self, cards: Vec<super::protocol::SceneCard>) {
        self.restart_from(HostCommand::Snapshot { cards });
    }

    fn restart_from(&mut self, snapshot: HostCommand) {
        self.commands.retain(survives_snapshot);
        self.commands.push_front(snapshot);
        self.awaiting_snapshot = false;
    }

    /// Bounded like the live queue; an operation beyond the bound is dropped
    /// (source-image requests then fail through their own timeout).
    fn retain_operation(&mut self, command: HostCommand) {
        if !self.coalesce_adjacent(&command) && self.commands.len() < MAX_BUFFERED_COMMANDS {
            self.commands.push_back(command);
        }
    }

    pub(super) fn drain(&mut self) -> Vec<HostCommand> {
        self.commands.drain(..).collect()
    }

    fn replace_pending_stream(&mut self, command: &HostCommand) -> bool {
        let HostCommand::Stream { card } = command else {
            return false;
        };
        for index in (0..self.commands.len()).rev() {
            match &self.commands[index] {
                HostCommand::Stream { card: previous } if previous.id == card.id => {
                    self.commands[index] = command.clone();
                    return true;
                }
                HostCommand::Upsert { card: previous } if previous.id == card.id => break,
                HostCommand::UpsertBatch { cards }
                    if cards.iter().any(|previous| previous.id == card.id) =>
                {
                    break;
                }
                HostCommand::Finalize { card: previous } if previous.id == card.id => break,
                HostCommand::Remove { id } if *id == card.id => break,
                HostCommand::Snapshot { .. } => break,
                _ => {}
            }
        }
        false
    }

    fn remove_pending_stream(&mut self, id: isize) {
        for index in (0..self.commands.len()).rev() {
            match &self.commands[index] {
                HostCommand::Stream { card } if card.id == id => {
                    self.commands.remove(index);
                    return;
                }
                HostCommand::Upsert { card } if card.id == id => return,
                HostCommand::UpsertBatch { cards } if cards.iter().any(|card| card.id == id) => {
                    return;
                }
                HostCommand::Finalize { card } if card.id == id => return,
                HostCommand::Remove { id: removed } if *removed == id => return,
                HostCommand::Snapshot { .. } => return,
                _ => {}
            }
        }
    }

    fn coalesce_adjacent(&mut self, command: &HostCommand) -> bool {
        let Some(previous) = self.commands.back_mut() else {
            return false;
        };
        if let (
            HostCommand::RefineText {
                id: previous_id,
                text: previous_text,
                is_insert: true,
                edit_session: previous_session,
            },
            HostCommand::RefineText {
                id: next_id,
                text: next_text,
                is_insert: true,
                edit_session: next_session,
            },
        ) = (&mut *previous, command)
            && previous_id == next_id
            && previous_session == next_session
        {
            previous_text.push_str(next_text);
            return true;
        }
        if can_replace_adjacent(previous, command) {
            previous.clone_from(command);
            return true;
        }
        match (previous, command) {
            (HostCommand::Geometry { cards: previous }, HostCommand::Geometry { cards: next }) => {
                merge_geometry(previous, next)
            }
            (HostCommand::Controls { cards: previous }, HostCommand::Controls { cards: next }) => {
                merge_controls(previous, next)
            }
            _ => return false,
        }
        true
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.commands.len()
    }
}

fn can_replace_adjacent(previous: &HostCommand, next: &HostCommand) -> bool {
    match (previous, next) {
        (HostCommand::Processing { .. }, HostCommand::Processing { .. })
        | (HostCommand::Theme { .. }, HostCommand::Theme { .. })
        | (HostCommand::Locale { .. }, HostCommand::Locale { .. })
        | (HostCommand::ExternalDrag { .. }, HostCommand::ExternalDrag { .. }) => true,
        (HostCommand::Opacity { id: previous, .. }, HostCommand::Opacity { id: next, .. })
        | (HostCommand::Raise { id: previous, .. }, HostCommand::Raise { id: next, .. }) => {
            previous == next
        }
        (
            HostCommand::RefineText {
                id: previous,
                is_insert: false,
                edit_session: previous_session,
                ..
            },
            HostCommand::RefineText {
                id: next,
                is_insert: false,
                edit_session: next_session,
                ..
            },
        ) => previous == next && previous_session == next_session,
        (HostCommand::Upsert { card: previous }, HostCommand::Upsert { card: next }) => {
            previous.id == next.id
        }
        (HostCommand::Stream { card: previous }, HostCommand::Finalize { card: next }) => {
            previous.id == next.id
        }
        _ => false,
    }
}

fn merge_geometry(previous: &mut Vec<SceneGeometry>, next: &[SceneGeometry]) {
    for update in next {
        if let Some(existing) = previous.iter_mut().find(|item| item.id == update.id) {
            existing.clone_from(update);
        } else {
            previous.push(update.clone());
        }
    }
}

fn merge_controls(previous: &mut Vec<SceneControlUpdate>, next: &[SceneControlUpdate]) {
    for update in next {
        if let Some(existing) = previous.iter_mut().find(|item| item.id == update.id) {
            existing.clone_from(update);
        } else {
            previous.push(update.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::result::scene_compositor::protocol::{
        SceneControls, SceneFinalize, SceneStream,
    };

    fn stream(id: isize, body: &str) -> HostCommand {
        HostCommand::Stream {
            card: SceneStream {
                id,
                body: body.to_string(),
                document: None,
                refining: false,
                navigation_loading: false,
                background: "#ffffff".to_string(),
                opacity: 100,
                visible: true,
                streaming_enabled: true,
                controls: SceneControls::default(),
            },
        }
    }

    fn finalize(id: isize, body: &str) -> HostCommand {
        HostCommand::Finalize {
            card: SceneFinalize {
                id,
                body: body.to_string(),
                document: None,
                refining: false,
                navigation_loading: false,
                background: "#ffffff".to_string(),
                opacity: 100,
                visible: true,
                streaming_enabled: false,
                controls: SceneControls::default(),
            },
        }
    }

    #[test]
    fn keeps_latest_stream_per_card_without_reordering_other_cards() {
        let mut buffer = CommandBuffer::default();
        buffer.push(stream(1, "old"));
        buffer.push(stream(2, "other"));
        buffer.push(stream(1, "latest"));
        let commands = buffer.drain();

        assert_eq!(commands.len(), 2);
        assert!(matches!(&commands[0], HostCommand::Stream { card } if card.body == "latest"));
        assert!(matches!(&commands[1], HostCommand::Stream { card } if card.body == "other"));
    }

    #[test]
    fn finalize_supersedes_pending_stream_for_the_same_card() {
        let mut buffer = CommandBuffer::default();
        buffer.push(stream(1, "partial"));
        buffer.push(stream(2, "other"));
        buffer.push(finalize(1, "done"));
        let commands = buffer.drain();

        assert_eq!(commands.len(), 2);
        assert!(matches!(&commands[1], HostCommand::Finalize { card } if card.body == "done"));
    }

    fn opacity(id: isize) -> HostCommand {
        HostCommand::Opacity { id, opacity: 50 }
    }

    fn dictated(text: &str) -> HostCommand {
        HostCommand::RefineText {
            id: 7,
            text: text.to_string(),
            is_insert: true,
            edit_session: Some("1".to_string()),
        }
    }

    fn fill_to_bound(buffer: &mut CommandBuffer) {
        let mut id = 0;
        while buffer.len() < MAX_BUFFERED_COMMANDS {
            assert_eq!(buffer.push(opacity(id)), PushResult::Queued);
            id += 1;
        }
    }

    #[test]
    fn stalled_consumer_has_a_hard_command_bound_until_snapshot() {
        let mut buffer = CommandBuffer::default();
        fill_to_bound(&mut buffer);
        assert_eq!(
            buffer.push(opacity(MAX_BUFFERED_COMMANDS as isize)),
            PushResult::Overflowed
        );
        for id in 0..MAX_BUFFERED_COMMANDS as isize * 4 {
            assert_eq!(buffer.push(opacity(id)), PushResult::AwaitingSnapshot);
        }
        assert_eq!(buffer.len(), 0);

        buffer.replace_with_snapshot(Vec::new());
        assert_eq!(buffer.len(), 1);
        assert_eq!(buffer.push(opacity(999)), PushResult::Queued);
    }

    #[test]
    fn overflow_recovery_replays_operations_the_snapshot_cannot_recreate() {
        let mut buffer = CommandBuffer::default();
        buffer.push(dictated("before "));
        fill_to_bound(&mut buffer);
        assert_eq!(buffer.push(dictated("lost?")), PushResult::Overflowed);
        let status = HostCommand::SourceImageStatus {
            id: 3,
            success: true,
        };
        assert_eq!(buffer.push(status.clone()), PushResult::AwaitingSnapshot);
        assert_eq!(buffer.push(opacity(1)), PushResult::AwaitingSnapshot);
        assert_eq!(
            buffer.push(dictated(" after")),
            PushResult::AwaitingSnapshot
        );

        buffer.replace_with_snapshot(Vec::new());
        assert_eq!(
            buffer.drain(),
            vec![
                HostCommand::Snapshot { cards: Vec::new() },
                dictated("before lost?"),
                status,
                dictated(" after"),
            ]
        );
    }

    #[test]
    fn a_queued_snapshot_keeps_pending_dictation_after_it() {
        let mut buffer = CommandBuffer::default();
        buffer.push(opacity(1));
        buffer.push(dictated("spoken"));
        buffer.push(HostCommand::Snapshot { cards: Vec::new() });
        assert_eq!(
            buffer.drain(),
            vec![
                HostCommand::Snapshot { cards: Vec::new() },
                dictated("spoken")
            ]
        );
    }

    #[test]
    fn retained_operations_stay_bounded_while_awaiting_a_snapshot() {
        let mut buffer = CommandBuffer::default();
        fill_to_bound(&mut buffer);
        buffer.push(opacity(MAX_BUFFERED_COMMANDS as isize));
        for id in 0..MAX_BUFFERED_COMMANDS as isize * 4 {
            buffer.push(HostCommand::SourceImageStatus { id, success: true });
        }
        assert_eq!(buffer.len(), MAX_BUFFERED_COMMANDS);
    }

    #[test]
    fn only_operations_missing_from_the_snapshot_survive_it() {
        assert!(survives_snapshot(&dictated("dictated")));
        assert!(survives_snapshot(&HostCommand::SourceImageStatus {
            id: 42,
            success: true,
        }));
        assert!(!survives_snapshot(&HostCommand::RefineText {
            id: 42,
            text: "canonical".to_string(),
            is_insert: false,
            edit_session: Some("1".to_string()),
        }));
        assert!(!survives_snapshot(&opacity(42)));
    }

    #[test]
    fn adjacent_geometry_updates_merge_by_card() {
        let mut buffer = CommandBuffer::default();
        let geometry = |id, x| SceneGeometry {
            id,
            rect: super::super::protocol::SceneRect {
                x,
                ..Default::default()
            },
            control_rect: Default::default(),
            visible: true,
        };
        buffer.push(HostCommand::Geometry {
            cards: vec![geometry(1, 1), geometry(2, 2)],
        });
        buffer.push(HostCommand::Geometry {
            cards: vec![geometry(1, 3)],
        });

        let commands = buffer.drain();
        assert!(
            matches!(&commands[0], HostCommand::Geometry { cards } if cards.len() == 2 && cards[0].rect.x == 3)
        );
    }

    #[test]
    fn sequential_text_insertions_are_combined_without_losing_content() {
        let mut buffer = CommandBuffer::default();
        for text in ["hello", " ", "world"] {
            buffer.push(HostCommand::RefineText {
                id: 42,
                text: text.to_string(),
                is_insert: true,
                edit_session: Some("1".to_string()),
            });
        }

        let commands = buffer.drain();
        assert!(
            matches!(&commands[0], HostCommand::RefineText { text, is_insert: true, .. } if text == "hello world")
        );
    }

    #[test]
    fn insertions_from_different_edit_sessions_are_never_combined() {
        let mut buffer = CommandBuffer::default();
        for session in ["1", "2"] {
            buffer.push(HostCommand::RefineText {
                id: 42,
                text: session.to_string(),
                is_insert: true,
                edit_session: Some(session.to_string()),
            });
        }
        assert_eq!(buffer.drain().len(), 2);
    }

    #[test]
    fn adjacent_locale_changes_keep_only_the_latest_complete_strings() {
        let mut buffer = CommandBuffer::default();
        for language in ["en", "vi", "ko"] {
            assert_eq!(
                buffer.push(super::super::localization::command_for(language)),
                PushResult::Queued
            );
        }
        assert_eq!(
            buffer.drain(),
            vec![super::super::localization::command_for("ko")]
        );
    }

    #[test]
    fn recovery_snapshot_admits_its_current_locale_after_queue_overflow() {
        let mut buffer = CommandBuffer::default();
        fill_to_bound(&mut buffer);
        assert_eq!(
            buffer.push(super::super::localization::command_for("en")),
            PushResult::Overflowed
        );
        assert_eq!(
            buffer.push(super::super::localization::command_for("vi")),
            PushResult::AwaitingSnapshot
        );
        buffer.replace_with_snapshot(Vec::new());
        assert_eq!(
            buffer.push(super::super::localization::command_for("ko")),
            PushResult::Queued
        );
        assert_eq!(
            buffer.drain(),
            vec![
                HostCommand::Snapshot { cards: Vec::new() },
                super::super::localization::command_for("ko"),
            ]
        );
    }
}
