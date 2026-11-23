//! Incremental card text publication.
//!
//! The parent keeps each card's displayed text in its scene and sends only the
//! change: how many UTF-16 units to drop from the end of the committed text,
//! the committed text to append, and the (short) draft. A growing transcript
//! therefore costs work proportional to what was added, not to its length.
//! Snapshots still carry the full text, so a restarted or resynchronized
//! renderer starts from an authoritative base.

use super::protocol::{CardText, TextEdit};

/// Brings `current` to the card text shown for `committed` and `draft`,
/// returning the edit that does the same in the renderer, or `None` when the
/// card already shows it.
pub(super) fn update(current: &mut CardText, committed: &str, draft: &str) -> Option<TextEdit> {
    let committed = committed.trim_end();
    let draft = draft.trim_start();
    let separated = !committed.is_empty() && !draft.is_empty();
    let draft_unchanged = if separated {
        current.draft.strip_prefix(' ') == Some(draft)
    } else {
        current.draft == draft
    };
    let shared = shared_prefix_len(&current.committed, committed);
    if shared == current.committed.len() && shared == committed.len() && draft_unchanged {
        return None;
    }
    let remove_utf16 = utf16_len(&current.committed[shared..]);
    let append = &committed[shared..];
    current.committed.truncate(shared);
    current.committed.push_str(append);
    current.draft.clear();
    if separated {
        current.draft.push(' ');
    }
    current.draft.push_str(draft);
    Some(TextEdit {
        remove_utf16,
        append: append.to_owned(),
        draft: current.draft.clone(),
    })
}

/// Applies `edit` to a full card text.
pub(super) fn apply(text: &mut CardText, edit: &TextEdit) {
    remove_utf16_from_end(&mut text.committed, edit.remove_utf16);
    text.committed.push_str(&edit.append);
    text.draft.clone_from(&edit.draft);
}

/// Folds `next` into `first`, so one edit does what both did in order.
pub(super) fn compose(first: &mut TextEdit, next: TextEdit) {
    let remaining = remove_utf16_from_end(&mut first.append, next.remove_utf16);
    first.remove_utf16 = first.remove_utf16.saturating_add(remaining);
    first.append.push_str(&next.append);
    first.draft = next.draft;
}

/// Drops `units` UTF-16 code units from the end of `text`, returning the units
/// that `text` was too short to supply.
fn remove_utf16_from_end(text: &mut String, units: usize) -> usize {
    let mut remaining = units;
    while remaining > 0 {
        let Some(character) = text.pop() else {
            break;
        };
        remaining = remaining.saturating_sub(character.len_utf16());
    }
    remaining
}

fn utf16_len(text: &str) -> usize {
    text.chars().map(char::len_utf16).sum()
}

/// Length in bytes of the longest common prefix that ends on a character
/// boundary. Appends, the common case, take one `memcmp`.
fn shared_prefix_len(previous: &str, next: &str) -> usize {
    if next.starts_with(previous) {
        return previous.len();
    }
    let mut shared = previous
        .bytes()
        .zip(next.bytes())
        .position(|(left, right)| left != right)
        .unwrap_or_else(|| previous.len().min(next.len()));
    while !next.is_char_boundary(shared) {
        shared -= 1;
    }
    shared
}

#[cfg(test)]
#[path = "text_edit_tests.rs"]
mod tests;
