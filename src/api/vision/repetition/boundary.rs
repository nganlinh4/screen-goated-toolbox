use super::{snap_to_token_end, squeeze};

/// Resolves a scan hit to the start of the restarted output.
///
/// Fragmented restatements arrive one physical line at a time. A hit within a
/// later line is safe only when that line begins by replaying the reply prefix;
/// this prevents a similar but complete list item from being mistaken for the
/// onset merely because damage occurs farther down the tail.
pub(super) fn restart_boundary(text: &str, hit: usize) -> Option<usize> {
    let line_start = text[..hit]
        .rfind(['\r', '\n'])
        .map_or(0, |offset| offset + 1);
    if line_start == 0 {
        return Some(snap_to_token_end(text, hit));
    }
    if line_restarts_prefix(text, line_start) {
        return Some(line_start);
    }

    // A hit in the final token of a complete line identifies the boundary
    // after that line. This occurs while streaming, when enough evidence has
    // accumulated only after the scan has passed the true restart point.
    let line_end = text[line_start..]
        .find(['\r', '\n'])
        .map_or(text.len(), |offset| line_start + offset);
    let snapped = snap_to_token_end(text, hit);
    if hit > line_start && snapped == line_end {
        return Some(line_end);
    }

    // The scan hit can land on the second half of a seam (`Screenshot` / `t`).
    // In that case the immediately preceding physical line is the restart.
    let previous_end = text[..line_start].trim_end_matches(['\r', '\n']).len();
    let previous_start = text[..previous_end]
        .rfind(['\r', '\n'])
        .map_or(0, |offset| offset + 1);
    (previous_start > 0 && line_restarts_prefix(text, previous_start)).then_some(previous_start)
}

fn line_restarts_prefix(text: &str, line_start: usize) -> bool {
    let line_end = text[line_start..]
        .find(['\r', '\n'])
        .map_or(text.len(), |offset| line_start + offset);
    let line = normalized_scan_text(&text[line_start..line_end]);
    let prefix = normalized_scan_text(&text[..line_start]);
    !line.is_empty() && prefix.starts_with(&line)
}

fn normalized_scan_text(text: &str) -> String {
    let folded: String = text
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect();
    squeeze(&folded)
}
