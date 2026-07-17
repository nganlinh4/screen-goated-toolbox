//! Unambiguous repairs for malformed delimited replacement fragments.

use std::ops::Range;

use super::{Cell, decoded_field, parse_cells};

pub(super) fn preserve_split_formula_tails(
    old: &str,
    new: &str,
    old_cells: &[Cell],
    new_cells: &[Cell],
) -> Option<(String, usize)> {
    let old_records = record_slices(old_cells);
    let new_records = record_slices(new_cells);
    if old_records.len() != new_records.len() {
        return None;
    }
    let mut patches = Vec::new();
    for (old_record, new_record) in old_records.into_iter().zip(new_records) {
        if old_record.len() == new_record.len() {
            for (old_cell, new_cell) in old_record.iter().zip(new_record) {
                if old_cell.formula && old[old_cell.raw.clone()] != new[new_cell.raw.clone()] {
                    patches.push((new_cell.raw.clone(), old[old_cell.raw.clone()].to_string()));
                }
            }
            continue;
        }
        let old_formula = old_record.last()?;
        if !old_formula.formula
            || old_record.iter().filter(|cell| cell.formula).count() != 1
            || new_record.len() <= old_formula.field
            || old_record[..old_formula.field]
                .iter()
                .zip(&new_record[..old_formula.field])
                .any(|(old_cell, new_cell)| {
                    old_cell.formula
                        || new_cell.formula
                        || old_cell.field != new_cell.field
                        || old_cell.record != new_cell.record
                })
        {
            return None;
        }
        let tail = new_record[old_formula.field].raw.start..new_record.last()?.raw.end;
        if !same_formula_tail(&new[tail.clone()], &old[old_formula.raw.clone()]) {
            return None;
        }
        patches.push((tail, old[old_formula.raw.clone()].to_string()));
    }
    apply_patches(new, patches)
}

pub(super) fn trim_redundant_trailing_empty_fields(
    old: &str,
    new: &str,
    delimiter: u8,
) -> Option<(String, usize)> {
    let old_cells = parse_cells(old, delimiter)?;
    let new_cells = parse_cells(new, delimiter)?;
    let old_records = record_slices(&old_cells);
    let new_records = record_slices(&new_cells);
    if old_records.len() != new_records.len() {
        return None;
    }
    let mut data_changed = false;
    let mut omitted_fields = 0usize;
    let mut patches = Vec::new();
    for (old_record, new_record) in old_records.into_iter().zip(new_records) {
        if new_record.len() < old_record.len() {
            return None;
        }
        data_changed |= old_record
            .iter()
            .zip(new_record)
            .any(|(old_cell, new_cell)| {
                !old_cell.formula && old[old_cell.raw.clone()] != new[new_cell.raw.clone()]
            });
        if new_record.len() == old_record.len() {
            continue;
        }
        if new_record[old_record.len()..]
            .iter()
            .any(|cell| !decoded_field(&new[cell.raw.clone()]).trim().is_empty())
        {
            return None;
        }
        let removal = new_record[old_record.len() - 1].raw.end..new_record.last()?.raw.end;
        patches.push((removal, String::new()));
        omitted_fields =
            omitted_fields.saturating_add(new_record.len().saturating_sub(old_record.len()));
    }
    if !data_changed || patches.is_empty() {
        return None;
    }
    let (rewritten, _) = apply_patches(new, patches)?;
    Some((rewritten, omitted_fields))
}

pub(super) fn serialize_split_trailing_values(
    old: &str,
    new: &str,
    delimiter: u8,
) -> Option<(String, usize)> {
    let old_cells = parse_cells(old, delimiter)?;
    let new_cells = parse_cells(new, delimiter)?;
    let old_records = record_slices(&old_cells);
    let new_records = record_slices(&new_cells);
    if old_records.len() != new_records.len() {
        return None;
    }
    let mut repaired_fields = 0usize;
    let mut patches = Vec::new();
    for (old_record, new_record) in old_records.into_iter().zip(new_records) {
        if old_record.is_empty() || new_record.len() < old_record.len() {
            return None;
        }
        if new_record.len() == old_record.len() {
            continue;
        }
        let trailing_index = old_record.len() - 1;
        let old_trailing = &old_record[trailing_index];
        if old_trailing.formula
            || old_record[..trailing_index]
                .iter()
                .zip(&new_record[..trailing_index])
                .any(|(old_cell, new_cell)| {
                    decoded_field(&old[old_cell.raw.clone()])
                        != decoded_field(&new[new_cell.raw.clone()])
                })
        {
            return None;
        }
        let tail = &new_record[trailing_index..];
        let old_value = decoded_field(&old[old_trailing.raw.clone()]);
        let first_value = decoded_field(&new[tail[0].raw.clone()]);
        if first_value == old_value
            || tail
                .iter()
                .skip(1)
                .any(|cell| decoded_field(&new[cell.raw.clone()]).is_empty())
        {
            return None;
        }
        let delimiter_text = char::from(delimiter).to_string();
        let value = tail
            .iter()
            .map(|cell| decoded_field(&new[cell.raw.clone()]))
            .collect::<Vec<_>>()
            .join(&delimiter_text);
        let range = tail[0].raw.start..tail.last()?.raw.end;
        patches.push((range, serialize_field(&value, delimiter)));
        repaired_fields =
            repaired_fields.saturating_add(new_record.len().saturating_sub(old_record.len()));
    }
    let (rewritten, _) = apply_patches(new, patches)?;
    Some((rewritten, repaired_fields))
}

fn record_slices(cells: &[Cell]) -> Vec<&[Cell]> {
    let mut records = Vec::new();
    let mut start = 0usize;
    while start < cells.len() {
        let record = cells[start].record;
        let mut end = start + 1;
        while end < cells.len() && cells[end].record == record {
            end += 1;
        }
        records.push(&cells[start..end]);
        start = end;
    }
    records
}

fn same_formula_tail(candidate: &str, original: &str) -> bool {
    let original = decoded_field(original);
    let candidate_raw = candidate.trim();
    let candidate = decoded_field(candidate_raw);
    candidate.trim() == original.trim()
        || (!candidate_raw.starts_with('"')
            && candidate.replace("\"\"", "\"").trim() == original.trim())
}

fn serialize_field(value: &str, delimiter: u8) -> String {
    if value
        .bytes()
        .any(|byte| byte == delimiter || matches!(byte, b'"' | b'\r' | b'\n'))
    {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

pub(super) fn apply_patches(
    new: &str,
    mut patches: Vec<(Range<usize>, String)>,
) -> Option<(String, usize)> {
    if patches.is_empty() {
        return None;
    }
    patches.sort_by_key(|(range, _)| range.start);
    let count = patches.len();
    let mut rewritten = new.to_string();
    for (range, original_formula) in patches.into_iter().rev() {
        rewritten.replace_range(range, &original_formula);
    }
    Some((rewritten, count))
}
