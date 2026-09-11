//! Opt-in transcript evidence, never destination document contents.
use super::policy::{Event, sanitize};
use std::sync::OnceLock;

fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("SGT_AUTOPASTE_TEXT_DIAGNOSTICS").as_deref() == Ok("1"))
}

fn payload(text: &str) -> serde_json::Value {
    // Two JSON fields, each at most six escaped bytes per scalar, fit the
    // logger's 64 KiB complete-line budget including metadata.
    const LIMIT: usize = 4_096;
    let captured: String = text.chars().take(LIMIT).collect();
    serde_json::json!({
        "text": captured,
        "chars": text.chars().count(),
        "utf16_units": text.encode_utf16().count(),
        "utf8_bytes": text.len(),
        "truncated": captured.len() != text.len(),
    })
}

pub(super) fn received(session: u64, sequence: u64, event: &Event) {
    if !enabled() {
        return;
    }
    let text = match event {
        Event::Interim(text) | Event::Final(text) => text,
        Event::Finish => "",
    };
    crate::log_info!(
        "[AutoPasteText] {}",
        serde_json::json!({
            "pid": std::process::id(), "session": session, "received": sequence,
            "stage": "received", "kind": event.kind(), "raw": payload(text),
            "sanitized": payload(&sanitize(text)),
        })
    );
}

pub(super) fn dispatched(session: u64, sequence: u64, kind: &str, old: &str, new: &str) {
    if !enabled() {
        return;
    }
    crate::log_info!(
        "[AutoPasteText] {}",
        serde_json::json!({
            "pid": std::process::id(), "session": session, "dispatch": sequence,
            "stage": "attempt", "kind": kind, "old": payload(old), "new": payload(new),
            "destination_readback": false,
        })
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_text_preserves_spaces_combining_characters_and_controls() {
        let text = " a\u{301} 🦀\r\n ";
        let value = payload(text);
        let encoded = serde_json::to_string(&value).unwrap();
        assert!(!encoded.contains('\n'));
        let decoded: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded["text"], text);
        assert_eq!(decoded["chars"], text.chars().count());
        assert_eq!(decoded["utf16_units"], text.encode_utf16().count());
        assert_eq!(decoded["truncated"], false);
    }
    #[test]
    fn bounded_payload_explicitly_marks_incomplete_evidence() {
        let value = payload(&"🦀".repeat(4_097));
        assert_eq!(value["text"].as_str().unwrap().chars().count(), 4_096);
        assert_eq!(value["chars"], 4_097);
        assert_eq!(value["truncated"], true);
    }
    #[test]
    fn escaped_pair_fits_complete_log_line() {
        let value = payload(&"\0".repeat(4_097));
        let record = serde_json::json!({"old": value, "new": value});
        assert!(record.to_string().len() + 1024 < 64 * 1024);
    }
}
