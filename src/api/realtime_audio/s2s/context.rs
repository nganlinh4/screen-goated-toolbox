use std::collections::VecDeque;

use super::{
    CONTEXT_LINE_CHAR_LIMIT, CONTEXT_SEGMENT_LIMIT, CONTEXT_TOTAL_CHAR_LIMIT, truncate_chars,
};

#[derive(Clone)]
struct S2sContextEntry {
    id: u64,
    source: String,
    target: String,
}

#[derive(Default)]
pub(super) struct S2sContextMemory {
    completed: VecDeque<S2sContextEntry>,
}

#[derive(Clone)]
pub(super) struct S2sContextSnapshot {
    pub(super) text: String,
}

impl S2sContextMemory {
    pub(super) fn push_completed(&mut self, id: u64, source: &str, target: &str) {
        let source = truncate_chars(source.trim(), CONTEXT_LINE_CHAR_LIMIT);
        let target = truncate_chars(target.trim(), CONTEXT_LINE_CHAR_LIMIT);
        if source.is_empty() && target.is_empty() {
            return;
        }
        if self.completed.iter().any(|entry| entry.id == id) {
            return;
        }
        self.completed
            .push_back(S2sContextEntry { id, source, target });
        while self.completed.len() > CONTEXT_SEGMENT_LIMIT {
            self.completed.pop_front();
        }
    }

    pub(super) fn snapshot(&self) -> S2sContextSnapshot {
        let entries = self
            .completed
            .iter()
            .rev()
            .take(CONTEXT_SEGMENT_LIMIT)
            .cloned()
            .collect::<Vec<_>>();
        if entries.is_empty() {
            return S2sContextSnapshot {
                text: String::new(),
            };
        }

        let mut ordered = entries;
        ordered.reverse();
        let mut text = String::from(
            "\n\nPrevious context for continuity only. Do not translate or speak this again.\n",
        );
        text.push_str("Use it only for pronouns, names, terminology, and topic continuity.\n");
        for (index, entry) in ordered.iter().enumerate() {
            let number = index + 1;
            if !entry.source.is_empty() {
                text.push_str(&format!("Previous source {number}: {}\n", entry.source));
            }
            if !entry.target.is_empty() {
                text.push_str(&format!(
                    "Previous translation {number}: {}\n",
                    entry.target
                ));
            }
        }
        text.push_str("Now translate only the new incoming audio segment.");
        if text.chars().count() > CONTEXT_TOTAL_CHAR_LIMIT {
            text = truncate_chars(&text, CONTEXT_TOTAL_CHAR_LIMIT);
        }
        S2sContextSnapshot { text }
    }
}
