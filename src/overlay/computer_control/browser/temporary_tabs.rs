//! Recoverable, identity-bound browser tab leases and verified cleanup.

use anyhow::{Context, Result};
use serde_json::json;
use std::fmt;
use std::time::{Duration, Instant};

const CLEANUP_TOTAL: Duration = Duration::from_secs(2);
const VERIFY_POLL: Duration = Duration::from_millis(50);

mod inventory;
#[cfg(test)]
mod test_support;
use inventory::*;

#[derive(Clone)]
pub(in crate::overlay::computer_control) struct TemporaryBrowserTab {
    pub id: i64,
    pub foreground: bool,
    pub recovered_create: bool,
    epoch: u64,
    window_id: Option<i64>,
    requested_url: String,
    navigation_allowed: bool,
    restore_allowed: bool,
    restore: Option<RestoreTarget>,
}

pub(in crate::overlay::computer_control) struct TemporaryTabCleanup {
    pub closed_verified: bool,
    pub preserved: bool,
    pub preservation_reason: Option<String>,
    pub restoration_required: bool,
    pub restored: bool,
    pub close_error: Option<String>,
    pub restore_error: Option<String>,
}

#[derive(Debug)]
struct OpenError {
    message: String,
    effect_ambiguous: bool,
}

enum DispatchedCreate {
    Reply(Option<i64>),
    Ambiguous(anyhow::Error),
}

#[derive(Clone, Copy)]
struct LeasePolicy {
    foreground: bool,
    navigation_allowed: bool,
    cleanup_required: bool,
}

impl fmt::Display for OpenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for OpenError {}

pub(in crate::overlay::computer_control) fn open_temporary_tab(
    url: &str,
) -> Result<TemporaryBrowserTab> {
    open_tab_lease(url, true, true, true)
}

pub(in crate::overlay::computer_control) fn open_turn_owned_tab(
    url: &str,
) -> Result<TemporaryBrowserTab> {
    open_tab_lease(url, true, false, true)
}

pub(in crate::overlay::computer_control) fn open_persistent_tab(
    url: &str,
) -> Result<TemporaryBrowserTab> {
    open_tab_lease(url, true, false, false)
}

fn open_tab_lease(
    url: &str,
    navigation_allowed: bool,
    prefer_background: bool,
    cleanup_required: bool,
) -> Result<TemporaryBrowserTab> {
    super::capabilities::require(super::capabilities::TABS_LIST)?;
    if cleanup_required {
        require_cleanup_capability()?;
    }
    let Some(foreground) = preferred_tab_foreground(
        super::capabilities::supports(super::capabilities::TABS_CREATE_BACKGROUND),
        super::capabilities::supports(super::capabilities::TABS_CREATE_FOREGROUND),
        prefer_background,
    ) else {
        return Err(super::capabilities::unsupported(if prefer_background {
            super::capabilities::TABS_CREATE_BACKGROUND
        } else {
            super::capabilities::TABS_CREATE_FOREGROUND
        }));
    };
    if cleanup_required && foreground {
        super::capabilities::require(super::capabilities::TABS_ACTIVATE)?;
    }
    let policy = LeasePolicy {
        foreground,
        navigation_allowed,
        cleanup_required,
    };
    let requested_url = canonical_url(url)
        .ok_or_else(|| anyhow::anyhow!("browser tab URL is not an absolute URL"))?;
    let epoch = super::bridge::connection_epoch();
    let window_identity_available =
        super::capabilities::supports(super::capabilities::TABS_INVENTORY_WINDOW_IDENTITY);
    if policy.cleanup_required && policy.foreground && !window_identity_available {
        return Err(super::capabilities::unsupported(
            super::capabilities::TABS_INVENTORY_WINDOW_IDENTITY,
        ));
    }
    let before = list_tabs(epoch).context("temporary tab pre-create inventory failed")?;
    require_foreground_precreate_identity(policy, window_identity_available, &before)?;

    let outcome = match super::bridge_rpc::rpc_on_epoch(
        "tabs",
        json!({"action": "create", "url": url, "active": foreground}),
        epoch,
    ) {
        Ok(reply) => DispatchedCreate::Reply(valid_created_tab_id(&reply)),
        Err(error) if create_was_not_dispatched(&error) => return Err(error),
        Err(error) => {
            let deadline = Instant::now() + CLEANUP_TOTAL;
            return reconcile_dispatched_create(
                requested_url,
                epoch,
                policy,
                &before,
                DispatchedCreate::Ambiguous(error),
                deadline,
            );
        }
    };
    let deadline = Instant::now() + CLEANUP_TOTAL;
    reconcile_dispatched_create(requested_url, epoch, policy, &before, outcome, deadline)
}

