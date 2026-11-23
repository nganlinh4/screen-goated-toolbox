use serde_json::Value;
use std::io::{self, Write};

const MAX_EVENT_BYTES: usize = 16 * 1024;

struct BoundedCount(usize);

impl Write for BoundedCount {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_EVENT_BYTES.saturating_sub(self.0) {
            return Err(io::Error::other("browser event exceeds retention limit"));
        }
        self.0 += bytes.len();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(super) fn bounded_event(event: Value) -> Value {
    if serde_json::to_writer(BoundedCount(0), &event).is_ok() {
        return event;
    }
    let method = event.get("method").and_then(Value::as_str);
    let method_truncated = method.is_none_or(|method| method.chars().nth(256).is_some());
    let method = method.map(|method| method.chars().take(256).collect::<String>());
    serde_json::json!({
        "type": "event",
        "tabId": event.get("tabId").and_then(Value::as_i64),
        "method": method,
        "method_truncated": method_truncated,
        "event_omitted": true,
    })
}

pub(in crate::overlay::computer_control::browser) fn recent_events_on_tab(
    filter: &str,
    limit: usize,
    tab_id: i64,
) -> Vec<Value> {
    let q = super::bridge().events.lock().unwrap();
    q.iter()
        .filter(|event| event_matches(event, filter, Some(tab_id)))
        .rev()
        .take(limit)
        .cloned()
        .collect()
}

#[cfg(test)]
pub(in crate::overlay::computer_control::browser) fn record_for_test(event: Value) {
    super::record_event(event);
}

fn event_matches(event: &Value, filter: &str, tab_id: Option<i64>) -> bool {
    let method_matches = filter.is_empty()
        || event.get("method_truncated").and_then(Value::as_bool) == Some(true)
        || event
            .get("method")
            .and_then(Value::as_str)
            .is_some_and(|method| method.contains(filter));
    let event_tab = event.get("tabId").and_then(Value::as_i64);
    let tab_matches = tab_id.is_none()
        || event_tab == tab_id
        || (event_tab.is_none()
            && event.get("event_omitted").and_then(Value::as_bool) == Some(true));
    method_matches && tab_matches
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn event_filter_matches_method_and_exact_root_tab() {
        let event = json!({"method": "Network.responseReceived", "tabId": 31});
        assert!(event_matches(&event, "response", Some(31)));
        assert!(!event_matches(&event, "response", Some(32)));
        assert!(!event_matches(&event, "console", Some(31)));
        assert!(event_matches(&event, "", None));
    }

    #[test]
    fn oversized_events_keep_identity_and_explicitly_omit_payload() {
        let original = json!({"type":"event", "method":"Runtime.consoleAPICalled", "tabId":31, "params":{"text":"x".repeat(100_000)}});
        let retained = bounded_event(original);
        assert_eq!(retained["event_omitted"], true);
        assert_eq!(retained["tabId"], 31);
        assert!(event_matches(&retained, "console", Some(31)));
        assert!(retained.get("params").is_none());
        let long_method = json!({"type":"event", "method":format!("{}console", "x".repeat(300)), "tabId":31, "params":{"text":"x".repeat(100_000)}});
        let retained = bounded_event(long_method);
        assert_eq!(retained["method_truncated"], true);
        assert!(event_matches(&retained, "console", Some(31)));
        assert!(!event_matches(&retained, "console", Some(32)));
        let unknown_tab = bounded_event(
            json!({"type":"event", "method":"Runtime.consoleAPICalled", "params":{"text":"x".repeat(100_000)}}),
        );
        assert!(event_matches(&unknown_tab, "console", Some(32)));
        let small = json!({"type":"event", "method":"Network.responseReceived", "tabId":31, "params":{"status":200}});
        assert_eq!(bounded_event(small.clone()), small);
    }
}
