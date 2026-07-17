//! Strict tab inventories and create reconciliation for browser tab leases.

use anyhow::Result;
use serde_json::{Map, Value};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TabSnapshot {
    pub(super) id: i64,
    pub(super) window_id: Option<i64>,
    pub(super) active: bool,
    pub(super) url: Option<String>,
    pub(super) pending_url: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RestoreTarget {
    pub(super) id: i64,
    pub(super) window_id: i64,
}

pub(super) fn select_created_tab<'a>(
    before: &[TabSnapshot],
    after: &'a [TabSnapshot],
    returned_id: Option<i64>,
    requested_url: &str,
    allow_url_recovery: bool,
) -> Option<(&'a TabSnapshot, bool)> {
    let before_ids = before.iter().map(|tab| tab.id).collect::<HashSet<_>>();
    let delta = after
        .iter()
        .filter(|tab| !before_ids.contains(&tab.id))
        .collect::<Vec<_>>();
    if let Some(returned_id) = returned_id
        && let Some(created) = delta.iter().find(|tab| tab.id == returned_id)
    {
        return Some((*created, false));
    }
    allow_url_recovery
        .then(|| {
            let matching = delta
                .into_iter()
                .filter(|tab| tab_matches_requested_url(tab, requested_url))
                .collect::<Vec<_>>();
            (matching.len() == 1)
                .then(|| matching.first().copied().map(|created| (created, true)))
                .flatten()
        })
        .flatten()
}

pub(super) fn parse_tab_inventory(value: &Value) -> Result<Vec<TabSnapshot>> {
    let tabs = value
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("tab inventory result is not an array"))?;
    let mut ids = HashSet::with_capacity(tabs.len());
    let mut active_windows = HashSet::new();
    let mut snapshots = Vec::with_capacity(tabs.len());
    for (index, value) in tabs.iter().enumerate() {
        let tab = value
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("tab inventory entry {index} is not an object"))?;
        let snapshot = parse_tab(tab, index)?;
        if !ids.insert(snapshot.id) {
            anyhow::bail!("tab inventory contains duplicate tab id {}", snapshot.id);
        }
        if snapshot.active
            && let Some(window_id) = snapshot.window_id
            && !active_windows.insert(window_id)
        {
            anyhow::bail!("tab inventory has multiple active tabs in one window");
        }
        snapshots.push(snapshot);
    }
    Ok(snapshots)
}

fn parse_tab(tab: &Map<String, Value>, index: usize) -> Result<TabSnapshot> {
    let positive = |field| {
        tab.get(field)
            .and_then(Value::as_i64)
            .filter(|value| *value > 0)
            .ok_or_else(|| anyhow::anyhow!("tab inventory entry {index} has invalid {field}"))
    };
    let nullable = |field| match tab.get(field) {
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(Value::Null) | None => Ok(None),
        _ => anyhow::bail!("tab inventory entry {index} has invalid {field}"),
    };
    let optional_positive = |field| match tab.get(field) {
        Some(value) => value
            .as_i64()
            .filter(|value| *value > 0)
            .map(Some)
            .ok_or_else(|| anyhow::anyhow!("tab inventory entry {index} has invalid {field}")),
        None => Ok(None),
    };
    Ok(TabSnapshot {
        id: positive("id")?,
        window_id: optional_positive("windowId")?,
        active: tab
            .get("active")
            .and_then(Value::as_bool)
            .ok_or_else(|| anyhow::anyhow!("tab inventory entry {index} has invalid active"))?,
        url: nullable("url")?,
        pending_url: nullable("pendingUrl")?,
    })
}

pub(super) fn active_in_window(tabs: &[TabSnapshot], window_id: i64) -> Option<RestoreTarget> {
    let active = tabs
        .iter()
        .filter(|tab| tab.window_id == Some(window_id) && tab.active)
        .collect::<Vec<_>>();
    (active.len() == 1).then(|| RestoreTarget {
        id: active[0].id,
        window_id,
    })
}

pub(super) fn tab_matches_requested_url(tab: &TabSnapshot, requested_url: &str) -> bool {
    [tab.url.as_deref(), tab.pending_url.as_deref()]
        .into_iter()
        .flatten()
        .filter_map(canonical_url)
        .any(|url| url == requested_url)
}

pub(super) fn canonical_url(value: &str) -> Option<String> {
    let mut parsed = url::Url::parse(value).ok()?;
    parsed.set_fragment(None);
    Some(parsed.to_string())
}

pub(super) fn valid_created_tab_id(value: &Value) -> Option<i64> {
    value.get("id").and_then(Value::as_i64).filter(|id| *id > 0)
}