fn reconcile_dispatched_create(
    requested_url: String,
    epoch: u64,
    policy: LeasePolicy,
    before: &[TabSnapshot],
    outcome: DispatchedCreate,
    deadline: Instant,
) -> Result<TemporaryBrowserTab> {
    let (returned_id, dispatch_error) = match outcome {
        DispatchedCreate::Reply(id) => (id, None),
        DispatchedCreate::Ambiguous(error) => (None, Some(error)),
    };
    let after = list_tabs_cleanup(epoch, deadline).map_err(|error| {
        open_error(
            true,
            format_dispatch_failure(dispatch_error.as_ref(), &error),
        )
    })?;
    let (created, recovered_create) =
        select_created_tab(before, &after, returned_id, &requested_url, true).ok_or_else(|| {
            open_error(
                true,
                format!(
                    "browser tab ownership could not be reconciled after dispatch{}",
                    dispatch_error
                        .as_ref()
                        .map(|error| format!(": {error}"))
                        .unwrap_or_default()
                ),
            )
        })?;
    if !policy.foreground && created.active {
        return Err(open_error(
            true,
            "background browser tab became active; preserving possible user takeover",
        ));
    }
    require_foreground_created_identity(policy, created)?;
    let restore_allowed = policy.cleanup_required && policy.foreground && created.active;
    let restore = restore_allowed
        .then(|| {
            created
                .window_id
                .and_then(|window_id| active_in_window(before, window_id))
        })
        .flatten();
    Ok(TemporaryBrowserTab {
        id: created.id,
        foreground: policy.foreground,
        recovered_create,
        epoch,
        window_id: created.window_id,
        requested_url,
        navigation_allowed: policy.navigation_allowed,
        restore_allowed,
        restore,
    })
}

pub(in crate::overlay::computer_control) fn open_effect_ambiguous(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<OpenError>()
        .is_some_and(|error| error.effect_ambiguous)
}

pub(in crate::overlay::computer_control) fn close_tab_verified(
    tab: &TemporaryBrowserTab,
) -> TemporaryTabCleanup {
    cleanup_with_deadline(tab, Instant::now() + CLEANUP_TOTAL, false)
}

pub(in crate::overlay::computer_control) fn close_tab_verified_until(
    tab: &TemporaryBrowserTab,
    deadline: Instant,
    preserve_active: bool,
) -> TemporaryTabCleanup {
    cleanup_with_deadline(tab, deadline, preserve_active)
}

fn cleanup_with_deadline(
    tab: &TemporaryBrowserTab,
    deadline: Instant,
    preserve_active: bool,
) -> TemporaryTabCleanup {
    if super::bridge::connection_epoch() != tab.epoch {
        return preserved_result("connection_epoch_changed");
    }
    let inventory = match list_tabs_cleanup(tab.epoch, deadline) {
        Ok(inventory) => inventory,
        Err(error) => return close_failed(error),
    };
    let Some(current) = inventory.iter().find(|current| current.id == tab.id) else {
        return cleanup_result(true, false, None, false, false, None, None);
    };
    if let Some(reason) = identity_conflict(tab, current, super::bridge::connection_epoch()) {
        return preserved_result(reason);
    }
    let policy = close_policy(tab, current, preserve_active);
    if policy == ClosePolicy::UserTakeover {
        return preserved_result("active_user_takeover");
    }
    let restoration_required = policy == ClosePolicy::CloseAndRestore;
    let restore_precheck = if restoration_required {
        validate_restore_target(&inventory, tab)
    } else {
        Ok(None)
    };
    if let Err(error) = close_owned_tab(tab, deadline) {
        return cleanup_result(
            false,
            false,
            None,
            restoration_required,
            false,
            Some(error.to_string()),
            None,
        );
    }
    if let Err(error) = wait_until_absent(tab, deadline) {
        return cleanup_result(
            false,
            false,
            None,
            restoration_required,
            false,
            Some(error.to_string()),
            None,
        );
    }
    if !restoration_required {
        return cleanup_result(true, false, None, false, false, None, None);
    }
    let target = match restore_precheck {
        Ok(Some(target)) => target,
        Ok(None) => {
            return cleanup_result(
                true,
                false,
                None,
                true,
                false,
                None,
                Some("temporary tab had no same-window restore target".to_string()),
            );
        }
        Err(error) => {
            return cleanup_result(
                true,
                false,
                None,
                true,
                false,
                None,
                Some(error.to_string()),
            );
        }
    };
    match restore_foreground(tab, target, deadline) {
        Ok(()) => cleanup_result(true, false, None, true, true, None, None),
        Err(error) => cleanup_result(
            true,
            false,
            None,
            true,
            false,
            None,
            Some(error.to_string()),
        ),
    }
}

