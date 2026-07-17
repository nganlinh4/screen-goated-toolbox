//! Automatic preservation of opaque formula fields and record shape during
//! ordinary table work.
//!
//! Formula-only edits remain untouched and therefore still require the
//! content-bound structural token. When a request also changes ordinary data,
//! formula bytes are restored inside aligned row/block replacements and
//! separate formula-only replacement groups are omitted.

use std::collections::HashSet;
use std::ops::Range;
use std::path::Path;

use super::Replacement;

#[path = "text_file_delimited_repair.rs"]
mod delimited_repair;

pub(super) struct NormalizedReplacements {
    pub(super) replacements: Vec<Replacement>,
    pub(super) requested_groups: usize,
    pub(super) preserved_cells: usize,
    pub(super) rewritten_groups: usize,
    pub(super) omitted_groups: usize,
    pub(super) trailing_empty_fields_omitted: usize,
    pub(super) trailing_value_fields_repaired: usize,
}

#[derive(Clone)]
struct Cell {
    record: usize,
    field: usize,
    raw: Range<usize>,
    formula: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum ChangeKind {
    None,
    Data,
    FormulaOnly,
    Mixed,
    Unknown,
}

struct Analysis {
    kind: ChangeKind,
    formula_cells: usize,
}

pub(super) fn normalize(
    path: &Path,
    original: &str,
    replacements: Vec<Replacement>,
) -> NormalizedReplacements {
    let requested_groups = replacements.len();
    let Some(delimiter) = delimiter_for(path) else {
        return unchanged(replacements, requested_groups, 0, 0);
    };
    let mut trailing_empty_fields_omitted = 0usize;
    let mut trailing_value_fields_repaired = 0usize;
    let replacements = replacements
        .into_iter()
        .map(|mut replacement| {
            if let Some((normalized, count)) =
                delimited_repair::trim_redundant_trailing_empty_fields(
                    &replacement.old_text,
                    &replacement.new_text,
                    delimiter,
                )
            {
                replacement.new_text = normalized;
                trailing_empty_fields_omitted = trailing_empty_fields_omitted
                    .saturating_add(count.saturating_mul(replacement.expected_count));
            }
            if let Some((normalized, count)) = delimited_repair::serialize_split_trailing_values(
                &replacement.old_text,
                &replacement.new_text,
                delimiter,
            ) {
                replacement.new_text = normalized;
                trailing_value_fields_repaired = trailing_value_fields_repaired
                    .saturating_add(count.saturating_mul(replacement.expected_count));
            }
            replacement
        })
        .collect::<Vec<_>>();
    let original_cells = parse_cells(original, delimiter);
    let analyses = replacements
        .iter()
        .map(|replacement| analyze(replacement, original, original_cells.as_deref(), delimiter))
        .collect::<Vec<_>>();
    let has_ordinary_change = analyses
        .iter()
        .any(|analysis| matches!(analysis.kind, ChangeKind::Data | ChangeKind::Mixed));
    if !has_ordinary_change {
        return unchanged(
            replacements,
            requested_groups,
            trailing_empty_fields_omitted,
            trailing_value_fields_repaired,
        );
    }

    let mut output = Vec::with_capacity(replacements.len());
    let mut preserved_cells = 0usize;
    let mut rewritten_groups = 0usize;
    let mut omitted_groups = 0usize;
    for (mut replacement, analysis) in replacements.into_iter().zip(analyses) {
        match analysis.kind {
            ChangeKind::FormulaOnly => {
                preserved_cells = preserved_cells.saturating_add(
                    analysis
                        .formula_cells
                        .saturating_mul(replacement.expected_count),
                );
                omitted_groups += 1;
            }
            ChangeKind::Mixed => {
                if let Some((rewritten, count)) = preserve_fragment_formulas(
                    &replacement.old_text,
                    &replacement.new_text,
                    delimiter,
                ) {
                    replacement.new_text = rewritten;
                    preserved_cells = preserved_cells
                        .saturating_add(count.saturating_mul(replacement.expected_count));
                    rewritten_groups += 1;
                }
                output.push(replacement);
            }
            ChangeKind::None | ChangeKind::Data | ChangeKind::Unknown => output.push(replacement),
        }
    }
    NormalizedReplacements {
        replacements: output,
        requested_groups,
        preserved_cells,
        rewritten_groups,
        omitted_groups,
        trailing_empty_fields_omitted,
        trailing_value_fields_repaired,
    }
}

pub(super) fn without_preservation(replacements: Vec<Replacement>) -> NormalizedReplacements {
    let requested_groups = replacements.len();
    unchanged(replacements, requested_groups, 0, 0)
}

fn unchanged(
    replacements: Vec<Replacement>,
    requested_groups: usize,
    trailing_empty_fields_omitted: usize,
    trailing_value_fields_repaired: usize,
) -> NormalizedReplacements {
    NormalizedReplacements {
        replacements,
        requested_groups,
        preserved_cells: 0,
        rewritten_groups: 0,
        omitted_groups: 0,
        trailing_empty_fields_omitted,
        trailing_value_fields_repaired,
    }
}

fn delimiter_for(path: &Path) -> Option<u8> {
    let extension = path.extension()?.to_str()?;
    if extension.eq_ignore_ascii_case("csv") {
        Some(b',')
    } else if extension.eq_ignore_ascii_case("tsv") {
        Some(b'\t')
    } else {
        None
    }
}

fn analyze(
    replacement: &Replacement,
    original: &str,
    original_cells: Option<&[Cell]>,
    delimiter: u8,
) -> Analysis {
    if replacement.old_text == replacement.new_text {
        return Analysis {
            kind: ChangeKind::None,
            formula_cells: 0,
        };
    }
    let range_analysis = analyze_original_ranges(replacement, original, original_cells);
    if matches!(
        range_analysis.kind,
        ChangeKind::FormulaOnly | ChangeKind::Unknown
    ) {
        return range_analysis;
    }
    if let Some(analysis) =
        analyze_aligned_fragments(&replacement.old_text, &replacement.new_text, delimiter)
    {
        return analysis;
    }
    range_analysis
}

fn analyze_aligned_fragments(old: &str, new: &str, delimiter: u8) -> Option<Analysis> {
    let old_cells = parse_cells(old, delimiter)?;
    let new_cells = parse_cells(new, delimiter)?;
    if !cells_align(&old_cells, &new_cells) {
        return None;
    }
    let mut formula_changed = false;
    let mut data_changed = false;
    let mut formula_positions = HashSet::new();
    for (old_cell, new_cell) in old_cells.iter().zip(&new_cells) {
        let changed = old[old_cell.raw.clone()] != new[new_cell.raw.clone()];
        if !changed {
            continue;
        }
        if old_cell.formula || new_cell.formula {
            formula_changed = true;
            formula_positions.insert((old_cell.record, old_cell.field));
        } else {
            data_changed = true;
        }
    }
    Some(Analysis {
        kind: match (data_changed, formula_changed) {
            (false, false) => ChangeKind::None,
            (true, false) => ChangeKind::Data,
            (false, true) => ChangeKind::FormulaOnly,
            (true, true) => ChangeKind::Mixed,
        },
        formula_cells: formula_positions.len(),
    })
}

fn analyze_original_ranges(
    replacement: &Replacement,
    original: &str,
    original_cells: Option<&[Cell]>,
) -> Analysis {
    let Some(cells) = original_cells else {
        return Analysis {
            kind: ChangeKind::Unknown,
            formula_cells: 0,
        };
    };
    let matches = original
        .match_indices(&replacement.old_text)
        .map(|(start, _)| start..start + replacement.old_text.len())
        .collect::<Vec<_>>();
    if matches.len() != replacement.expected_count {
        return Analysis {
            kind: ChangeKind::Unknown,
            formula_cells: 0,
        };
    }
    let formula_ranges = cells
        .iter()
        .filter(|cell| cell.formula)
        .map(|cell| cell.raw.clone())
        .collect::<Vec<_>>();
    let mut kinds = HashSet::new();
    let mut touched = HashSet::new();
    for range in matches {
        let overlaps = formula_ranges
            .iter()
            .enumerate()
            .filter(|(_, formula)| ranges_overlap(&range, formula))
            .collect::<Vec<_>>();
        for (index, _) in &overlaps {
            touched.insert(*index);
        }
        let kind = if overlaps.is_empty() {
            ChangeKind::Data
        } else if overlaps
            .iter()
            .any(|(_, formula)| formula.start <= range.start && range.end <= formula.end)
        {
            ChangeKind::FormulaOnly
        } else {
            ChangeKind::Mixed
        };
        kinds.insert(kind);
    }
    let kind = if kinds.len() == 1 {
        *kinds.iter().next().unwrap_or(&ChangeKind::Unknown)
    } else {
        ChangeKind::Unknown
    };
    Analysis {
        kind,
        formula_cells: touched.len(),
    }
}

fn preserve_fragment_formulas(old: &str, new: &str, delimiter: u8) -> Option<(String, usize)> {
    let old_cells = parse_cells(old, delimiter)?;
    let new_cells = parse_cells(new, delimiter)?;
    if !cells_align(&old_cells, &new_cells) {
        return delimited_repair::preserve_split_formula_tails(old, new, &old_cells, &new_cells);
    }
    let mut patches = Vec::new();
    for (old_cell, new_cell) in old_cells.iter().zip(&new_cells) {
        if old_cell.formula && old[old_cell.raw.clone()] != new[new_cell.raw.clone()] {
            patches.push((new_cell.raw.clone(), old[old_cell.raw.clone()].to_string()));
        }
    }
    delimited_repair::apply_patches(new, patches)
}

fn cells_align(old: &[Cell], new: &[Cell]) -> bool {
    old.len() == new.len()
        && old
            .iter()
            .zip(new)
            .all(|(left, right)| left.record == right.record && left.field == right.field)
}

fn ranges_overlap(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start < right.end && right.start < left.end
}

fn parse_cells(text: &str, delimiter: u8) -> Option<Vec<Cell>> {
    let bytes = text.as_bytes();
    let mut cells = Vec::new();
    let mut start = 0usize;
    let mut index = 0usize;
    let mut record = 0usize;
    let mut field = 0usize;
    let mut in_quotes = false;
    let mut record_started = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if in_quotes {
            if byte == b'"' {
                if bytes.get(index + 1) == Some(&b'"') {
                    index += 2;
                    continue;
                }
                in_quotes = false;
            }
            index += 1;
            continue;
        }
        if byte == b'"' && index == start {
            in_quotes = true;
            record_started = true;
            index += 1;
            continue;
        }
        if byte == delimiter {
            push_cell(&mut cells, text, record, field, start..index);
            field += 1;
            record_started = true;
            index += 1;
            start = index;
            continue;
        }
        if matches!(byte, b'\r' | b'\n') {
            push_cell(&mut cells, text, record, field, start..index);
            if byte == b'\r' && bytes.get(index + 1) == Some(&b'\n') {
                index += 1;
            }
            index += 1;
            start = index;
            record += 1;
            field = 0;
            record_started = false;
            continue;
        }
        record_started = true;
        index += 1;
    }
    if in_quotes {
        return None;
    }
    if record_started || start < bytes.len() || field > 0 {
        push_cell(&mut cells, text, record, field, start..bytes.len());
    }
    Some(cells)
}

