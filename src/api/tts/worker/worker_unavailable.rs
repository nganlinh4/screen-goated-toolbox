use super::super::types::{AudioEvent, QueuedRequest};
use super::open_weights::fail_request;

const LEGACY_PROVIDER_LABEL: &str = "Unavailable TTS provider";
const LEGACY_PROVIDER_MESSAGE: &str = "This saved text-to-speech provider is no longer available. Choose an available provider in settings.";

pub(super) fn handle_unavailable_legacy_tts(
    request: QueuedRequest,
    tx: std::sync::mpsc::Sender<AudioEvent>,
) {
    fail_request(
        LEGACY_PROVIDER_LABEL,
        request.req.hwnd,
        &tx,
        unavailable_legacy_message(),
    );
}

fn unavailable_legacy_message() -> &'static str {
    LEGACY_PROVIDER_MESSAGE
}

#[cfg(test)]
mod tests {
    #[test]
    fn retired_capability_resolves_to_a_local_failure_only() {
        assert_eq!(
            super::unavailable_legacy_message(),
            "This saved text-to-speech provider is no longer available. Choose an available provider in settings."
        );
    }
}
