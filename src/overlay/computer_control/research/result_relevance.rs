//! Query-aware, bounded sampling that keeps coherent evidence neighborhoods.

use std::collections::HashSet;

const CONTEXT_RADIUS: usize = 6;
const PASSAGE_COUNT_TARGET: usize = 3;

pub(super) fn relevant_body_sample(body: &str, max_chars: usize, query: &str) -> String {
    if max_chars == 0 || body.is_empty() {
        return String::new();
    }
    if body.chars().count() <= max_chars {
        return body.to_string();
    }
    let terms = query
        .split(|character: char| !character.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|term| term.chars().count() >= 3)
        .collect::<HashSet<_>>();
    let lines = body
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if lines.is_empty() {
        return span_sample(body, max_chars);
    }

    let mut ranked = (0..lines.len())
        .filter_map(|index| contextual_score(&lines, index, &terms).map(|score| (score, index)))
        .collect::<Vec<_>>();
    ranked.sort_by_key(|((score, matched_terms, direct_signal), index)| {
        (
            std::cmp::Reverse(*score),
            std::cmp::Reverse(*matched_terms),
            std::cmp::Reverse(*direct_signal),
            *index,
        )
    });
    if ranked.is_empty() {
        return span_sample(body, max_chars);
    }

    let relevant_budget = max_chars;
    let passage_budget = (relevant_budget / PASSAGE_COUNT_TARGET).max(160);
    let mut selected = HashSet::new();
    let header = "[query-relevant passages]\n";
    let mut remaining = relevant_budget.saturating_sub(header.chars().count());
    let mut passages = Vec::new();
    for (_, index) in ranked {
        if selected.contains(&index) || remaining == 0 {
            continue;
        }
        let separator_chars = usize::from(!passages.is_empty());
        if remaining <= separator_chars {
            break;
        }
        let available = passage_budget.min(remaining - separator_chars);
        let passage = bounded_context_passage(&lines, index, available, &terms, &selected);
        if passage.text.is_empty() {
            continue;
        }
        for neighbor in passage.first..=passage.last {
            selected.insert(neighbor);
        }
        remaining = remaining.saturating_sub(separator_chars + passage.text.chars().count());
        passages.push(passage);
        if passages.len() >= PASSAGE_COUNT_TARGET {
            break;
        }
    }
    if passages.is_empty() {
        return span_sample(body, max_chars);
    }
    passages.sort_by_key(|passage| passage.first);
    let mut relevant = String::from(header);
    for passage in passages {
        push_bounded(&mut relevant, &passage.text, relevant_budget);
    }
    relevant.chars().take(max_chars).collect()
}

fn contextual_score(
    lines: &[&str],
    index: usize,
    terms: &HashSet<String>,
) -> Option<(usize, usize, usize)> {
    let first = index.saturating_sub(CONTEXT_RADIUS);
    let last = (index + CONTEXT_RADIUS).min(lines.len() - 1);
    let mut matched = HashSet::new();
    let mut value_weight = 0usize;
    for line in lines.iter().take(last + 1).skip(first) {
        let lower = line.to_lowercase();
        for term in terms {
            if approximate_term_match(term, &lower) {
                matched.insert(term.as_str());
            }
        }
        value_weight += value_signal_weight(line);
    }
    let direct_line = lines[index].to_lowercase();
    let direct_matches = terms
        .iter()
        .filter(|term| approximate_term_match(term, &direct_line))
        .count();
    let direct_value = value_signal_weight(lines[index]);
    (!matched.is_empty()).then(|| {
        let fact_bonus = value_weight.min(24);
        let direct_signal = direct_matches * 8 + direct_value.min(8) * 8;
        (
            matched.len() * 4 + fact_bonus + direct_signal,
            matched.len(),
            direct_signal,
        )
    })
}

struct Passage {
    first: usize,
    last: usize,
    text: String,
}

