pub(super) fn merge_text(existing: &mut String, incoming: &str) {
    let incoming = incoming.trim();
    if incoming.is_empty() {
        return;
    }
    let current = existing.trim();
    if current.is_empty() || incoming.starts_with(current) {
        *existing = incoming.to_string();
    } else if current.ends_with(incoming) || current.contains(incoming) {
        return;
    } else {
        let overlap = longest_suffix_prefix_overlap(current, incoming);
        if overlap < incoming.len() {
            existing.push(' ');
            existing.push_str(incoming[overlap..].trim_start());
        }
    }
}

pub(super) fn take_text_delta(text: &str, previous: &mut String) -> String {
    let current = text.trim();
    let last = previous.trim();
    let mut should_update_previous = !current.is_empty();
    let delta = if current.is_empty() || current == last {
        String::new()
    } else if last.contains(current) {
        should_update_previous = false;
        String::new()
    } else if last.is_empty() {
        current.to_string()
    } else if let Some(suffix) = current.strip_prefix(last) {
        suffix.trim().to_string()
    } else {
        let overlap = longest_suffix_prefix_overlap(last, current);
        current[overlap..].trim().to_string()
    };
    if should_update_previous {
        *previous = current.to_string();
    }
    delta
}

pub(super) fn nonempty_text(delta: String, full: &str, fallback: &str) -> String {
    if !delta.trim().is_empty() {
        delta
    } else if !full.trim().is_empty() {
        full.trim().to_string()
    } else {
        fallback.to_string()
    }
}

fn longest_suffix_prefix_overlap(left: &str, right: &str) -> usize {
    let max_len = left.len().min(right.len());
    (1..=max_len)
        .rev()
        .find(|len| {
            left.is_char_boundary(left.len() - len)
                && right.is_char_boundary(*len)
                && left[left.len() - len..] == right[..*len]
        })
        .unwrap_or(0)
}
