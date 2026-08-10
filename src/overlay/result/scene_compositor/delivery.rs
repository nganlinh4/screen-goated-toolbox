use super::parent::{ensure_process, restart_process_now, write_command};
use super::protocol::HostCommand;
use std::collections::HashMap;
use std::sync::LazyLock;
use std::sync::mpsc::{Receiver, Sender, channel};

static SENDER: LazyLock<Sender<Message>> = LazyLock::new(|| {
    let (sender, receiver) = channel();
    std::thread::Builder::new()
        .name("sgt-result-delivery".to_string())
        .spawn(move || delivery_loop(receiver))
        .expect("failed to start result delivery thread");
    sender
});

enum Message {
    Command(HostCommand),
    Restart,
    Warmup,
}

pub(super) fn warmup() {
    let _ = SENDER.send(Message::Warmup);
}

pub(super) fn send_command(command: HostCommand) {
    let _ = SENDER.send(Message::Command(command));
}

pub(super) fn request_restart() {
    let _ = SENDER.send(Message::Restart);
}

fn delivery_loop(receiver: Receiver<Message>) {
    while let Ok(message) = receiver.recv() {
        let mut commands = Vec::new();
        let mut restart = false;
        let mut warmup = false;
        collect(message, &mut commands, &mut restart, &mut warmup);
        while let Ok(message) = receiver.try_recv() {
            collect(message, &mut commands, &mut restart, &mut warmup);
        }

        if restart {
            restart_process_now();
        } else if warmup || !commands.is_empty() {
            ensure_process();
        }
        for command in coalesce_stream_commands(commands) {
            if write_command(&command).is_err() {
                restart_process_now();
                break;
            }
        }
    }
}

fn collect(
    message: Message,
    commands: &mut Vec<HostCommand>,
    restart: &mut bool,
    warmup: &mut bool,
) {
    match message {
        Message::Command(command) => commands.push(command),
        Message::Restart => *restart = true,
        Message::Warmup => *warmup = true,
    }
}

fn coalesce_stream_commands(commands: Vec<HostCommand>) -> Vec<HostCommand> {
    let mut output: Vec<Option<HostCommand>> = Vec::with_capacity(commands.len());
    let mut latest_stream = HashMap::new();
    for command in commands {
        match &command {
            HostCommand::Stream { card } => {
                if let Some(index) = latest_stream.get(&card.id).copied() {
                    output[index] = Some(command);
                } else {
                    latest_stream.insert(card.id, output.len());
                    output.push(Some(command));
                }
            }
            HostCommand::Finalize { card } => {
                if let Some(index) = latest_stream.remove(&card.id) {
                    output[index] = None;
                }
                output.push(Some(command));
            }
            HostCommand::Upsert { card } => {
                latest_stream.remove(&card.id);
                output.push(Some(command));
            }
            HostCommand::Remove { id } => {
                latest_stream.remove(id);
                output.push(Some(command));
            }
            _ => output.push(Some(command)),
        }
    }
    output.into_iter().flatten().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::result::scene_compositor::protocol::{SceneFinalize, SceneStream};

    fn stream(id: isize, body: &str) -> HostCommand {
        HostCommand::Stream {
            card: SceneStream {
                id,
                body: body.to_string(),
                document: None,
                refining: false,
                background: "#ffffff".to_string(),
                opacity: 100,
                visible: true,
                streaming_enabled: true,
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
                background: "#ffffff".to_string(),
                opacity: 100,
                visible: true,
                streaming_enabled: false,
            },
        }
    }

    #[test]
    fn keeps_only_the_latest_pending_stream_per_card() {
        let commands = coalesce_stream_commands(vec![
            stream(1, "old"),
            stream(2, "other"),
            stream(1, "latest"),
        ]);
        assert_eq!(commands.len(), 2);
        assert!(matches!(&commands[0], HostCommand::Stream { card } if card.body == "latest"));
        assert!(matches!(&commands[1], HostCommand::Stream { card } if card.body == "other"));
    }

    #[test]
    fn finalize_supersedes_a_pending_stream_for_the_same_card() {
        let commands = coalesce_stream_commands(vec![stream(1, "partial"), finalize(1, "done")]);
        assert_eq!(commands.len(), 1);
        assert!(matches!(&commands[0], HostCommand::Finalize { card } if card.body == "done"));
    }
}
