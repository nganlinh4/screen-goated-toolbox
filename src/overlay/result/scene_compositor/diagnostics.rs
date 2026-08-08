use super::protocol::HostCommand;

pub(super) struct CardDiagnosticLog {
    pub id: isize,
    pub phase: String,
    pub revision: u64,
    pub visible: bool,
    pub ready: bool,
    pub payload_len: usize,
    pub text_len: usize,
    pub opacity: String,
    pub error: Option<String>,
}

pub(super) fn log_card_diagnostic(event: CardDiagnosticLog) {
    crate::log_info!(
        "[ResultCard] id={} phase={} revision={} visible={} ready={} payload_len={} text_len={} opacity={} error={}",
        event.id,
        event.phase,
        event.revision,
        event.visible,
        event.ready,
        event.payload_len,
        event.text_len,
        if event.opacity.is_empty() {
            "unknown"
        } else {
            &event.opacity
        },
        event.error.as_deref().unwrap_or("none")
    );
}

pub(super) fn log_host_command(command: &HostCommand, text_len: usize) {
    match command {
        HostCommand::Upsert { card } => crate::log_info!(
            "[ResultCard] id={} host=upsert visible={} streaming={} text_len={} rect={}x{}",
            card.id,
            card.visible,
            card.streaming,
            text_len,
            card.rect.width,
            card.rect.height
        ),
        HostCommand::Finalize { card } => crate::log_info!(
            "[ResultCard] id={} host=finalize visible={} text_len={}",
            card.id,
            card.visible,
            text_len
        ),
        _ => {}
    }
}

pub(super) fn log_fit_diagnostic(id: isize, payload: &serde_json::Value) {
    let action = payload["action"].as_str().unwrap_or("unknown");
    let phase = payload["phase"].as_str().unwrap_or("unknown");
    let streaming = payload["streamingFit"].as_bool().unwrap_or(false);
    let text_len = payload["textLen"]
        .as_u64()
        .or_else(|| payload["renderedTextLen"].as_u64())
        .unwrap_or(0);
    let width = payload["winW"].as_u64().unwrap_or(0);
    let height = payload["winH"].as_u64().unwrap_or(0);
    let from_font_size = payload["fromFontSize"].as_f64().unwrap_or(0.0);
    let font_size = payload["fontSize"].as_f64().unwrap_or(0.0);
    let font_stretch = payload["fontStretch"].as_f64().unwrap_or(0.0);
    let duration = payload["fitDurationMs"].as_f64().unwrap_or(0.0);
    let reason = payload["reason"].as_str().unwrap_or("none");
    crate::log_info!(
        "[ResultCard] id={id} phase=fit action={action} fit_phase={phase} streaming={streaming} text_len={text_len} viewport={width}x{height} from_font_size={from_font_size:.1} target_font_size={font_size:.1} font_stretch={font_stretch:.1} duration_ms={duration:.1} reason={reason}"
    );
}
