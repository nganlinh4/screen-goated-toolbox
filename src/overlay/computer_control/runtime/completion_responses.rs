//! Bounded model-facing response for the model's terminal signal.

use serde_json::{Value, json};

pub(super) fn accepted_done_response(summary: &str) -> Value {
    let summary = summary.trim().chars().take(320).collect::<String>();
    json!({
        "ok": true,
        "completion_status": "model_declared",
        "summary": summary,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_signal_keeps_one_bounded_model_authored_summary() {
        let response = accepted_done_response(&"x".repeat(400));
        assert_eq!(response["ok"], true);
        assert_eq!(response["completion_status"], "model_declared");
        assert_eq!(response["summary"].as_str().unwrap().chars().count(), 320);
    }
}
