use crate::process_job::JobGroup;
use std::io::BufReader;
use std::sync::{Arc, mpsc};

use sgt_recorder_protocol::{
    MAX_RESPONSE_LINE_BYTES, RESPONSE_PREFIX, Response, SETTINGS_PREFIX, SettingsEvent,
};

use super::{ResponseEvent, framing::read_bounded_line, session_writer::SessionWriter, settings};

pub(super) fn start_response_reader(
    stdout: impl std::io::Read + Send + 'static,
    token: String,
    writer: Arc<SessionWriter>,
    job: JobGroup,
    progress: Arc<super::progress::ProgressSession>,
) -> mpsc::Receiver<ResponseEvent> {
    let (sender, receiver) = mpsc::sync_channel(32);
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut previous_event_id = 0;
        loop {
            let line = match read_bounded_line(&mut reader, MAX_RESPONSE_LINE_BYTES) {
                Ok(Some(line)) => line,
                Ok(None) => {
                    progress.retire();
                    let _ = sender.send(ResponseEvent::Failed(
                        "recorder worker closed its response stream".to_string(),
                    ));
                    return;
                }
                Err(error) => {
                    progress.retire();
                    let _ = sender.send(ResponseEvent::Failed(format!(
                        "recorder worker response framing failed: {error:#}"
                    )));
                    return;
                }
            };
            if let Some(body) = line.strip_prefix(SETTINGS_PREFIX.as_bytes()) {
                match serde_json::from_slice::<SettingsEvent>(body)
                    .map_err(|error| error.to_string())
                    .and_then(|event| {
                        event.validate(&token, previous_event_id)?;
                        Ok(event)
                    }) {
                    Ok(event) => {
                        previous_event_id = event.event_id;
                        if let Some(ack) = settings::process(event)
                            && let Err(error) = writer.send(ack)
                        {
                            progress.retire();
                            let _ = sender.send(ResponseEvent::Failed(format!(
                                "recorder settings acknowledgement failed: {error:#}"
                            )));
                            let _ = job.terminate();
                            return;
                        }
                    }
                    Err(error) => {
                        progress.retire();
                        let _ = sender.send(ResponseEvent::Failed(error));
                        return;
                    }
                }
                continue;
            }
            let Some(body) = line.strip_prefix(RESPONSE_PREFIX.as_bytes()) else {
                continue;
            };
            match serde_json::from_slice::<Response>(body) {
                Ok(response) => {
                    if sender.send(ResponseEvent::Response(response)).is_err() {
                        progress.retire();
                        return;
                    }
                }
                Err(error) => {
                    progress.retire();
                    let _ = sender.send(ResponseEvent::Failed(format!(
                        "recorder worker returned malformed JSON: {error}"
                    )));
                    return;
                }
            }
        }
    });
    receiver
}