fn bounded_context_passage(
    lines: &[&str],
    center: usize,
    max_chars: usize,
    terms: &HashSet<String>,
    selected: &HashSet<usize>,
) -> Passage {
    let mut first = center.saturating_sub(CONTEXT_RADIUS);
    let mut last = (center + CONTEXT_RADIUS).min(lines.len() - 1);
    if let Some(neighbor) = (first..center)
        .rev()
        .find(|neighbor| selected.contains(neighbor))
    {
        first = neighbor + 1;
    }
    if let Some(neighbor) = ((center + 1)..=last).find(|neighbor| selected.contains(neighbor)) {
        last = neighbor - 1;
    }
    while first < last && passage_char_count(lines, first, last) > max_chars {
        let left_distance = center - first;
        let right_distance = last - center;
        if left_distance > right_distance
            || (left_distance == right_distance
                && lines[first].chars().count() >= lines[last].chars().count())
        {
            first += 1;
        } else {
            last -= 1;
        }
    }
    if first == last && lines[center].chars().count() > max_chars {
        return Passage {
            first: center,
            last: center,
            text: focal_line_sample(lines[center], max_chars, terms),
        };
    }
    Passage {
        first,
        last,
        text: lines[first..=last].join("\n"),
    }
}

fn passage_char_count(lines: &[&str], first: usize, last: usize) -> usize {
    lines[first..=last]
        .iter()
        .map(|line| line.chars().count())
        .sum::<usize>()
        + last.saturating_sub(first)
}

fn focal_line_sample(line: &str, max_chars: usize, terms: &HashSet<String>) -> String {
    let chars = line.chars().collect::<Vec<_>>();
    if chars.len() <= max_chars {
        return line.to_string();
    }
    let focus = chars
        .iter()
        .position(|character| {
            matches!(character, '$' | '€' | '£' | '¥' | '₩' | '₹' | '%')
                || character.is_ascii_digit()
        })
        .or_else(|| {
            let lower = line.to_lowercase();
            terms
                .iter()
                .filter_map(|term| lower.find(term))
                .map(|byte_index| lower[..byte_index].chars().count())
                .min()
        })
        .unwrap_or(0);
    let start = focus
        .saturating_sub(max_chars / 2)
        .min(chars.len().saturating_sub(max_chars));
    chars[start..(start + max_chars).min(chars.len())]
        .iter()
        .collect()
}

fn approximate_term_match(term: &str, text: &str) -> bool {
    if text.contains(term) {
        return true;
    }
    text.split(|character: char| !character.is_alphanumeric())
        .filter(|token| token.chars().count() >= 4)
        .any(|token| common_prefix_chars(term, token) >= 4)
}

fn common_prefix_chars(left: &str, right: &str) -> usize {
    left.chars()
        .zip(right.chars())
        .take_while(|(left, right)| left == right)
        .count()
}

fn value_signal_weight(line: &str) -> usize {
    if line
        .chars()
        .any(|character| matches!(character, '$' | '€' | '£' | '¥' | '₩' | '₹'))
    {
        6
    } else if line.contains('%') {
        4
    } else if line.chars().any(|character| character.is_ascii_digit()) {
        1
    } else {
        0
    }
}

fn span_sample(text: &str, max_chars: usize) -> String {
    let chars = text.chars().collect::<Vec<_>>();
    if chars.len() <= max_chars {
        return text.to_string();
    }
    const LABELS: [&str; 3] = ["[start]\n", "\n[middle]\n", "\n[end]\n"];
    let label_chars = LABELS
        .iter()
        .map(|label| label.chars().count())
        .sum::<usize>();
    if max_chars <= label_chars + 3 {
        return chars.into_iter().take(max_chars).collect();
    }
    let chunk = (max_chars - label_chars) / 3;
    let starts = [
        0,
        chars.len().saturating_div(2).saturating_sub(chunk / 2),
        chars.len().saturating_sub(chunk),
    ];
    let mut sampled = String::new();
    for (label, start) in LABELS.into_iter().zip(starts) {
        sampled.push_str(label);
        sampled.extend(chars[start..(start + chunk).min(chars.len())].iter());
    }
    sampled.chars().take(max_chars).collect()
}

fn push_bounded(output: &mut String, text: &str, max_chars: usize) {
    if output.chars().count() >= max_chars {
        return;
    }
    if !output.is_empty() && !output.ends_with('\n') {
        output.push('\n');
    }
    let remaining = max_chars.saturating_sub(output.chars().count());
    output.extend(text.chars().take(remaining));
}
