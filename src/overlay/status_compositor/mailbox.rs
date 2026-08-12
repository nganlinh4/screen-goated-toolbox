use super::protocol::HostCommand;
use std::collections::VecDeque;

pub(super) const MAX_BUFFERED_COMMANDS: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PushResult {
    Queued,
    Overflowed,
    AwaitingSnapshot,
}

#[derive(Default)]
pub(super) struct CommandBuffer {
    commands: VecDeque<HostCommand>,
    awaiting_snapshot: bool,
}

impl CommandBuffer {
    pub(super) fn push(&mut self, command: HostCommand) -> PushResult {
        if matches!(command, HostCommand::Snapshot { .. }) {
            self.replace_with_snapshot(command);
            return PushResult::Queued;
        }
        if self.awaiting_snapshot {
            return PushResult::AwaitingSnapshot;
        }
        if self
            .commands
            .back()
            .is_some_and(|previous| can_replace(previous, &command))
        {
            *self.commands.back_mut().unwrap() = command;
            return PushResult::Queued;
        }
        if self.commands.len() >= MAX_BUFFERED_COMMANDS {
            self.commands.clear();
            self.awaiting_snapshot = true;
            return PushResult::Overflowed;
        }
        self.commands.push_back(command);
        PushResult::Queued
    }

    pub(super) fn replace_with_snapshot(&mut self, snapshot: HostCommand) {
        debug_assert!(matches!(snapshot, HostCommand::Snapshot { .. }));
        self.commands.clear();
        self.commands.push_back(snapshot);
        self.awaiting_snapshot = false;
    }

    pub(super) fn drain(&mut self) -> Vec<HostCommand> {
        self.commands.drain(..).collect()
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.commands.len()
    }
}

fn can_replace(previous: &HostCommand, next: &HostCommand) -> bool {
    matches!(
        (previous, next),
        (HostCommand::Theme { .. }, HostCommand::Theme { .. })
            | (
                HostCommand::RecordingUpdate { .. },
                HostCommand::RecordingUpdate { .. }
            )
            | (
                HostCommand::ProgressUpsert { .. },
                HostCommand::ProgressUpsert { .. }
            )
            | (
                HostCommand::SelectionUpdate { .. },
                HostCommand::SelectionUpdate { .. }
            )
            | (
                HostCommand::SelectionPosition { .. },
                HostCommand::SelectionPosition { .. }
            )
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::status_compositor::protocol::{
        NotificationScene, PhysicalRect, StatusSnapshot,
    };

    fn notification(id: u64) -> HostCommand {
        HostCommand::NotificationAdd {
            rect: PhysicalRect::default(),
            notification: NotificationScene {
                id,
                title: id.to_string(),
                snippet: String::new(),
                kind: "info".to_string(),
                duration_ms: None,
            },
        }
    }

    #[test]
    fn adjacent_high_frequency_state_keeps_only_the_latest_value() {
        let mut buffer = CommandBuffer::default();
        for rms in [0.1, 0.4, 0.9] {
            assert_eq!(
                buffer.push(HostCommand::RecordingUpdate {
                    state: "recording".to_string(),
                    rms,
                }),
                PushResult::Queued
            );
        }
        let commands = buffer.drain();
        assert_eq!(commands.len(), 1);
        assert!(matches!(commands[0], HostCommand::RecordingUpdate { rms, .. } if rms == 0.9));
    }

    #[test]
    fn stalled_consumer_has_a_hard_memory_bound_until_resynchronized() {
        let mut buffer = CommandBuffer::default();
        for id in 0..MAX_BUFFERED_COMMANDS as u64 {
            assert_eq!(buffer.push(notification(id)), PushResult::Queued);
        }
        assert_eq!(
            buffer.push(notification(MAX_BUFFERED_COMMANDS as u64)),
            PushResult::Overflowed
        );
        for id in 0..MAX_BUFFERED_COMMANDS as u64 * 4 {
            assert_eq!(buffer.push(notification(id)), PushResult::AwaitingSnapshot);
        }
        assert_eq!(buffer.len(), 0);

        buffer.replace_with_snapshot(HostCommand::Snapshot {
            scene: StatusSnapshot::default(),
        });
        assert_eq!(buffer.len(), 1);
        assert_eq!(buffer.push(notification(999)), PushResult::Queued);
        assert_eq!(buffer.len(), 2);
    }

    #[test]
    fn state_is_not_coalesced_across_a_visibility_boundary() {
        let mut buffer = CommandBuffer::default();
        buffer.push(HostCommand::SelectionPosition {
            rect: PhysicalRect {
                x: 10,
                ..Default::default()
            },
        });
        buffer.push(HostCommand::SelectionShow {
            rect: PhysicalRect {
                x: 20,
                ..Default::default()
            },
            text: "selected".to_string(),
        });
        buffer.push(HostCommand::SelectionPosition {
            rect: PhysicalRect {
                x: 30,
                ..Default::default()
            },
        });
        assert_eq!(buffer.drain().len(), 3);
    }
}
