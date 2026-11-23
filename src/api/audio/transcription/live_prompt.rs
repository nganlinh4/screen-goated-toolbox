use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use native_tls::TlsStream;
use tungstenite::{Message, WebSocket};

pub(super) fn check_cancelled(cancelled: Option<&AtomicBool>) -> Result<()> {
    anyhow::ensure!(
        !cancelled.is_some_and(|flag| flag.load(Ordering::SeqCst)),
        "Audio processing cancelled"
    );
    Ok(())
}

fn setup(model: &str, prompt: &str, manual_activity: bool) -> serde_json::Value {
    use crate::api::gemini_live::setup::{LiveSetupBuilder, MediaResolution, TranscriptionMode};

    let mut builder = LiveSetupBuilder::new(model)
        .media_resolution(MediaResolution::Low)
        .transcription(TranscriptionMode::Output)
        .system_instruction(prompt);
    if manual_activity {
        builder = builder.setup_field(
            "realtimeInputConfig",
            serde_json::json!({"automaticActivityDetection": {"disabled": true}}),
        );
    }
    builder.build()
}

pub(super) fn send_setup(
    socket: &mut WebSocket<TlsStream<TcpStream>>,
    model: &str,
    prompt: &str,
    manual_activity: bool,
) -> Result<()> {
    socket.write(Message::Text(
        setup(model, prompt, manual_activity).to_string().into(),
    ))?;
    socket.flush()?;
    Ok(())
}

pub(super) struct ResponseState {
    require_interaction_idle: bool,
    input_finished: bool,
}

impl ResponseState {
    pub(super) fn new(require_interaction_idle: bool) -> Self {
        Self {
            require_interaction_idle,
            input_finished: false,
        }
    }

    pub(super) fn finish_input(&mut self) {
        self.input_finished = true;
    }

    pub(super) fn observe_setup(&self, message: &str) -> Result<bool> {
        let frame = crate::api::gemini_live::server_frame::parse_server_frame(message)?;
        self.observe(message)?;
        if frame.has_post_setup_observation() {
            anyhow::bail!("Live audio provider sent unexpected data during setup");
        }
        Ok(frame.setup_complete)
    }

    pub(super) fn observe(&self, message: &str) -> Result<bool> {
        let frame = crate::api::gemini_live::server_frame::parse_server_frame(message)?;
        if let Some(error) = &frame.error {
            anyhow::bail!("Live audio provider error: {error}");
        }
        if frame.interrupted {
            anyhow::bail!("Live audio response was interrupted");
        }
        if frame.tool_call_present {
            anyhow::bail!("Live audio provider requested an unsupported tool call");
        }
        let complete = frame.finite_response_complete(self.require_interaction_idle);
        if complete && !self.input_finished {
            anyhow::bail!("Live audio response completed before all audio was sent");
        }
        Ok(complete)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_setup_requests_output_transcription_and_preserves_instruction() {
        let value = setup("example-live-model", "Summarize the audio", true);
        let body = &value["setup"];
        assert_eq!(
            body["systemInstruction"]["parts"][0]["text"],
            "Summarize the audio"
        );
        assert!(body["outputAudioTranscription"].is_object());
        assert!(body.get("inputAudioTranscription").is_none());
        assert_eq!(
            body["realtimeInputConfig"]["automaticActivityDetection"]["disabled"],
            true
        );
    }

    #[test]
    fn completion_requires_an_uninterrupted_terminal_frame() {
        let mut state = ResponseState::new(false);
        assert!(state.observe_setup(r#"{"setupComplete":{}}"#).unwrap());
        assert!(
            state
                .observe_setup(r#"{"setupComplete":{},"error":{"message":"bad"}}"#)
                .is_err()
        );
        assert!(
            state
                .observe_setup(r#"{"setupComplete":{},"serverContent":{"turnComplete":true}}"#)
                .is_err()
        );
        assert!(state.observe_setup(r#"{"setupComplete":{},"serverContent":{"outputTranscription":{"text":"early"}}}"#).is_err());
        assert!(
            !state
                .observe(r#"{"serverContent":{"outputTranscription":{"text":"partial"}}}"#,)
                .unwrap()
        );
        assert!(
            state
                .observe(r#"{"serverContent":{"turnComplete":true}}"#)
                .is_err()
        );
        assert!(
            state
                .observe(r#"{"error":{"message":"rejected"}}"#)
                .is_err()
        );
        assert!(
            state
                .observe(r#"{"serverContent":{"interrupted":true}}"#)
                .is_err()
        );
        assert!(state.observe(r#"{"toolCall":{}}"#).is_err());
        assert!(state.observe("not JSON").is_err());
        state.finish_input();
        assert!(
            state
                .observe(r#"{"serverContent":{"turnComplete":true}}"#)
                .unwrap()
        );
        let mut idle = ResponseState::new(true);
        idle.finish_input();
        assert!(
            !idle
                .observe(
                    r#"{"serverContent":{"turnComplete":true,"interactionStatus":"IN_PROGRESS"}}"#
                )
                .unwrap()
        );
        assert!(
            idle.observe(r#"{"serverContent":{"interactionStatus":"IDLE"}}"#)
                .unwrap()
        );
    }
}