fn push_cell(cells: &mut Vec<Cell>, text: &str, record: usize, field: usize, raw: Range<usize>) {
    let formula = decoded_field(&text[raw.clone()])
        .trim_start()
        .starts_with('=');
    cells.push(Cell {
        record,
        field,
        raw,
        formula,
    });
}

fn decoded_field(raw: &str) -> String {
    raw.strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
        .map(|inner| inner.replace("\"\"", "\""))
        .unwrap_or_else(|| raw.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn replacement(old: &str, new: &str) -> Replacement {
        Replacement {
            old_text: old.to_string(),
            new_text: new.to_string(),
            expected_count: 1,
        }
    }

    #[test]
    fn mixed_row_update_restores_exact_formula_field_bytes() {
        let old = "item,status,total\r\nalpha,Unknown,\"=B2*12\"\r\n";
        let new = "item,status,total\r\nalpha,Ready,\"=99\"\r\n";
        let normalized = normalize(Path::new("table.csv"), old, vec![replacement(old, new)]);
        assert_eq!(normalized.replacements.len(), 1);
        assert_eq!(normalized.preserved_cells, 1);
        assert!(normalized.replacements[0].new_text.contains("\"=B2*12\""));
        assert!(!normalized.replacements[0].new_text.contains("\"=99\""));
    }

    #[test]
    fn split_unchanged_formula_tail_is_requoted_without_accepting_extra_data() {
        let old = "alpha,Unknown,\"=AND(B2>0,C2=\"\"Yes\"\")\"\r\n";
        for new in [
            "alpha,Ready,=AND(B2>0,C2=\"Yes\")\r\n",
            "alpha,Ready,=AND(B2>0,C2=\"\"Yes\"\")\r\n",
        ] {
            let normalized = normalize(
                Path::new("table.csv"),
                old,
                vec![replacement(old.trim_end(), new.trim_end())],
            );
            assert_eq!(normalized.preserved_cells, 1);
            assert_eq!(
                normalized.replacements[0].new_text,
                "alpha,Ready,\"=AND(B2>0,C2=\"\"Yes\"\")\""
            );
        }

        let ambiguous = "alpha,Ready,=AND(B2>0,C2=\"Yes\"),extra";
        let normalized = normalize(
            Path::new("table.csv"),
            old,
            vec![replacement(old.trim_end(), ambiguous)],
        );
        assert_eq!(normalized.preserved_cells, 0);
        assert_eq!(normalized.replacements[0].new_text, ambiguous);
    }

    #[test]
    fn ordinary_data_change_drops_only_redundant_trailing_empty_fields() {
        let old = "Label,Pending";
        let normalized = normalize(
            Path::new("table.csv"),
            old,
            vec![replacement(old, "Label,Ready,,")],
        );
        assert_eq!(normalized.trailing_empty_fields_omitted, 2);
        assert_eq!(normalized.replacements[0].new_text, "Label,Ready");

        let nonempty = normalize(
            Path::new("table.csv"),
            old,
            vec![replacement(old, "Label,Ready,,unexpected")],
        );
        assert_eq!(nonempty.trailing_empty_fields_omitted, 0);
        assert_eq!(nonempty.replacements[0].new_text, "Label,Ready,,unexpected");
    }

    #[test]
    fn unchanged_prefix_allows_a_split_trailing_value_to_be_serialized() {
        let old = "Rationale,Unknown";
        let normalized = normalize(
            Path::new("table.csv"),
            old,
            vec![replacement(
                old,
                "Rationale,Supports families, shared vaults, and recovery",
            )],
        );
        assert_eq!(normalized.trailing_value_fields_repaired, 2);
        assert_eq!(
            normalized.replacements[0].new_text,
            "Rationale,\"Supports families, shared vaults, and recovery\""
        );

        let three_columns = normalize(
            Path::new("table.csv"),
            "id,status,notes",
            vec![replacement(
                "id,status,notes",
                "id,status,Changed, with detail",
            )],
        );
        assert_eq!(three_columns.trailing_value_fields_repaired, 1);
        assert_eq!(
            three_columns.replacements[0].new_text,
            "id,status,\"Changed, with detail\""
        );
    }

    #[test]
    fn trailing_value_repair_rejects_ambiguous_structure_changes() {
        for (old, new) in [
            ("id,status,notes", "id,ready,Changed, with detail"),
            ("id,status", "id,status,extra"),
            ("id,status", "id,ready,,extra"),
        ] {
            let normalized = normalize(Path::new("table.csv"), old, vec![replacement(old, new)]);
            assert_eq!(normalized.trailing_value_fields_repaired, 0);
            assert_eq!(normalized.replacements[0].new_text, new);
        }
    }

    #[test]
    fn tsv_trailing_value_with_a_tab_is_safely_quoted() {
        let old = "Label\tPending";
        let normalized = normalize(
            Path::new("table.tsv"),
            old,
            vec![replacement(old, "Label\tReady\twith detail")],
        );
        assert_eq!(normalized.trailing_value_fields_repaired, 1);
        assert_eq!(
            normalized.replacements[0].new_text,
            "Label\t\"Ready\twith detail\""
        );
    }

    #[test]
    fn separate_formula_group_is_omitted_when_data_changes() {
        let original = "item\tstatus\ttotal\nalpha\tUnknown\t=B2*12\n";
        let normalized = normalize(
            Path::new("table.tsv"),
            original,
            vec![
                replacement("Unknown", "Ready"),
                replacement("=B2*12", "=99"),
            ],
        );
        assert_eq!(normalized.replacements.len(), 1);
        assert_eq!(normalized.omitted_groups, 1);
        assert_eq!(normalized.replacements[0].new_text, "Ready");
    }

    #[test]
    fn formula_only_request_is_left_for_content_bound_confirmation() {
        let original = "item,total\nalpha,=B2*12\n";
        let normalized = normalize(
            Path::new("table.csv"),
            original,
            vec![replacement("=B2*12", "=99")],
        );
        assert_eq!(normalized.replacements.len(), 1);
        assert_eq!(normalized.preserved_cells, 0);
        assert_eq!(normalized.replacements[0].new_text, "=99");
    }
}
