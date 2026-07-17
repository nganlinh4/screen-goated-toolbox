//! Content-free diagnostics for exact replacement misses.
//!
//! The tool remains exact: diagnostics never choose or mutate a fuzzy target.
//! They only distinguish a non-contiguous block or line-ending mismatch so the
//! caller can repair the replacement deterministically.

use serde_json::{Value, json};
use std::path::Path;

struct Line<'a> {
    text: &'a str,
    ordinal: usize,
}

pub(super) fn missing_match_failure(
    path: &Path,
    replacement_index: usize,
    expected_count: usize,
    original: &str,
    old_text: &str,
) -> Value {
    let requested = logical_lines(old_text);
    let source = logical_lines(original);
    let diagnostic = diagnose(&requested, &source);
    let (code, message) = match diagnostic {
        Some(Diagnostic::NonContiguous) => (
            "ERR_TEXT_FILE_MATCH_NONCONTIGUOUS",
            "old_text combines unique source lines that are not adjacent; split them into separate exact replacements, or include every intervening line exactly",
        ),
        Some(Diagnostic::Reordered) => (
            "ERR_TEXT_FILE_MATCH_REORDERED",
            "old_text combines unique source lines in a different order; use separate exact replacements or preserve their source order",
        ),
        Some(Diagnostic::LineEndings) => (
            "ERR_TEXT_FILE_LINE_ENDING_MISMATCH",
            "old_text lines are adjacent but their line endings differ from the file; copy the exact block returned by read_text_file",
        ),
        None => (
            "ERR_TEXT_FILE_MATCH_MISSING",
            "an exact replacement target was not found; re-read the current file and copy a contiguous old_text block exactly",
        ),
    };
    let mut value = super::failure(code, Some(path), message, true);
    if let Some(fields) = value.as_object_mut() {
        fields.insert("replacement_index".to_string(), json!(replacement_index));
        fields.insert("expected_count".to_string(), json!(expected_count));
        fields.insert("actual_count".to_string(), json!(0));
        fields.insert("requested_line_count".to_string(), json!(requested.len()));
        fields.insert(
            "repair_strategy".to_string(),
            json!(match diagnostic {
                Some(Diagnostic::NonContiguous | Diagnostic::Reordered) => {
                    "split_into_independent_exact_replacements"
                }
                Some(Diagnostic::LineEndings) => "copy_exact_contiguous_block",
                None => "reread_and_copy_exact_contiguous_block",
            }),
        );
    }
    value
}

#[derive(Clone, Copy)]
enum Diagnostic {
    NonContiguous,
    Reordered,
    LineEndings,
}

fn diagnose(requested: &[Line<'_>], source: &[Line<'_>]) -> Option<Diagnostic> {
    if requested.len() < 2 {
        return None;
    }
    let ordinals = requested
        .iter()
        .map(|requested_line| {
            let matches = source
                .iter()
                .filter(|source_line| source_line.text == requested_line.text)
                .collect::<Vec<_>>();
            (matches.len() == 1).then(|| matches[0].ordinal)
        })
        .collect::<Option<Vec<_>>>()?;
    if ordinals.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Some(Diagnostic::Reordered);
    }
    if ordinals.windows(2).any(|pair| pair[1] != pair[0] + 1) {
        return Some(Diagnostic::NonContiguous);
    }
    Some(Diagnostic::LineEndings)
}

fn logical_lines(text: &str) -> Vec<Line<'_>> {
    text.split('\n')
        .enumerate()
        .filter_map(|(ordinal, line)| {
            let text = line.strip_suffix('\r').unwrap_or(line);
            (!text.is_empty()).then_some(Line { text, ordinal })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_lines_with_a_gap_are_reported_as_non_contiguous() {
        let result = missing_match_failure(
            Path::new("data.csv"),
            0,
            1,
            "a,1\nmiddle,2\nb,3\n",
            "a,1\nb,3",
        );
        assert_eq!(result["code"], "ERR_TEXT_FILE_MATCH_NONCONTIGUOUS");
        assert_eq!(
            result["repair_strategy"],
            "split_into_independent_exact_replacements"
        );
    }

    #[test]
    fn adjacent_lines_with_different_endings_get_an_exact_copy_hint() {
        let result =
            missing_match_failure(Path::new("data.csv"), 0, 1, "a,1\r\nb,2\r\n", "a,1\nb,2");
        assert_eq!(result["code"], "ERR_TEXT_FILE_LINE_ENDING_MISMATCH");
        assert_eq!(result["repair_strategy"], "copy_exact_contiguous_block");
    }
}
