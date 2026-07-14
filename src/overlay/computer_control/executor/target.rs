use anyhow::{Result, anyhow};
use serde_json::Value;

use super::super::controller::world::BrowserWindowIdentity;

#[derive(Debug, PartialEq, Eq)]
enum ExpectedInputTarget {
    Native {
        hwnd: u64,
        pid: u64,
        generation: u64,
        element: Option<ExpectedNativeElement>,
    },
    Browser {
        tab_id: i64,
        document_id: String,
        window: BrowserWindowIdentity,
    },
}

#[derive(Debug, PartialEq, Eq)]
struct ExpectedNativeElement {
    x: i32,
    y: i32,
    role: String,
    provider_name: String,
    automation_id: String,
    runtime_id: Vec<i32>,
}

fn expected_input_target(args: &Value) -> Result<Option<ExpectedInputTarget>> {
    let Some(expected) = args.get("expected_input_target") else {
        return Ok(None);
    };
    if expected.get("kind").and_then(Value::as_str) == Some("browser") {
        let tab_id = expected.get("tab_id").and_then(Value::as_i64).unwrap_or(0);
        let document_id = expected
            .get("document_id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        let window = BrowserWindowIdentity {
            browser_window_id: expected
                .get("browser_window_id")
                .and_then(Value::as_i64)
                .unwrap_or(0),
            hwnd: expected.get("hwnd").and_then(Value::as_u64).unwrap_or(0),
            pid: expected.get("pid").and_then(Value::as_u64).unwrap_or(0),
            generation: expected
                .get("generation")
                .and_then(Value::as_u64)
                .unwrap_or(0),
        };
        if tab_id <= 0
            || document_id.is_empty()
            || window.browser_window_id <= 0
            || window.hwnd == 0
            || window.pid == 0
            || window.generation == 0
        {
            return Err(anyhow!("invalid expected browser input target identity"));
        }
        return Ok(Some(ExpectedInputTarget::Browser {
            tab_id,
            document_id: document_id.to_string(),
            window,
        }));
    }
    let hwnd = expected.get("hwnd").and_then(Value::as_u64).unwrap_or(0);
    let pid = expected.get("pid").and_then(Value::as_u64).unwrap_or(0);
    let generation = expected
        .get("generation")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if hwnd == 0 || pid == 0 || generation == 0 {
        return Err(anyhow!("invalid expected input target identity"));
    }
    Ok(Some(ExpectedInputTarget::Native {
        hwnd,
        pid,
        generation,
        element: expected
            .get("element")
            .map(expected_native_element)
            .transpose()?,
    }))
}

fn expected_native_element(value: &Value) -> Result<ExpectedNativeElement> {
    let point = value
        .get("screen_px")
        .and_then(Value::as_array)
        .filter(|point| point.len() == 2)
        .ok_or_else(|| anyhow!("invalid expected native element point"))?;
    let x = point[0]
        .as_i64()
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(|| anyhow!("invalid expected native element x coordinate"))?;
    let y = point[1]
        .as_i64()
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(|| anyhow!("invalid expected native element y coordinate"))?;
    let role = required_string(value, "role")?;
    let provider_name = string_field(value, "provider_name")?;
    let automation_id = string_field(value, "automation_id")?;
    let runtime_id = value
        .get("runtime_id")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("expected native element runtime_id is missing"))?
        .iter()
        .map(|item| {
            item.as_i64()
                .and_then(|value| i32::try_from(value).ok())
                .ok_or_else(|| anyhow!("expected native element runtime_id is invalid"))
        })
        .collect::<Result<Vec<_>>>()?;
    if runtime_id.is_empty() || runtime_id.len() > 128 {
        return Err(anyhow!(
            "expected native element runtime_id has an invalid length"
        ));
    }
    Ok(ExpectedNativeElement {
        x,
        y,
        role,
        provider_name,
        automation_id,
        runtime_id,
    })
}

fn required_string(value: &Value, field: &str) -> Result<String> {
    let text = string_field(value, field)?;
    if text.trim().is_empty() {
        return Err(anyhow!("expected native element {field} is empty"));
    }
    Ok(text)
}

fn string_field(value: &Value, field: &str) -> Result<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| anyhow!("expected native element {field} is missing"))
}

