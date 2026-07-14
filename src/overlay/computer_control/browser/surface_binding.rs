//! Exact ownership of the foreground OS window by the paired browser extension.
//!
//! Chromium's top-level widget class is shared by browsers and unrelated desktop
//! shells, so it is never ownership evidence. The extension proves that its exact
//! active browser window is OS-focused; Rust binds that to the foreground HWND,
//! PID, and generation and revalidates the whole tuple before using browser input.

use anyhow::{Result, anyhow};
use serde_json::{Value, json};
use std::sync::{Mutex, OnceLock};

use super::super::controller::world::BrowserWindowIdentity;

#[derive(Debug, PartialEq, Eq)]
struct ExtensionSurface<'a> {
    tab_id: i64,
    browser_window_id: i64,
    window_focused: bool,
    title: &'a str,
}

#[derive(Debug, PartialEq, Eq)]
struct ForegroundSurface<'a> {
    hwnd: u64,
    pid: u64,
    generation: u64,
    title: &'a str,
    window_class: &'a str,
}

fn bind_surface(
    extension: &ExtensionSurface<'_>,
    foreground: &ForegroundSurface<'_>,
) -> Option<BrowserWindowIdentity> {
    let _non_ownership_metadata = foreground.window_class;
    if extension.tab_id <= 0
        || extension.browser_window_id <= 0
        || !extension.window_focused
        || foreground.hwnd == 0
        || foreground.pid == 0
        || foreground.generation == 0
        || !super::controller_io::title_matches_window(extension.title, foreground.title)
    {
        return None;
    }
    Some(BrowserWindowIdentity {
        browser_window_id: extension.browser_window_id,
        hwnd: foreground.hwnd,
        pid: foreground.pid,
        generation: foreground.generation,
    })
}

fn required_u64(value: &Value, field: &str) -> Result<u64> {
    value
        .get(field)
        .and_then(Value::as_u64)
        .filter(|value| *value != 0)
        .ok_or_else(|| anyhow!("active browser surface omitted {field}"))
}

fn required_i64(value: &Value, field: &str) -> Result<i64> {
    value
        .get(field)
        .and_then(Value::as_i64)
        .filter(|value| *value > 0)
        .ok_or_else(|| anyhow!("active browser surface omitted {field}"))
}

pub(super) fn active() -> Result<(i64, BrowserWindowIdentity)> {
    let result = resolve_active();
    record_binding_outcome(&result);
    result
}

fn resolve_active() -> Result<(i64, BrowserWindowIdentity)> {
    super::capabilities::require(super::capabilities::TABS_ACTIVE_FOCUSED_WINDOW)?;
    let observed = super::super::uia::observe_native_identity(None)?;
    let foreground = super::super::uia::input_target_snapshot();
    let foreground_surface = ForegroundSurface {
        hwnd: required_u64(&foreground, "hwnd")?,
        pid: required_u64(&foreground, "pid")?,
        generation: required_u64(&foreground, "generation")?,
        title: foreground
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or(""),
        window_class: foreground
            .get("class")
            .and_then(Value::as_str)
            .unwrap_or(""),
    };
    if observed
        != (
            foreground_surface.hwnd,
            foreground_surface.pid,
            foreground_surface.generation,
        )
    {
        anyhow::bail!("foreground window changed while browser ownership was observed");
    }

    let extension = super::bridge::rpc("tabs", json!({"action": "active"}))?;
    let extension_surface = ExtensionSurface {
        tab_id: required_i64(&extension, "id")?,
        browser_window_id: required_i64(&extension, "windowId")?,
        window_focused: extension.get("windowFocused").and_then(Value::as_bool) == Some(true),
        title: extension.get("title").and_then(Value::as_str).unwrap_or(""),
    };
    let binding = bind_surface(&extension_surface, &foreground_surface)
        .ok_or_else(|| anyhow!("active extension tab does not own the foreground OS window"))?;
    if super::super::uia::current_native_identity(None)? != observed {
        anyhow::bail!("foreground window changed while browser ownership was confirmed");
    }
    Ok((extension_surface.tab_id, binding))
}

fn record_binding_outcome(result: &Result<(i64, BrowserWindowIdentity)>) {
    static LAST_FAILURE: OnceLock<Mutex<Option<(String, String)>>> = OnceLock::new();
    let mut last = LAST_FAILURE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    match result {
        Ok((tab_id, binding)) if last.take().is_some() => {
            super::super::telemetry::event(
                "browser_surface_binding_recovered",
                "browser_surface",
                super::super::telemetry::Privacy::Safe,
                json!({"tab_id": tab_id, "hwnd": binding.hwnd, "pid": binding.pid}),
            );
        }
        Ok(_) => {}
        Err(error) => {
            let session_id = super::super::telemetry::session_id();
            let reason = error.to_string();
            if last.as_ref() != Some(&(session_id.clone(), reason.clone())) {
                super::super::telemetry::event(
                    "browser_surface_binding_unavailable",
                    "browser_surface",
                    super::super::telemetry::Privacy::Sensitive,
                    json!({"reason": reason}),
                );
                *last = Some((session_id, reason));
            }
        }
    }
}

pub(super) fn validate(tab_id: i64, expected: &BrowserWindowIdentity) -> Result<()> {
    let (actual_tab_id, actual_window) = active()?;
    if actual_tab_id == tab_id && actual_window == *expected {
        Ok(())
    } else {
        anyhow::bail!(
            "active browser window changed; expected tab/window {tab_id}/{expected:?}, got {actual_tab_id}/{actual_window:?}"
        )
    }
}

pub(super) fn owns_foreground_window(hwnd: u64) -> bool {
    super::is_connected()
        && active().is_ok_and(|(_, binding)| binding.hwnd == hwnd && binding.hwnd != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extension(focused: bool) -> ExtensionSurface<'static> {
        ExtensionSurface {
            tab_id: 17,
            browser_window_id: 23,
            window_focused: focused,
            title: "active document",
        }
    }

    fn foreground(pid: u64, window_class: &'static str) -> ForegroundSurface<'static> {
        ForegroundSurface {
            hwnd: 31,
            pid,
            generation: 7,
            title: "active document - Browser Shell",
            window_class,
        }
    }

    #[test]
    fn unfocused_extension_window_never_owns_the_foreground() {
        assert_eq!(
            bind_surface(&extension(false), &foreground(42, "SharedWidgetClass")),
            None
        );
    }

    #[test]
    fn exact_focused_window_binding_does_not_depend_on_widget_class() {
        let first = bind_surface(&extension(true), &foreground(41, "SharedWidgetClass"));
        let second = bind_surface(&extension(true), &foreground(41, "AnotherWidgetClass"));
        assert_eq!(first, second);
        assert_eq!(first.unwrap().hwnd, 31);
    }

    #[test]
    fn focused_window_with_a_different_visible_document_is_not_owned() {
        let mut other = foreground(41, "SharedWidgetClass");
        other.title = "unrelated document - Desktop Shell";
        assert_eq!(bind_surface(&extension(true), &other), None);
    }
}
