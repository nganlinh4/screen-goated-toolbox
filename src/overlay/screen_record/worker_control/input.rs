//! Read authenticated commands independently of long-running command execution.

use std::io::{BufRead, BufReader};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};

use sgt_recorder_protocol::{Command, RecorderSettings};

use super::{MAX_REQUEST_LINE_BYTES, Request, read_bounded_line};

type Input = Result<Request, String>;
const MAX_QUEUED_COMMANDS: usize = 16;

pub(super) fn start(token: String) -> Receiver<Input> {
    let (sender, receiver) = mpsc::sync_channel(MAX_QUEUED_COMMANDS);
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        let mut reader = BufReader::new(stdin.lock());
        if let Err(error) = pump(
            &mut reader,
            &token,
            &sender,
            super::super::worker_settings::acknowledge,
        ) {
            let _ = sender.try_send(Err(error));
        }
    });
    receiver
}

fn pump(
    input: &mut impl BufRead,
    token: &str,
    sender: &SyncSender<Input>,
    mut acknowledge: impl FnMut(u64, Option<RecorderSettings>, Option<String>),
) -> Result<(), String> {
    let mut previous_request_id = 0;
    while let Some(line) =
        read_bounded_line(input, MAX_REQUEST_LINE_BYTES).map_err(|error| format!("{error:#}"))?
    {
        let request: Request = serde_json::from_slice(&line)
            .map_err(|error| format!("malformed recorder request: {error}"))?;
        request.validate(token, previous_request_id)?;
        previous_request_id = request.request_id;
        if let Command::SettingsAck {
            event_id,
            settings,
            error,
        } = request.command
        {
            acknowledge(event_id, settings, error);
            continue;
        }
        let shutdown = matches!(request.command, Command::Shutdown);
        match sender.try_send(Ok(request)) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => return Err("recorder command queue is full".to_string()),
            Err(TrySendError::Disconnected(_)) => return Ok(()),
        }
        if shutdown {
            return Ok(());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sgt_recorder_protocol::{PROTOCOL_VERSION, TOKEN_BYTES};

    fn frame(token: &str, id: u64, command: Command) -> Vec<u8> {
        let mut line = serde_json::to_vec(&Request {
            protocol_version: PROTOCOL_VERSION,
            token: token.to_string(),
            request_id: id,
            command,
        })
        .unwrap();
        line.push(b'\n');
        line
    }

    #[test]
    fn acknowledgement_is_delivered_while_a_normal_command_is_pending() {
        let token = "01".repeat(TOKEN_BYTES);
        let mut input = frame(&token, 1, Command::Ping);
        input.extend(frame(
            &token,
            2,
            Command::SettingsAck {
                event_id: 7,
                settings: None,
                error: Some("denied".into()),
            },
        ));
        input.extend(frame(&token, 3, Command::Shutdown));
        let (sender, receiver) = mpsc::sync_channel(MAX_QUEUED_COMMANDS);
        let mut acknowledgements = Vec::new();
        pump(
            &mut std::io::Cursor::new(input),
            &token,
            &sender,
            |id, _, _| acknowledgements.push(id),
        )
        .unwrap();
        assert_eq!(acknowledgements, vec![7]);
        assert!(matches!(
            receiver.try_recv().unwrap().unwrap().command,
            Command::Ping
        ));
        assert!(matches!(
            receiver.try_recv().unwrap().unwrap().command,
            Command::Shutdown
        ));
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn rejects_replayed_or_wrong_session_acknowledgements() {
        let token = "01".repeat(TOKEN_BYTES);
        let wrong = "02".repeat(TOKEN_BYTES);
        let ack = || Command::SettingsAck {
            event_id: 1,
            settings: None,
            error: Some("denied".into()),
        };
        let (sender, _receiver) = mpsc::sync_channel(MAX_QUEUED_COMMANDS);
        let mut called = 0;
        let mut stale = frame(&token, 1, Command::Ping);
        stale.extend(frame(&token, 1, ack()));
        assert!(
            pump(
                &mut std::io::Cursor::new(stale),
                &token,
                &sender,
                |_, _, _| called += 1
            )
            .is_err()
        );
        assert!(
            pump(
                &mut std::io::Cursor::new(frame(&wrong, 2, ack())),
                &token,
                &sender,
                |_, _, _| called += 1
            )
            .is_err()
        );
        assert_eq!(called, 0);
    }

    #[test]
    fn clean_eof_and_shutdown_do_not_run_later_commands() {
        let token = "01".repeat(TOKEN_BYTES);
        let (sender, receiver) = mpsc::sync_channel(MAX_QUEUED_COMMANDS);
        pump(
            &mut std::io::Cursor::new(Vec::<u8>::new()),
            &token,
            &sender,
            |_, _, _| panic!(),
        )
        .unwrap();
        let mut input = frame(&token, 1, Command::Shutdown);
        input.extend(frame(&token, 2, Command::Ping));
        pump(
            &mut std::io::Cursor::new(input),
            &token,
            &sender,
            |_, _, _| panic!(),
        )
        .unwrap();
        assert!(matches!(
            receiver.try_recv().unwrap().unwrap().command,
            Command::Shutdown
        ));
        assert!(receiver.try_recv().is_err());
    }
}
