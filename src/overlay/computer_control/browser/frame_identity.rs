//! Exact browser document identities used to bind model-visible frames to actions.

use anyhow::{Result, anyhow};
use serde_json::Value;

use super::super::controller::world::BrowserWindowIdentity;

fn document_id(value: Value) -> Result<String> {
    value
        .as_str()
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .ok_or_else(|| anyhow!("browser document identity is unavailable"))
}

pub(in crate::overlay::computer_control) fn active_document_identity()
-> Result<(i64, String, BrowserWindowIdentity)> {
    let (tab_id, window) = super::surface_binding::active()?;
    let value = super::controller_io::eval_value_in_active_tab(
        super::controller_io::DOCUMENT_ID_JS,
        tab_id,
        None,
    )?;
    let document_id = document_id(value)?;
    super::surface_binding::validate(tab_id, &window)?;
    Ok((tab_id, document_id, window))
}

pub(in crate::overlay::computer_control) fn document_identity_on_tab(
    tab_id: i64,
) -> Result<String> {
    document_id(super::eval_value_on_tab(
        super::controller_io::DOCUMENT_ID_JS,
        tab_id,
    )?)
}

pub(in crate::overlay::computer_control) fn validate_active_document_identity(
    tab_id: i64,
    expected_document_id: &str,
    expected_window: &BrowserWindowIdentity,
) -> Result<()> {
    let actual = active_document_identity()?;
    if actual.0 == tab_id && actual.1 == expected_document_id && actual.2 == *expected_window {
        Ok(())
    } else {
        anyhow::bail!(
            "active browser surface changed; expected tab/document/window {tab_id}/{expected_document_id}/{expected_window:?}, got {}/{}/{:?}",
            actual.0,
            actual.1,
            actual.2
        )
    }
}

pub(in crate::overlay::computer_control) fn validate_document_identity_on_tab(
    tab_id: i64,
    expected_document_id: &str,
) -> Result<()> {
    let actual = document_identity_on_tab(tab_id)?;
    if actual == expected_document_id {
        Ok(())
    } else {
        anyhow::bail!(
            "browser document changed; expected {expected_document_id}, got {actual} on tab {tab_id}"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_identity_requires_a_nonempty_string() {
        assert_eq!(document_id(Value::String("doc-1".into())).unwrap(), "doc-1");
        assert!(document_id(Value::String(String::new())).is_err());
        assert!(document_id(Value::Null).is_err());
    }
}
