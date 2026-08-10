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
    let crucial = event.error.is_some()
        || matches!(
            event.phase.as_str(),
            "document_load_requested"
                | "document_loaded"
                | "bridge_ready"
                | "activate_font_received"
                | "finalize_received"
                | "run_fit_received"
                | "command_rejected"
                | "font_failed"
                | "finalize_failed"
                | "script_error"
                | "promise_rejection"
                | "grid_runtime_failed"
                | "final_painted"
                | "final_fit_completed"
                | "fit_timeout"
        );
    if !crucial {
        return;
    }
    crate::debug_log::log_debug(&format!(
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
    ));
}

pub(super) fn log_host_command(command: &HostCommand, text_len: usize) {
    match command {
        HostCommand::Upsert { card } => crate::debug_log::log_debug(&format!(
            "[ResultCard] id={} host=upsert visible={} streaming={} text_len={} rect={}x{}",
            card.id, card.visible, card.streaming, text_len, card.rect.width, card.rect.height
        )),
        HostCommand::Finalize { card } => crate::debug_log::log_debug(&format!(
            "[ResultCard] id={} host=finalize visible={} text_len={}",
            card.id, card.visible, text_len
        )),
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
    let layout_probes = payload["layoutProbes"].as_u64().unwrap_or(0);
    let painted_shrink_px_per_sec = payload["paintedShrinkPxPerSec"].as_f64().unwrap_or(0.0);
    let settle_before_reveal = payload["settleBeforeReveal"].as_bool().unwrap_or(false);
    let reason = payload["reason"].as_str().unwrap_or("none");
    if streaming && duration < 16.0 && reason == "none" {
        return;
    }
    crate::debug_log::log_debug(&format!(
        "[ResultCard] id={id} phase=fit action={action} fit_phase={phase} streaming={streaming} settle_before_reveal={settle_before_reveal} text_len={text_len} viewport={width}x{height} from_font_size={from_font_size:.1} target_font_size={font_size:.1} font_stretch={font_stretch:.1} painted_shrink_px_per_sec={painted_shrink_px_per_sec:.1} duration_ms={duration:.1} layout_probes={layout_probes} reason={reason}"
    ));
}