fn create_was_not_dispatched(error: &anyhow::Error) -> bool {
    super::capabilities::unsupported_from(error).is_some()
        || super::bridge_wait::dispatch_effect(error) == Some(false)
}

fn identity_conflict(
    tab: &TemporaryBrowserTab,
    current: &TabSnapshot,
    current_epoch: u64,
) -> Option<&'static str> {
    if current_epoch != tab.epoch {
        return Some("connection_epoch_changed");
    }
    if tab
        .window_id
        .is_some_and(|window_id| current.window_id != Some(window_id))
    {
        return Some("browser_window_changed");
    }
    if !tab.navigation_allowed && !tab_matches_requested_url(current, &tab.requested_url) {
        return Some("document_identity_changed");
    }
    None
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ClosePolicy {
    Close,
    CloseAndRestore,
    UserTakeover,
}

fn close_policy(
    tab: &TemporaryBrowserTab,
    current: &TabSnapshot,
    preserve_active: bool,
) -> ClosePolicy {
    if current.active && (preserve_active || !tab.restore_allowed) {
        ClosePolicy::UserTakeover
    } else if current.active {
        ClosePolicy::CloseAndRestore
    } else {
        ClosePolicy::Close
    }
}

fn validate_restore_target(
    inventory: &[TabSnapshot],
    tab: &TemporaryBrowserTab,
) -> Result<Option<RestoreTarget>> {
    let Some(target) = tab.restore else {
        return Ok(None);
    };
    let current = inventory
        .iter()
        .find(|current| current.id == target.id)
        .ok_or_else(|| anyhow::anyhow!("temporary tab restore target is no longer available"))?;
    if current.window_id != Some(target.window_id) || tab.window_id != Some(target.window_id) {
        anyhow::bail!("temporary tab restore target moved to another browser window");
    }
    Ok(Some(target))
}

fn close_owned_tab(tab: &TemporaryBrowserTab, deadline: Instant) -> Result<()> {
    if super::capabilities::supports(super::capabilities::TABS_REMOVE) {
        super::bridge_rpc::rpc_cleanup_until(
            "tabs",
            json!({"action": "remove", "tabId": tab.id}),
            tab.epoch,
            deadline,
        )?;
    } else {
        super::bridge_cleanup::cdp_on_tab_cleanup_until(
            "Page.close",
            json!({}),
            tab.id,
            tab.epoch,
            deadline,
        )?;
    }
    Ok(())
}

fn wait_until_absent(tab: &TemporaryBrowserTab, deadline: Instant) -> Result<()> {
    loop {
        let inventory = list_tabs_cleanup(tab.epoch, deadline)?;
        if !inventory.iter().any(|current| current.id == tab.id) {
            return Ok(());
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            anyhow::bail!("temporary tab remained after the cleanup deadline");
        }
        std::thread::sleep(VERIFY_POLL.min(remaining));
    }
}

fn restore_foreground(
    tab: &TemporaryBrowserTab,
    target: RestoreTarget,
    deadline: Instant,
) -> Result<()> {
    let inventory = list_tabs_cleanup(tab.epoch, deadline)?;
    let current = inventory
        .iter()
        .find(|current| current.id == target.id)
        .ok_or_else(|| anyhow::anyhow!("temporary tab restore target disappeared"))?;
    if current.window_id != Some(target.window_id) || tab.window_id != Some(target.window_id) {
        anyhow::bail!("temporary tab restore target changed browser windows");
    }
    if !current.active {
        super::bridge_rpc::rpc_cleanup_until(
            "tabs",
            json!({"action": "activate", "tabId": target.id}),
            tab.epoch,
            deadline,
        )?;
    }
    let inventory = list_tabs_cleanup(tab.epoch, deadline)?;
    if inventory.iter().any(|current| {
        current.id == target.id
            && current.window_id == Some(target.window_id)
            && tab.window_id == Some(target.window_id)
            && current.active
    }) {
        Ok(())
    } else {
        anyhow::bail!("temporary tab foreground restoration was not verified")
    }
}

