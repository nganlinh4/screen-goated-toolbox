//! Edge synthesis ownership, limits and completion validation.
use std::time::{Duration, Instant};

pub(super) const SETUP_BUDGET: Duration = Duration::from_secs(10);
pub(super) const SESSION_BUDGET: Duration = Duration::from_secs(180);
pub(super) const MAX_AUDIO_BYTES: usize = 16 * 1024 * 1024;

pub(super) fn wait_for_io(
    started: Instant,
    budget: Duration,
    cancelled: &impl Fn() -> bool,
) -> Result<(), &'static str> {
    if cancelled() {
        return Err("cancelled");
    }
    if started.elapsed() >= budget {
        return Err("Edge connection timed out");
    }
    std::thread::sleep(Duration::from_millis(10));
    Ok(())
}

pub(super) fn send_message<S: std::io::Read + std::io::Write>(
    socket: &mut tungstenite::WebSocket<S>,
    message: tungstenite::Message,
    started: Instant,
    cancelled: &impl Fn() -> bool,
) -> Result<(), &'static str> {
    if cancelled() {
        return Err("cancelled");
    }
    if started.elapsed() >= SESSION_BUDGET {
        return Err("Edge synthesis timed out");
    }
    let sent = socket.send(message);
    if let Err(ref error) = sent
        && !crate::api::gemini_live::transport::is_transient_socket_read_error(error)
    {
        return Err("Edge connection failed while sending request");
    }
    if sent.is_err() {
        loop {
            wait_for_io(started, SESSION_BUDGET, cancelled)?;
            match socket.flush() {
                Ok(()) => break,
                Err(ref error)
                    if crate::api::gemini_live::transport::is_transient_socket_read_error(
                        error,
                    ) => {}
                Err(_) => return Err("Edge connection failed while sending request"),
            }
        }
    }
    if cancelled() {
        return Err("cancelled");
    }
    if started.elapsed() >= SESSION_BUDGET {
        return Err("Edge synthesis timed out");
    }
    Ok(())
}

pub(super) fn receive_audio(
    mut read: impl FnMut() -> tungstenite::Result<tungstenite::Message>,
    cancelled: impl Fn() -> bool,
    expired: impl Fn() -> bool,
) -> Result<Vec<u8>, &'static str> {
    use tungstenite::Message;
    let mut audio = AudioBuffer::default();
    loop {
        if cancelled() {
            return Err("cancelled");
        }
        if expired() {
            return Err("Edge synthesis timed out");
        }
        let next = read();
        if cancelled() {
            return Err("cancelled");
        }
        if expired() {
            return Err("Edge synthesis timed out");
        }
        match next {
            Ok(Message::Binary(data)) => {
                if data.len() < 2 {
                    return Err("Malformed Edge audio frame");
                }
                let start = 2 + u16::from_be_bytes([data[0], data[1]]) as usize;
                if start > data.len() {
                    return Err("Malformed Edge audio frame");
                }
                if data[2..start]
                    .windows(11)
                    .any(|window| window == b"Path:audio\r")
                {
                    audio.push(&data[start..])?;
                }
            }
            Ok(Message::Text(text)) if is_turn_end(text.as_str()) => return audio.finish(),
            Ok(Message::Close(_)) => return Err("Edge connection closed before completion"),
            Err(ref error)
                if crate::api::gemini_live::transport::is_transient_socket_read_error(error) =>
            {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(_) => return Err("Edge connection failed before completion"),
            _ => {}
        }
    }
}

#[derive(Default)]
pub(super) struct AudioBuffer {
    bytes: Vec<u8>,
}

impl AudioBuffer {
    pub(super) fn push(&mut self, bytes: &[u8]) -> Result<(), &'static str> {
        if bytes.len() > MAX_AUDIO_BYTES.saturating_sub(self.bytes.len()) {
            return Err("Edge audio exceeds the size limit");
        }
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
    pub(super) fn finish(self) -> Result<Vec<u8>, &'static str> {
        if self.bytes.is_empty() {
            Err("Edge response contained no audio")
        } else {
            Ok(self.bytes)
        }
    }
}

pub(super) fn is_turn_end(text: &str) -> bool {
    text.split_once("\r\n\r\n").is_some_and(|(headers, _)| {
        headers
            .lines()
            .any(|line| line.trim_end_matches('\r') == "Path:turn.end")
    })
}

#[cfg(test)]
#[path = "edge_transport_tests.rs"]
mod tests;
