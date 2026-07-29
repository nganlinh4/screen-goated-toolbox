use serde_json::{Value, json};
use std::io::BufRead;

const MAX_EVENT_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    None,
    Accepted,
    Recovering,
    Completed,
}

pub struct Identity<'a> {
    pub tool: &'a str,
    pub operation: &'a str,
    pub dispatch_id: &'a str,
    pub request_fingerprint: &'a str,
}

pub fn query_message(id: &str, identity: &Identity<'_>) -> Value {
    json!({
        "id": id,
        "cmd": "query_creation_recovery",
        "args": {
            "dispatchId": identity.dispatch_id,
            "tool": identity.tool,
            "operation": identity.operation,
            "requestFingerprint": identity.request_fingerprint,
        }
    })
}

pub fn resume_message(id: &str, identity: &Identity<'_>, request: Value) -> Value {
    json!({
        "id": id,
        "cmd": "resume_creation",
        "args": {
            "dispatchId": identity.dispatch_id,
            "tool": identity.tool,
            "operation": identity.operation,
            "requestFingerprint": identity.request_fingerprint,
            "request": request,
        }
    })
}

pub fn parse_query_response(value: &Value, identity: &Identity<'_>) -> Result<State, String> {
    if value.get("ok").and_then(Value::as_bool) != Some(true) {
        return Err(public_error(value));
    }
    let result = value
        .get("result")
        .ok_or_else(|| "Creation recovery result is unavailable.".to_string())?;
    if result.get("dispatchId").and_then(Value::as_str) != Some(identity.dispatch_id)
        || result.get("tool").and_then(Value::as_str) != Some(identity.tool)
        || result.get("operation").and_then(Value::as_str) != Some(identity.operation)
        || result.get("requestFingerprint").and_then(Value::as_str)
            != Some(identity.request_fingerprint)
    {
        return Err("Creation recovery identity did not match.".to_string());
    }
    match result.get("state").and_then(Value::as_str) {
        Some("none") => Ok(State::None),
        Some("accepted") => Ok(State::Accepted),
        Some("recovering") => Ok(State::Recovering),
        Some("completed") => Ok(State::Completed),
        _ => Err("Creation recovery state is invalid.".to_string()),
    }
}

pub fn public_error(value: &Value) -> String {
    match value.get("error").and_then(Value::as_str) {
        Some("creation.recovery_conflict") => {
            "This saved creation request no longer matches its recovery record.".to_string()
        }
        Some("creation.recovery_not_found") => {
            "This creation request could not be recovered.".to_string()
        }
        _ => "Creation recovery could not finish.".to_string(),
    }
}

pub fn read_event(reader: &mut impl BufRead) -> Result<Option<Value>, String> {
    let mut bytes = Vec::new();
    loop {
        let available = reader
            .fill_buf()
            .map_err(|_| "Creation response could not be read.".to_string())?;
        if available.is_empty() {
            if bytes.is_empty() {
                return Ok(None);
            }
            return Err("Creation response ended mid-event.".to_string());
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let take = newline.map_or(available.len(), |index| index + 1);
        if bytes.len().saturating_add(take) > MAX_EVENT_BYTES {
            reader.consume(take);
            while newline.is_none() {
                let available = reader
                    .fill_buf()
                    .map_err(|_| "Creation response could not be read.".to_string())?;
                if available.is_empty() {
                    break;
                }
                let end = available
                    .iter()
                    .position(|byte| *byte == b'\n')
                    .map_or(available.len(), |index| index + 1);
                let complete = available.get(end.saturating_sub(1)) == Some(&b'\n');
                reader.consume(end);
                if complete {
                    break;
                }
            }
            return Err("Creation response event is too large.".to_string());
        }
        bytes.extend_from_slice(&available[..take]);
        reader.consume(take);
        if newline.is_some() {
            while bytes
                .last()
                .is_some_and(|byte| matches!(byte, b'\n' | b'\r'))
            {
                bytes.pop();
            }
            if bytes.is_empty() {
                return Ok(Some(Value::Null));
            }
            return serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|_| "Creation response event is invalid.".to_string());
        }
    }
}

pub fn terminate_process_tree(pid: u32) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .creation_flags(0x0800_0000)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_status_requires_the_exact_frozen_identity() {
        let identity = Identity {
            tool: "image",
            operation: "create_image",
            dispatch_id: "dispatch-1",
            request_fingerprint: "abc123",
        };
        let accepted = json!({
            "ok": true,
            "result": {
                "dispatchId": "dispatch-1",
                "tool": "image",
                "operation": "create_image",
                "requestFingerprint": "abc123",
                "state": "accepted"
            }
        });
        assert_eq!(
            parse_query_response(&accepted, &identity).unwrap(),
            State::Accepted
        );
        let mut mismatch = accepted;
        mismatch["result"]["requestFingerprint"] = json!("different");
        assert!(parse_query_response(&mismatch, &identity).is_err());
    }

    #[test]
    fn recovery_errors_are_reduced_to_product_copy() {
        let error = json!({"ok": false, "error": "private detail"});
        assert_eq!(
            public_error(&error),
            "Creation recovery could not finish.".to_string()
        );
    }

    #[test]
    fn event_reader_bounds_unterminated_and_oversized_frames() {
        let mut valid = std::io::Cursor::new(b"{\"ok\":true}\n".to_vec());
        assert_eq!(read_event(&mut valid).unwrap().unwrap()["ok"], true);
        let mut unterminated = std::io::Cursor::new(b"{\"ok\":true}".to_vec());
        assert!(read_event(&mut unterminated).is_err());
        let mut oversized = std::io::Cursor::new(vec![b'x'; MAX_EVENT_BYTES + 1]);
        assert!(read_event(&mut oversized).is_err());
    }
}