fn require_cleanup_capability() -> Result<()> {
    if super::capabilities::supports(super::capabilities::TABS_REMOVE) {
        return Ok(());
    }
    super::capabilities::require(super::capabilities::CDP)?;
    super::capabilities::require(super::capabilities::CDP_EXPLICIT_TAB)
}

#[cfg(test)]
fn cleanup_capability_available(remove: bool, cdp: bool, explicit_tab: bool) -> bool {
    remove || (cdp && explicit_tab)
}

fn list_tabs(epoch: u64) -> Result<Vec<TabSnapshot>> {
    let value = super::bridge_rpc::rpc_on_epoch("tabs", json!({"action": "list"}), epoch)?;
    parse_tab_inventory(&value)
}

fn list_tabs_cleanup(epoch: u64, deadline: Instant) -> Result<Vec<TabSnapshot>> {
    let value =
        super::bridge_rpc::rpc_cleanup_until("tabs", json!({"action": "list"}), epoch, deadline)?;
    parse_tab_inventory(&value)
}

fn preferred_tab_foreground(
    background: bool,
    foreground: bool,
    prefer_background: bool,
) -> Option<bool> {
    if prefer_background {
        background.then_some(false).or(foreground.then_some(true))
    } else {
        foreground.then_some(true).or(background.then_some(false))
    }
}

fn require_foreground_precreate_identity(
    policy: LeasePolicy,
    identity_capability: bool,
    inventory: &[TabSnapshot],
) -> Result<()> {
    if !policy.cleanup_required || !policy.foreground {
        return Ok(());
    }
    if !identity_capability {
        return Err(super::capabilities::unsupported(
            super::capabilities::TABS_INVENTORY_WINDOW_IDENTITY,
        ));
    }
    if let Some(tab) = inventory.iter().find(|tab| tab.window_id.is_none()) {
        anyhow::bail!(
            "browser advertised tab window identity but pre-create tab {} omitted windowId",
            tab.id
        );
    }
    Ok(())
}

fn require_foreground_created_identity(policy: LeasePolicy, created: &TabSnapshot) -> Result<()> {
    if policy.cleanup_required && policy.foreground && created.window_id.is_none() {
        return Err(open_error(
            true,
            format!(
                "browser advertised tab window identity but created tab {} omitted windowId",
                created.id
            ),
        ));
    }
    Ok(())
}

fn open_error(ambiguous: bool, message: impl Into<String>) -> anyhow::Error {
    OpenError {
        message: message.into(),
        effect_ambiguous: ambiguous,
    }
    .into()
}

fn format_dispatch_failure(dispatch: Option<&anyhow::Error>, reconcile: &anyhow::Error) -> String {
    dispatch.map_or_else(
        || format!("temporary tab post-create inventory failed: {reconcile}"),
        |dispatch| format!("temporary tab create failed after dispatch ({dispatch}); reconciliation failed: {reconcile}"),
    )
}

fn close_failed(error: anyhow::Error) -> TemporaryTabCleanup {
    cleanup_result(
        false,
        false,
        None,
        false,
        false,
        Some(error.to_string()),
        None,
    )
}

fn preserved_result(reason: &str) -> TemporaryTabCleanup {
    cleanup_result(
        false,
        true,
        Some(reason.to_string()),
        false,
        false,
        None,
        None,
    )
}

fn cleanup_result(
    closed_verified: bool,
    preserved: bool,
    preservation_reason: Option<String>,
    restoration_required: bool,
    restored: bool,
    close_error: Option<String>,
    restore_error: Option<String>,
) -> TemporaryTabCleanup {
    TemporaryTabCleanup {
        closed_verified,
        preserved,
        preservation_reason,
        restoration_required,
        restored,
        close_error,
        restore_error,
    }
}

#[cfg(test)]
#[path = "temporary_tabs_tests.rs"]
mod tests;
