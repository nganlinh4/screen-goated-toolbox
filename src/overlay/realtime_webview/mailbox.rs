use std::collections::VecDeque;

use super::protocol::HostCommand;

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
        if matches!(
            command,
            HostCommand::Snapshot { .. } | HostCommand::Shutdown
        ) {
            self.commands.clear();
            self.commands.push_back(command);
            self.awaiting_snapshot = false;
            return PushResult::Queued;
        }
        if self.awaiting_snapshot {
            return PushResult::AwaitingSnapshot;
        }
        if let Some(index) = self
            .commands
            .iter()
            .rposition(|pending| same_replaceable_lane(pending, &command))
        {
            self.commands[index] = command;
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

    pub(super) fn replace_with_snapshot(&mut self, command: HostCommand) {
        debug_assert!(matches!(command, HostCommand::Snapshot { .. }));
        self.commands.clear();
        self.commands.push_back(command);
        self.awaiting_snapshot = false;
    }

    pub(super) fn drain(&mut self) -> Vec<HostCommand> {
        self.commands.drain(..).collect()
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.commands.len()
    }
}

fn same_replaceable_lane(previous: &HostCommand, next: &HostCommand) -> bool {
    match (previous, next) {
        (HostCommand::Layout { .. }, HostCommand::Layout { .. })
        | (HostCommand::Settings { .. }, HostCommand::Settings { .. })
        | (HostCommand::Tts { .. }, HostCommand::Tts { .. })
        | (HostCommand::Volume { .. }, HostCommand::Volume { .. })
        | (HostCommand::TranslationModel { .. }, HostCommand::TranslationModel { .. })
        | (HostCommand::Download { .. }, HostCommand::Download { .. })
        | (HostCommand::Theme { .. }, HostCommand::Theme { .. }) => true,
        (HostCommand::Text { role: previous, .. }, HostCommand::Text { role: next, .. }) => {
            previous == next
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::realtime_webview::layout::CardRole;
    use crate::overlay::realtime_webview::protocol::{CardText, RealtimeScene};

    fn text(role: CardRole, value: &str) -> HostCommand {
        HostCommand::Text {
            role,
            text: CardText {
                committed: value.to_string(),
                draft: String::new(),
            },
        }
    }

    #[test]
    fn high_rate_updates_keep_only_the_latest_value_per_lane() {
        let mut buffer = CommandBuffer::default();
        for value in 0..1_000 {
            assert_eq!(
                buffer.push(text(CardRole::Transcription, &value.to_string())),
                PushResult::Queued
            );
            assert_eq!(
                buffer.push(HostCommand::Volume { rms: value as f32 }),
                PushResult::Queued
            );
        }
        assert_eq!(buffer.len(), 2);
        let commands = buffer.drain();
        assert!(matches!(&commands[0], HostCommand::Text { text, .. } if text.committed == "999"));
        assert!(matches!(&commands[1], HostCommand::Volume { rms } if *rms == 999.0));
    }

    #[test]
    fn overflow_requires_an_authoritative_snapshot() {
        let mut buffer = CommandBuffer::default();
        for index in 0..MAX_BUFFERED_COMMANDS {
            assert_eq!(
                buffer.push(HostCommand::Script {
                    role: None,
                    script: index.to_string(),
                }),
                PushResult::Queued
            );
        }
        assert_eq!(
            buffer.push(HostCommand::Script {
                role: None,
                script: "overflow".to_string(),
            }),
            PushResult::Overflowed
        );
        assert_eq!(buffer.len(), 0);
        buffer.replace_with_snapshot(HostCommand::Snapshot {
            scene: Box::new(RealtimeScene::default()),
        });
        assert_eq!(buffer.len(), 1);
    }
}
