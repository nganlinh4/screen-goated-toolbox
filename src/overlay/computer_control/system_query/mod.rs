//! Typed, read-only OS fact queries for Computer Control.
//!
//! `run_command` remains the escape hatch, but common system facts should go
//! through this registry so the model gets stable JSON from known-good sources.

mod audio;
mod filesystem;
mod process;
mod storage;

use serde_json::{Value, json};

use super::telemetry::{self, Privacy};

pub(crate) fn query(args: &Value) -> Value {
    let observed_at_ms = observed_at_ms();
    let domain = args.get("domain").and_then(Value::as_str).unwrap_or("");
    let query = args.get("query").and_then(Value::as_str).unwrap_or("");
    let payload = args.get("args").unwrap_or(&Value::Null);

    let result = if domain.is_empty() || query.is_empty() {
        failure(
            domain,
            query,
            "system_query needs string fields: domain and query",
            observed_at_ms,
        )
    } else {
        match (domain, query) {
            ("capabilities", "list") => capabilities(observed_at_ms),
            ("audio", "active_sessions") => audio::active_sessions(payload, observed_at_ms),
            ("clipboard", "text") => clipboard_text(observed_at_ms),
            ("process", "list_basic") => process::list_basic(payload, observed_at_ms),
            ("storage", "volumes") => storage::volumes(observed_at_ms),
            ("window", "list") => window_list(observed_at_ms),
            _ => failure(
                domain,
                query,
                "unsupported system_query domain/query; call capabilities.list",
                observed_at_ms,
            ),
        }
    };

    log_result(domain, query, &result);
    result
}

pub(crate) fn list_files(args: &Value) -> Value {
    filesystem::list_directory(args, observed_at_ms())
}

pub(super) fn ok(
    domain: &str,
    query: &str,
    source: &str,
    confidence: &str,
    items: Vec<Value>,
    warnings: Vec<String>,
    observed_at_ms: u128,
) -> Value {
    json!({
        "ok": true,
        "domain": domain,
        "query": query,
        "source": source,
        "confidence": confidence,
        "items": items,
        "warnings": warnings,
        "observed_at_ms": observed_at_ms,
    })
}

pub(super) fn failure(domain: &str, query: &str, error: &str, observed_at_ms: u128) -> Value {
    json!({
        "ok": false,
        "domain": domain,
        "query": query,
        "source": "system_query_registry",
        "confidence": "none",
        "items": [],
        "warnings": [error],
        "error": error,
        "observed_at_ms": observed_at_ms,
    })
}

fn capabilities(observed_at_ms: u128) -> Value {
    ok(
        "capabilities",
        "list",
        "system_query_registry",
        "high",
        vec![
            json!({"domain": "capabilities", "queries": ["list"]}),
            json!({"domain": "audio", "queries": ["active_sessions"], "source": "windows_core_audio"}),
            json!({"domain": "clipboard", "queries": ["text"], "source": "windows_clipboard"}),
            json!({"domain": "process", "queries": ["list_basic"], "source": "windows_toolhelp"}),
            json!({"domain": "storage", "queries": ["volumes"], "source": "win32_volume_api"}),
            json!({"domain": "window", "queries": ["list"], "source": "existing_uia_window_index"}),
        ],
        Vec::new(),
        observed_at_ms,
    )
}

fn clipboard_text(observed_at_ms: u128) -> Value {
    let text = super::clipboard::get_text();
    ok(
        "clipboard",
        "text",
        "windows_clipboard",
        "high",
        vec![json!({
            "text": text,
            "char_count": text.chars().count(),
        })],
        Vec::new(),
        observed_at_ms,
    )
}

fn window_list(observed_at_ms: u128) -> Value {
    let items = super::uia::list_windows()
        .into_iter()
        .map(|window| json!({"window": window}))
        .collect();
    ok(
        "window",
        "list",
        "existing_uia_window_index",
        "high",
        items,
        Vec::new(),
        observed_at_ms,
    )
}

fn observed_at_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

fn log_result(domain: &str, query: &str, result: &Value) {
    let ok = result.get("ok").and_then(Value::as_bool).unwrap_or(false);
    let source = result
        .get("source")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let confidence = result
        .get("confidence")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let item_count = result
        .get("items")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let warning_count = result
        .get("warnings")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    telemetry::human(
        "cc-system",
        format!(
            "query ok={ok} source={source} confidence={confidence} items={item_count} warnings={warning_count}"
        ),
    );
    telemetry::event(
        "system_query",
        "system_query",
        Privacy::Safe,
        json!({
            "domain": domain,
            "query": query,
            "ok": ok,
            "source": source,
            "confidence": confidence,
            "items": item_count,
            "warnings": warning_count,
        }),
    );
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn missing_fields_fail_cleanly() {
        let result = super::query(&json!({}));

        assert_eq!(
            result.get("ok").and_then(|value| value.as_bool()),
            Some(false)
        );
        assert!(
            result
                .get("error")
                .and_then(|value| value.as_str())
                .is_some()
        );
    }

    #[test]
    fn capabilities_list_advertises_core_domains() {
        let result = super::query(&json!({
            "domain": "capabilities",
            "query": "list"
        }));

        assert_eq!(
            result.get("ok").and_then(|value| value.as_bool()),
            Some(true)
        );
        let items = result
            .get("items")
            .and_then(|value| value.as_array())
            .expect("capabilities items");
        for domain in ["audio", "clipboard", "process", "window"] {
            assert!(
                items.iter().any(|item| item.get("domain").and_then(|value| value.as_str()) == Some(domain)),
                "missing {domain} domain"
            );
        }
    }
}
