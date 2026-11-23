//! Structural classification of one raw Live socket read for batch audio
//! transcription, so server errors, premature closes and hard read errors
//! fail the request instead of ending it as an empty success.

use crate::api::gemini_live::transport::is_transient_socket_read_error;
use crate::api::gemini_live::websocket::parse_error;

#[derive(Debug, PartialEq, Eq)]
pub(super) enum LiveRead {
    /// A decoded JSON protocol frame.
    Text(String),
    /// A control frame (ping/pong/raw frame) with no transcript content.
    Other,
    /// No frame was available before the read timeout.
    Idle,
}

pub(super) fn classify_live_read(
    read: tungstenite::Result<tungstenite::Message>,
    phase: &str,
) -> anyhow::Result<LiveRead> {
    let text = match read {
        Ok(tungstenite::Message::Text(text)) => text.as_str().to_owned(),
        Ok(tungstenite::Message::Binary(data)) => String::from_utf8(data.to_vec())
            .map_err(|_| anyhow::anyhow!("Live audio provider sent invalid UTF-8 {phase}"))?,
        Ok(tungstenite::Message::Close(frame)) => {
            let detail = frame
                .map(|frame| format!(" (code {}: {})", u16::from(frame.code), frame.reason))
                .unwrap_or_default();
            anyhow::bail!("Live audio connection closed {phase}{detail}");
        }
        Ok(_) => return Ok(LiveRead::Other),
        Err(error) if is_transient_socket_read_error(&error) => return Ok(LiveRead::Idle),
        Err(error) => return Err(error.into()),
    };
    if let Some(error) = parse_error(&text) {
        anyhow::bail!("Server returned error: {error}");
    }
    Ok(LiveRead::Text(text))
}

/// Read until the server acknowledges setup. Transient reads (including
/// overlapped I/O still pending) keep waiting until `timeout`; error frames,
/// closes and hard read errors fail. `is_complete` judges each JSON frame.
pub(super) fn wait_for_setup(
    mut read: impl FnMut() -> tungstenite::Result<tungstenite::Message>,
    mut is_complete: impl FnMut(&str) -> anyhow::Result<bool>,
    mut check_cancelled: impl FnMut() -> anyhow::Result<()>,
    timeout: std::time::Duration,
) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    loop {
        check_cancelled()?;
        match classify_live_read(read(), "during setup")? {
            LiveRead::Text(message) => {
                if is_complete(&message)? {
                    return Ok(());
                }
            }
            LiveRead::Other => {}
            LiveRead::Idle => {
                anyhow::ensure!(started.elapsed() <= timeout, "Setup timeout");
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tungstenite::Message;

    const PHASE: &str = "before response completion";

    #[test]
    fn server_error_frame_fails_instead_of_returning_empty_text() {
        let frame = r#"{"error":{"code":429,"message":"RESOURCE_EXHAUSTED","status":"RESOURCE_EXHAUSTED"}}"#;
        let error = classify_live_read(Ok(Message::Text(frame.into())), PHASE).unwrap_err();
        assert!(error.to_string().contains("RESOURCE_EXHAUSTED"), "{error}");
        let transcript = r#"{"serverContent":{"inputTranscription":{"text":"hi"}}}"#;
        assert_eq!(
            classify_live_read(
                Ok(Message::Binary(transcript.as_bytes().to_vec().into())),
                PHASE
            )
            .unwrap(),
            LiveRead::Text(transcript.to_string())
        );
    }

    #[test]
    fn premature_close_and_hard_read_errors_fail() {
        assert!(classify_live_read(Ok(Message::Close(None)), PHASE).is_err());
        let reset = std::io::Error::from(std::io::ErrorKind::ConnectionReset);
        assert!(classify_live_read(Err(tungstenite::Error::Io(reset)), PHASE).is_err());
    }

    #[test]
    fn overlapped_io_pending_is_a_transient_idle_read() {
        let pending = std::io::Error::from_raw_os_error(997);
        assert_eq!(
            classify_live_read(Err(tungstenite::Error::Io(pending)), PHASE).unwrap(),
            LiveRead::Idle
        );
        let would_block = std::io::Error::from(std::io::ErrorKind::WouldBlock);
        assert_eq!(
            classify_live_read(Err(tungstenite::Error::Io(would_block)), PHASE).unwrap(),
            LiveRead::Idle
        );
        assert_eq!(
            classify_live_read(Ok(Message::Ping(Vec::new().into())), PHASE).unwrap(),
            LiveRead::Other
        );
    }

    #[test]
    fn setup_wait_survives_pending_io_and_fails_on_errors_or_close() {
        let complete = |message: &str| {
            Ok(crate::api::gemini_live::websocket::is_setup_complete(
                message,
            ))
        };
        let timeout = std::time::Duration::from_secs(5);
        let mut reads = vec![
            Err(tungstenite::Error::Io(std::io::Error::from_raw_os_error(
                997,
            ))),
            Ok(Message::Text(r#"{"setupComplete":{}}"#.into())),
        ]
        .into_iter();
        wait_for_setup(|| reads.next().unwrap(), complete, || Ok(()), timeout).unwrap();

        for failing in [
            Ok(Message::Close(None)),
            Ok(Message::Text(r#"{"error":{"message":"bad setup"}}"#.into())),
            Err(tungstenite::Error::Io(
                std::io::ErrorKind::ConnectionReset.into(),
            )),
        ] {
            let mut reads = vec![failing].into_iter();
            assert!(
                wait_for_setup(|| reads.next().unwrap(), complete, || Ok(()), timeout).is_err()
            );
        }
        let error = wait_for_setup(
            || {
                Err(tungstenite::Error::Io(
                    std::io::ErrorKind::WouldBlock.into(),
                ))
            },
            complete,
            || Ok(()),
            std::time::Duration::ZERO,
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "Setup timeout");
    }
}
