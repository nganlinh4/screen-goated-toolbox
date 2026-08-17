use super::contract::{MemberJoin, TranslationSelection};

pub(super) fn distribute_text(
    text: &str,
    selections: &[TranslationSelection],
    joins: &[MemberJoin],
) -> Vec<String> {
    if selections.is_empty() {
        return Vec::new();
    }
    if selections.len() == 1 || joins.len() + 1 != selections.len() {
        return vec![text.to_string()];
    }
    let vertical = selections.iter().map(is_vertical).collect::<Vec<_>>();
    if vertical.iter().any(|value| *value != vertical[0]) {
        let mut segments = vec![String::new(); selections.len()];
        segments[0] = text.to_string();
        return segments;
    }
    let capacities = selections
        .iter()
        .map(|selection| {
            let bounds = selection.bounds;
            let width = bounds.right.saturating_sub(bounds.left).max(1);
            let height = bounds.bottom.saturating_sub(bounds.top).max(1);
            f64::from(if vertical[0] { height } else { width })
        })
        .collect::<Vec<_>>();
    let words = text.split_whitespace().collect::<Vec<_>>();
    if words.len() >= selections.len() {
        return partition(&words, &capacities, " ");
    }
    let characters = text
        .chars()
        .filter(|character| !character.is_whitespace())
        .map(|character| character.to_string())
        .collect::<Vec<_>>();
    if characters.len() >= selections.len() {
        let refs = characters.iter().map(String::as_str).collect::<Vec<_>>();
        return partition(&refs, &capacities, "");
    }
    let mut segments = vec![String::new(); selections.len()];
    segments[0] = text.to_string();
    segments
}

fn is_vertical(selection: &TranslationSelection) -> bool {
    let bounds = selection.bounds;
    bounds.bottom.saturating_sub(bounds.top)
        > bounds.right.saturating_sub(bounds.left).saturating_mul(3) / 2
}

fn partition(tokens: &[&str], capacities: &[f64], separator: &str) -> Vec<String> {
    let total_capacity = capacities.iter().sum::<f64>().max(1.0);
    let mut result = Vec::with_capacity(capacities.len());
    let mut start = 0usize;
    let mut consumed_capacity = 0.0;
    for (index, capacity) in capacities.iter().enumerate() {
        let remaining_slots = capacities.len() - index;
        if remaining_slots == 1 {
            result.push(tokens[start..].join(separator));
            break;
        }
        consumed_capacity += capacity;
        let ideal_end = (tokens.len() as f64 * consumed_capacity / total_capacity).round() as usize;
        let end = ideal_end.clamp(start + 1, tokens.len() - (remaining_slots - 1));
        result.push(tokens[start..end].join(separator));
        start = end;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::screen_translate::contract::NormalizedBounds;

    fn selection(id: u16, bounds: [u16; 4]) -> TranslationSelection {
        TranslationSelection {
            region_id: id,
            candidate_id: format!("r{id}c0"),
            source_text: id.to_string(),
            bounds: NormalizedBounds::from(bounds),
        }
    }

    #[test]
    fn complete_translation_uses_each_approved_line_capacity() {
        let result = distribute_text(
            "one two three four five six seven eight",
            &[
                selection(1, [0, 0, 20, 300]),
                selection(2, [25, 0, 45, 100]),
            ],
            &[MemberJoin::WrappedLine],
        );
        assert_eq!(result, ["one two three four five six", "seven eight"]);
    }

    #[test]
    fn unspaced_translation_is_distributed_by_character() {
        let result = distribute_text(
            "翻訳された文章",
            &[
                selection(1, [0, 0, 20, 300]),
                selection(2, [25, 0, 45, 100]),
            ],
            &[MemberJoin::WrappedLine],
        );
        assert_eq!(result, ["翻訳された", "文章"]);
    }
}
