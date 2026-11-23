use super::{CommitContext, HistoryItem, MediaRef, notify_ui, run_history_action};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub(super) struct Change {
    snapshot: Vec<HistoryItem>,
    removed: Vec<MediaRef>,
}

pub(super) fn insert(
    item: HistoryItem,
    token: Option<Arc<crate::overlay::result::ChainCancelToken>>,
    clear_epoch: u64,
    context: &CommitContext<'_>,
) -> Option<Change> {
    let mut snapshot = context
        .cache
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone();
    snapshot.insert(0, item);
    let removed = trim(&mut snapshot, context.max_items);
    let mut admitted = false;
    run_history_action(token, || {
        admitted = context.epoch.load(Ordering::Acquire) == clear_epoch;
    });
    admitted.then_some(Change { snapshot, removed })
}

pub(super) fn delete(cache: &Mutex<Vec<HistoryItem>>, target: &HistoryItem) -> Option<Change> {
    let mut snapshot = cache
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone();
    let index = snapshot.iter().position(|item| item == target)?;
    let removed = vec![MediaRef::from(&snapshot.remove(index))];
    Some(Change { snapshot, removed })
}

pub(super) fn clear(cache: &Mutex<Vec<HistoryItem>>) -> Change {
    let removed = cache
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .iter()
        .map(MediaRef::from)
        .collect();
    Change {
        snapshot: Vec::new(),
        removed,
    }
}

pub(super) fn prune(cache: &Mutex<Vec<HistoryItem>>, limit: usize) -> Option<Change> {
    let mut snapshot = cache
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone();
    let removed = trim(&mut snapshot, limit);
    (!removed.is_empty()).then_some(Change { snapshot, removed })
}

fn trim(snapshot: &mut Vec<HistoryItem>, limit: usize) -> Vec<MediaRef> {
    let limit = limit.clamp(10, 200);
    if snapshot.len() <= limit {
        return Vec::new();
    }
    snapshot
        .split_off(limit)
        .iter()
        .map(MediaRef::from)
        .collect()
}

pub(super) fn publish_with(
    cache: &Mutex<Vec<HistoryItem>>,
    failed: &AtomicBool,
    change: Change,
    mut persist: impl FnMut(&[HistoryItem]) -> bool,
    before_publish: impl FnOnce(),
) -> Option<Vec<MediaRef>> {
    if !persist(&change.snapshot) {
        failed.store(true, Ordering::Release);
        notify_ui();
        return None;
    }
    before_publish();
    *cache.lock().unwrap_or_else(|error| error.into_inner()) = change.snapshot;
    failed.store(false, Ordering::Release);
    notify_ui();
    Some(change.removed)
}
