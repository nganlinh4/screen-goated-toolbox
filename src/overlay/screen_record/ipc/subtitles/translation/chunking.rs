use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use super::super::types::SubtitleTranslationItemRequest;
use super::TRANSLATION_RETRY_BASE_DELAY_MS;

pub(super) fn translation_retry_delay(attempt_index: usize) -> Duration {
    let multiplier = 1u64 << attempt_index.saturating_sub(1).min(4);
    Duration::from_millis(TRANSLATION_RETRY_BASE_DELAY_MS * multiplier)
}

pub(super) fn sleep_cancelable(cancelled: &AtomicBool, duration: Duration) {
    let step = Duration::from_millis(100);
    let mut slept = Duration::ZERO;
    while slept < duration && !cancelled.load(Ordering::SeqCst) {
        let remaining = duration.saturating_sub(slept);
        let next = remaining.min(step);
        std::thread::sleep(next);
        slept += next;
    }
}

pub(super) fn initial_translation_chunk_count(
    chunk_count: Option<usize>,
    chunk_mode: &Option<String>,
    item_count: usize,
) -> usize {
    if item_count <= 1 {
        return 1;
    }
    if let Some(chunk_count) = chunk_count {
        return chunk_count.max(1).min(item_count);
    }

    let items_per_chunk = match chunk_mode.as_deref() {
        Some("small") => 25,
        Some("tiny") => 10,
        _ => item_count,
    };
    item_count.div_ceil(items_per_chunk).max(1).min(item_count)
}

pub(super) fn split_translation_items(
    items: &[SubtitleTranslationItemRequest],
    chunk_count: usize,
) -> Vec<Vec<SubtitleTranslationItemRequest>> {
    let mut grouped: Vec<(Option<String>, Vec<SubtitleTranslationItemRequest>)> = Vec::new();
    for item in items {
        let group_id = item.source_group_id.clone();
        if let Some((last_group_id, last_items)) = grouped.last_mut()
            && *last_group_id == group_id
        {
            last_items.push(item.clone());
            continue;
        }
        grouped.push((group_id, vec![item.clone()]));
    }

    let total_items = items.len().max(1);
    let safe_chunk_count = chunk_count.max(1).min(total_items);
    let mut chunks = Vec::with_capacity(safe_chunk_count);
    for (_group_id, group_items) in grouped {
        let group_chunk_count = (safe_chunk_count * group_items.len())
            .div_ceil(total_items)
            .max(1)
            .min(group_items.len().max(1));
        for chunk_index in 0..group_chunk_count {
            let start = chunk_index * group_items.len() / group_chunk_count;
            let end = (chunk_index + 1) * group_items.len() / group_chunk_count;
            if start < end {
                chunks.push(group_items[start..end].to_vec());
            }
        }
    }
    chunks
}
