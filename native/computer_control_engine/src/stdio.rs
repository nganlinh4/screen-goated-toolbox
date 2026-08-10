use std::io::{BufRead, BufReader, BufWriter, Write};

use anyhow::{Context, Result, bail};
use sgt_computer_control_protocol::{
    Command, MAX_JSON_BYTES, MAX_REQUEST_LINE_BYTES, PROTOCOL_VERSION, RESPONSE_PREFIX, Request,
    Response,
};

pub(super) fn serve(
    token: &str,
    mut dispatch: impl FnMut(Command) -> Result<sgt_computer_control_protocol::Output>,
) -> Result<()> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = BufWriter::new(stdout.lock());
    let mut previous_request_id = 0_u64;
    let mut handshaken = false;

    while let Some(line) = read_bounded_line(&mut reader, MAX_REQUEST_LINE_BYTES)? {
        let request = match serde_json::from_slice::<Request>(&line) {
            Ok(request) => request,
            Err(error) => {
                bail!("request JSON is malformed: {error}");
            }
        };
        request
            .validate(token, previous_request_id)
            .map_err(anyhow::Error::msg)?;
        previous_request_id = request.request_id;
        let is_handshake = matches!(request.command, Command::Handshake { .. });
        let is_shutdown = matches!(request.command, Command::Shutdown);
        let outcome = if !handshaken && !is_handshake {
            Err(anyhow::anyhow!("protocol handshake is required"))
        } else if handshaken && is_handshake {
            Err(anyhow::anyhow!("protocol handshake was already completed"))
        } else {
            dispatch(request.command)
        };
        if is_handshake && outcome.is_ok() {
            handshaken = true;
        }
        write_response(&mut writer, token, request.request_id, outcome)?;
        if is_shutdown {
            return Ok(());
        }
    }
    Ok(())
}

fn write_response(
    writer: &mut impl Write,
    token: &str,
    request_id: u64,
    outcome: Result<sgt_computer_control_protocol::Output>,
) -> Result<()> {
    let mut response = match outcome {
        Ok(output) => Response {
            protocol_version: PROTOCOL_VERSION,
            token: token.to_string(),
            request_id,
            output: Some(output),
            error: None,
        },
        Err(error) => Response {
            protocol_version: PROTOCOL_VERSION,
            token: token.to_string(),
            request_id,
            output: None,
            error: Some(format!("{error:#}")),
        },
    };
    let mut body = serde_json::to_vec(&response)?;
    if body.len() > MAX_JSON_BYTES {
        response.output = None;
        response.error = Some("Computer Control engine response exceeds protocol limit".into());
        body = serde_json::to_vec(&response)?;
        if body.len() > MAX_JSON_BYTES {
            bail!("bounded error response exceeds protocol limit");
        }
    }
    writer.write_all(RESPONSE_PREFIX.as_bytes())?;
    writer.write_all(&body)?;
    writer.write_all(b"\n")?;
    writer.flush().context("flush engine response")
}

fn read_bounded_line(reader: &mut impl BufRead, maximum: usize) -> Result<Option<Vec<u8>>> {
    let mut line = Vec::new();
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return if line.is_empty() {
                Ok(None)
            } else {
                bail!("request ended before newline")
            };
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let take = newline.map_or(available.len(), |index| index + 1);
        if line.len().saturating_add(take) > maximum {
            bail!("request exceeds protocol limit");
        }
        line.extend_from_slice(&available[..take]);
        reader.consume(take);
        if newline.is_some() {
            while matches!(line.last(), Some(b'\n' | b'\r')) {
                line.pop();
            }
            return Ok(Some(line));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_reader_rejects_oversize_and_partial_frames() {
        let mut oversized = vec![b'x'; MAX_REQUEST_LINE_BYTES];
        oversized.push(b'\n');
        assert!(
            read_bounded_line(&mut std::io::Cursor::new(oversized), MAX_REQUEST_LINE_BYTES)
                .is_err()
        );
        assert!(read_bounded_line(&mut std::io::Cursor::new(b"partial"), 32).is_err());
    }
}