pub(super) fn verify_window(args: &Value) -> Result<()> {
    let Some(expected) = expected_input_target(args)? else {
        return Ok(());
    };
    let (expected_hwnd, expected_pid, expected_generation) = match expected {
        ExpectedInputTarget::Browser {
            tab_id,
            document_id,
            window,
        } => {
            return super::super::browser::validate_active_document_identity(
                tab_id,
                &document_id,
                &window,
            );
        }
        ExpectedInputTarget::Native {
            hwnd,
            pid,
            generation,
            ..
        } => (hwnd, pid, generation),
    };
    super::super::uia::validate_native_provider_ownership()?;
    super::super::uia::validate_native_identity(expected_hwnd, expected_pid, expected_generation)?;
    let actual = super::super::uia::input_target_snapshot();
    let actual_hwnd = actual.get("hwnd").and_then(Value::as_u64).unwrap_or(0);
    let actual_pid = actual.get("pid").and_then(Value::as_u64).unwrap_or(0);
    let actual_generation = actual
        .get("generation")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if actual_hwnd != expected_hwnd
        || actual_pid != expected_pid
        || actual_generation != expected_generation
    {
        return Err(anyhow!(
            "input target changed before injection; expected observed window generation {expected_hwnd}/{expected_pid}/{expected_generation}, got {actual_hwnd}/{actual_pid}/{actual_generation}"
        ));
    }
    Ok(())
}

pub(super) fn verify_pointer(args: &Value) -> Result<()> {
    verify_window(args)?;
    let Some(ExpectedInputTarget::Native {
        element: Some(element),
        ..
    }) = expected_input_target(args)?
    else {
        return Ok(());
    };
    super::super::uia::validate_native_element_at(
        element.x,
        element.y,
        super::super::uia::ExpectedNativeElement {
            role: &element.role,
            provider_name: &element.provider_name,
            automation_id: &element.automation_id,
            runtime_id: &element.runtime_id,
        },
    )
}

pub(super) fn verify_keyboard(args: &Value) -> Result<()> {
    verify_window(args)?;
    let Some(ExpectedInputTarget::Native {
        element: Some(element),
        ..
    }) = expected_input_target(args)?
    else {
        return Ok(());
    };
    super::super::uia::validate_native_focused_element(super::super::uia::ExpectedNativeElement {
        role: &element.role,
        provider_name: &element.provider_name,
        automation_id: &element.automation_id,
        runtime_id: &element.runtime_id,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn browser_window() -> BrowserWindowIdentity {
        BrowserWindowIdentity {
            browser_window_id: 7,
            hwnd: 8,
            pid: 9,
            generation: 10,
        }
    }

    #[test]
    fn native_target_requires_window_generation_and_exact_element_fields() {
        assert_eq!(expected_input_target(&json!({})).unwrap(), None);
        assert!(
            expected_input_target(&json!({
                "expected_input_target": {"hwnd": 41, "pid": 73}
            }))
            .is_err()
        );

        let parsed = expected_input_target(&json!({
            "expected_input_target": {
                "hwnd": 41,
                "pid": 73,
                "generation": 5,
                "element": {
                    "screen_px": [12, 34],
                    "role": "link",
                    "provider_name": "entry",
                    "automation_id": "node-4",
                    "runtime_id": [42, 4]
                }
            }
        }))
        .unwrap();
        assert_eq!(
            parsed,
            Some(ExpectedInputTarget::Native {
                hwnd: 41,
                pid: 73,
                generation: 5,
                element: Some(ExpectedNativeElement {
                    x: 12,
                    y: 34,
                    role: "link".into(),
                    provider_name: "entry".into(),
                    automation_id: "node-4".into(),
                    runtime_id: vec![42, 4],
                }),
            })
        );
    }

    #[test]
    fn browser_target_requires_exact_document_identity() {
        assert_eq!(
            expected_input_target(&json!({
                "expected_input_target": {
                    "kind": "browser", "tab_id": 91, "document_id": "doc-4",
                    "browser_window_id": 7, "hwnd": 8, "pid": 9, "generation": 10
                }
            }))
            .unwrap(),
            Some(ExpectedInputTarget::Browser {
                tab_id: 91,
                document_id: "doc-4".into(),
                window: browser_window(),
            })
        );
        assert!(
            expected_input_target(&json!({
                "expected_input_target": {"kind": "browser", "tab_id": 91}
            }))
            .is_err()
        );
    }

    #[test]
    fn partial_element_identity_is_rejected() {
        assert!(
            expected_input_target(&json!({
                "expected_input_target": {
                    "hwnd": 41,
                    "pid": 73,
                    "generation": 5,
                "element": {"screen_px": [1, 2], "role": "link", "runtime_id": []}
                }
            }))
            .is_err()
        );
    }

    #[test]
    fn empty_runtime_identity_is_rejected() {
        assert!(
            expected_input_target(&json!({
                "expected_input_target": {
                    "hwnd": 41,
                    "pid": 73,
                    "generation": 5,
                    "element": {
                        "screen_px": [1, 2],
                        "role": "link",
                        "provider_name": "entry",
                        "automation_id": "entry-1",
                        "runtime_id": []
                    }
                }
            }))
            .is_err()
        );
    }
}
