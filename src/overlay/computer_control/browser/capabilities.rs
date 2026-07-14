//! Connection-scoped browser-extension capability negotiation.
//!
//! Older protocols' exact shipped feature sets are mapped here. Newer extensions advertise every supported
//! operation and no capability is inferred from a newer version number.

use std::collections::BTreeSet;
use std::fmt;
use std::sync::{OnceLock, RwLock};

use serde_json::Value;

pub(super) const CURRENT_PROTOCOL: u64 = 5;
pub(super) const CDP: &str = "cdp.command";
pub(super) const CDP_EXPLICIT_TAB: &str = "cdp.explicit_tab";
pub(super) const CDP_SESSION: &str = "cdp.session";
pub(super) const CDP_REQUIRE_ACTIVE: &str = "cdp.require_active";
pub(super) const TABS_LIST: &str = "tabs.list";
pub(super) const TABS_ACTIVE: &str = "tabs.active";
pub(super) const TABS_ACTIVE_FOCUSED_WINDOW: &str = "tabs.active.focused_window";
pub(super) const TABS_ACTIVATE: &str = "tabs.activate";
pub(super) const TABS_NAVIGATE: &str = "tabs.navigate";
pub(super) const TABS_CREATE_FOREGROUND: &str = "tabs.create.foreground";
pub(super) const TABS_CREATE_BACKGROUND: &str = "tabs.create.background";
pub(super) const TABS_REMOVE: &str = "tabs.remove";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Negotiated {
    protocol: u64,
    names: BTreeSet<String>,
}

fn state() -> &'static RwLock<Negotiated> {
    static STATE: OnceLock<RwLock<Negotiated>> = OnceLock::new();
    STATE.get_or_init(|| RwLock::new(Negotiated::default()))
}

pub(super) fn negotiate(protocol: u64, advertised: Option<&Value>) {
    *state().write().unwrap() = negotiated(protocol, advertised);
}

pub(super) fn reset() {
    *state().write().unwrap() = Negotiated::default();
}

pub(super) fn protocol_version() -> u64 {
    state().read().unwrap().protocol
}

pub(super) fn list() -> Vec<String> {
    state().read().unwrap().names.iter().cloned().collect()
}

pub(super) fn supports(name: &str) -> bool {
    state().read().unwrap().names.contains(name)
}

pub(super) fn usable() -> bool {
    supports(CDP)
}

pub(super) fn update_staged() -> bool {
    let protocol = protocol_version();
    protocol > 0 && protocol < CURRENT_PROTOCOL
}

pub(super) fn require(name: impl Into<String>) -> anyhow::Result<()> {
    let name = name.into();
    if supports(&name) {
        Ok(())
    } else {
        Err(UnsupportedCapability { name }.into())
    }
}

pub(super) fn unsupported(name: impl Into<String>) -> anyhow::Error {
    UnsupportedCapability { name: name.into() }.into()
}

pub(super) fn unsupported_from(error: &anyhow::Error) -> Option<&str> {
    error
        .downcast_ref::<UnsupportedCapability>()
        .map(|unsupported| unsupported.name.as_str())
}

#[derive(Debug)]
struct UnsupportedCapability {
    name: String,
}

impl fmt::Display for UnsupportedCapability {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "connected browser extension does not support capability '{}'",
            self.name
        )
    }
}

impl std::error::Error for UnsupportedCapability {}

fn negotiated(protocol: u64, advertised: Option<&Value>) -> Negotiated {
    let names = if protocol >= CURRENT_PROTOCOL {
        advertised
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .collect()
    } else {
        legacy_names(protocol)
    };
    Negotiated { protocol, names }
}

fn legacy_names(protocol: u64) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    if protocol >= 1 {
        names.extend(
            [
                CDP,
                CDP_EXPLICIT_TAB,
                CDP_SESSION,
                TABS_LIST,
                TABS_ACTIVATE,
                TABS_CREATE_FOREGROUND,
            ]
            .into_iter()
            .map(str::to_string),
        );
    }
    if protocol >= 2 {
        names.extend(
            [TABS_CREATE_BACKGROUND, TABS_REMOVE]
                .into_iter()
                .map(str::to_string),
        );
    }
    if protocol >= 3 {
        names.extend(
            [CDP_REQUIRE_ACTIVE, TABS_ACTIVE]
                .into_iter()
                .map(str::to_string),
        );
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn legacy_protocols_map_only_their_shipped_features() {
        let one = negotiated(1, None);
        assert!(one.names.contains(CDP_EXPLICIT_TAB));
        assert!(one.names.contains(TABS_CREATE_FOREGROUND));
        assert!(!one.names.contains(TABS_CREATE_BACKGROUND));
        assert!(!one.names.contains(TABS_REMOVE));
        assert!(!one.names.contains(CDP_REQUIRE_ACTIVE));

        let two = negotiated(2, None);
        assert!(two.names.contains(TABS_CREATE_BACKGROUND));
        assert!(two.names.contains(TABS_REMOVE));
        assert!(!two.names.contains(TABS_ACTIVE));
        assert!(!two.names.contains(CDP_REQUIRE_ACTIVE));

        let three = negotiated(3, None);
        assert!(three.names.contains(TABS_ACTIVE));
        assert!(three.names.contains(CDP_REQUIRE_ACTIVE));
        assert!(!three.names.contains(TABS_ACTIVE_FOCUSED_WINDOW));

        let four = negotiated(4, None);
        assert!(!four.names.contains(TABS_ACTIVE_FOCUSED_WINDOW));
        assert!(!four.names.contains(TABS_NAVIGATE));
    }

    #[test]
    fn current_protocol_uses_only_advertised_capabilities() {
        let advertised = json!([CDP, TABS_ACTIVE, "", 7]);
        let current = negotiated(CURRENT_PROTOCOL, Some(&advertised));
        assert_eq!(
            current.names,
            BTreeSet::from([CDP.to_string(), TABS_ACTIVE.to_string()])
        );

        assert!(
            negotiated(CURRENT_PROTOCOL, None).names.is_empty(),
            "missing negotiation must fail closed"
        );
    }

    #[test]
    fn capability_errors_remain_typed_through_anyhow() {
        let error = unsupported("future.operation");
        assert_eq!(unsupported_from(&error), Some("future.operation"));
    }
}
