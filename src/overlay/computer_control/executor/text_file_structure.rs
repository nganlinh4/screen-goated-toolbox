//! Format-level invariants for delimited text edits.
//!
//! Exact byte replacement is not enough for tables: a shorter replacement can
//! be written atomically while silently dropping a column or formula. CSV/TSV
//! edits therefore preserve record shape and formula cells by default. An
//! separate explicit capability is required when the requested task genuinely
//! changes them.

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::Path;

#[derive(Clone, Copy)]
enum Format {
    Csv,
    Tsv,
}

impl Format {
    fn for_path(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?;
        if extension.eq_ignore_ascii_case("csv") {
            Some(Self::Csv)
        } else if extension.eq_ignore_ascii_case("tsv") {
            Some(Self::Tsv)
        } else {
            None
        }
    }

    fn delimiter(self) -> char {
        match self {
            Self::Csv => ',',
            Self::Tsv => '\t',
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Tsv => "tsv",
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct FormulaCell {
    record: usize,
    field: usize,
    value: String,
}

struct Profile {
    records: Vec<Vec<String>>,
    field_counts: Vec<usize>,
    formulas: Vec<FormulaCell>,
}

#[derive(Clone, Copy)]
enum Authorization<'a> {
    Preserve,
    Explicit(Option<&'a str>),
}

pub(super) fn validate_preserving_change(
    path: &Path,
    before: &str,
    after: &str,
) -> Result<Option<Value>, Value> {
    validate_change(path, before, after, Authorization::Preserve)
}

pub(super) fn validate_explicit_change(
    path: &Path,
    before: &str,
    after: &str,
    supplied_token: Option<&str>,
) -> Result<Option<Value>, Value> {
    validate_change(path, before, after, Authorization::Explicit(supplied_token))
}

fn validate_change(
    path: &Path,
    before: &str,
    after: &str,
    authorization: Authorization<'_>,
) -> Result<Option<Value>, Value> {
    let Some(format) = Format::for_path(path) else {
        return match authorization {
            Authorization::Preserve => Ok(None),
            Authorization::Explicit(_) => Err(super::failure(
                "ERR_TEXT_FILE_STRUCTURE_UNSUPPORTED",
                Some(path),
                "explicit structural text edits support only CSV and TSV files; use edit_text_file for ordinary text content",
                true,
            )),
        };
    };
    let before_profile = match parse(before, format.delimiter()) {
        Ok(profile) => profile,
        Err(error) => {
            return validate_unreadable_baseline(path, format, before, after, error, authorization);
        }
    };
    let after_profile = match parse(after, format.delimiter()) {
        Ok(profile) => profile,
        Err(error) => {
            let expected_token = change_token(format, before, after);
            let supplied_token = match authorization {
                Authorization::Preserve => None,
                Authorization::Explicit(token) => token,
            };
            if !matches!(authorization, Authorization::Explicit(_))
                || supplied_token != Some(expected_token.as_str())
            {
                return Err(structure_failure(
                    path,
                    "ERR_TEXT_FILE_STRUCTURE_UNREADABLE",
                    match authorization {
                        Authorization::Preserve => {
                            "the proposed ordinary edit makes the delimited text unparsable; correct it, or use edit_text_file_structure only when the user explicitly requested a structural change"
                        }
                        Authorization::Explicit(_) => {
                            "the proposed structural edit makes the delimited text unparsable; correct it, or retry this explicit capability with its content-bound token"
                        }
                    },
                    format,
                    &before_profile,
                    FailureContext {
                        after: None,
                        parse_error: Some(error),
                        confirmation_token: matches!(authorization, Authorization::Explicit(_))
                            .then_some(expected_token.as_str()),
                    },
                ));
            }
            return Ok(Some(json!({
                "format": format.name(),
                "checked": false,
                "reason": error,
                "structural_change_confirmed": true,
            })));
        }
    };
    let shape_preserved = before_profile.field_counts == after_profile.field_counts;
    let formulas_preserved = before_profile.formulas == after_profile.formulas;
    if shape_preserved
        && !formulas_preserved
        && mixed_formula_and_data_change(&before_profile, &after_profile)
    {
        return Err(structure_failure(
            path,
            "ERR_TEXT_FILE_FORMULA_MIXED_EDIT",
            "formula cells changed in the same edit as ordinary data; formulas are opaque preserved data, so retry with their original bytes. An explicitly requested formula change must be a separate formula-only edit",
            format,
            &before_profile,
            FailureContext {
                after: Some(&after_profile),
                parse_error: None,
                confirmation_token: None,
            },
        ));
    }
    let structure_changed = !(shape_preserved && formulas_preserved);
    match authorization {
        Authorization::Preserve if structure_changed => {
            return Err(structure_failure(
                path,
                "ERR_TEXT_FILE_STRUCTURE_CHANGE_REQUIRES_EXPLICIT_TOOL",
                "the proposed ordinary edit changes record shape or formula cells. First correct unintended delimiters or quoting in new_text and retry edit_text_file. Use edit_text_file_structure only when the user explicitly requested the exact row, column, or formula change",
                format,
                &before_profile,
                FailureContext {
                    after: Some(&after_profile),
                    parse_error: None,
                    confirmation_token: None,
                },
            ));
        }
        Authorization::Explicit(_) if !structure_changed => {
            return Err(structure_failure(
                path,
                "ERR_TEXT_FILE_STRUCTURE_NOT_CHANGED",
                "the proposal changes only ordinary content; use edit_text_file so formulas and record shape remain protected",
                format,
                &before_profile,
                FailureContext {
                    after: Some(&after_profile),
                    parse_error: None,
                    confirmation_token: None,
                },
            ));
        }
        Authorization::Explicit(supplied_token) => {
            let expected_token = change_token(format, before, after);
            if supplied_token != Some(expected_token.as_str()) {
                return Err(structure_failure(
                    path,
                    "ERR_TEXT_FILE_STRUCTURE_CHANGE",
                    "the explicit proposal changes record shape or formula cells. Its token identifies bytes, not permission. Retry with that token only when the user requested this exact structural effect; otherwise correct delimiters or quoting and use edit_text_file",
                    format,
                    &before_profile,
                    FailureContext {
                        after: Some(&after_profile),
                        parse_error: None,
                        confirmation_token: Some(&expected_token),
                    },
                ));
            }
        }
        Authorization::Preserve => {}
    }
    Ok(Some(json!({
        "format": format.name(),
        "checked": true,
        "structural_change_confirmed": matches!(authorization, Authorization::Explicit(_)),
        "record_shape_preserved": shape_preserved,
        "formulas_preserved": formulas_preserved,
        "before_record_count": before_profile.field_counts.len(),
        "after_record_count": after_profile.field_counts.len(),
        "before_formula_count": before_profile.formulas.len(),
        "after_formula_count": after_profile.formulas.len(),
    })))
}

fn validate_unreadable_baseline(
    path: &Path,
    format: Format,
    before: &str,
    after: &str,
    parse_error: &str,
    authorization: Authorization<'_>,
) -> Result<Option<Value>, Value> {
    let token = change_token(format, before, after);
    if matches!(authorization, Authorization::Explicit(Some(supplied)) if supplied == token) {
        return Ok(Some(json!({
            "format": format.name(),
            "checked": false,
            "reason": parse_error,
            "structural_change_confirmed": true,
        })));
    }
    let mut value = super::failure(
        "ERR_TEXT_FILE_STRUCTURE_BASELINE_UNREADABLE",
        Some(path),
        match authorization {
            Authorization::Preserve => {
                "the original delimited text is not parseable, so an ordinary edit cannot prove that shape and formulas are preserved"
            }
            Authorization::Explicit(_) => {
                "the original delimited text is not parseable; retry the explicit structural capability with its content-bound token only when changing this malformed structure is intentional"
            }
        },
        true,
    );
    if let Some(fields) = value.as_object_mut() {
        fields.insert("format".to_string(), json!(format.name()));
        fields.insert("parse_error".to_string(), json!(parse_error));
        if matches!(authorization, Authorization::Explicit(_)) {
            fields.insert("structural_change_token".to_string(), json!(token));
        }
    }
    Err(value)
}

fn mixed_formula_and_data_change(before: &Profile, after: &Profile) -> bool {
    let formula_positions = before
        .formulas
        .iter()
        .chain(&after.formulas)
        .map(|formula| (formula.record, formula.field))
        .collect::<HashSet<_>>();
    before.records.iter().enumerate().any(|(record, fields)| {
        fields.iter().enumerate().any(|(field, value)| {
            !formula_positions.contains(&(record, field)) && after.records[record][field] != *value
        })
    })
}

struct FailureContext<'a> {
    after: Option<&'a Profile>,
    parse_error: Option<&'a str>,
    confirmation_token: Option<&'a str>,
}

fn structure_failure(
    path: &Path,
    code: &str,
    message: &str,
    format: Format,
    before: &Profile,
    context: FailureContext<'_>,
) -> Value {
    let mut value = super::failure(code, Some(path), message, true);
    if let Some(fields) = value.as_object_mut() {
        fields.insert("format".to_string(), json!(format.name()));
        fields.insert(
            "before_record_count".to_string(),
            json!(before.field_counts.len()),
        );
        fields.insert(
            "before_formula_count".to_string(),
            json!(before.formulas.len()),
        );
        fields.insert(
            "before_field_counts".to_string(),
            json!(bounded_counts(&before.field_counts)),
        );
        if let Some(after) = context.after {
            fields.insert(
                "after_record_count".to_string(),
                json!(after.field_counts.len()),
            );
            fields.insert(
                "after_formula_count".to_string(),
                json!(after.formulas.len()),
            );
            fields.insert(
                "after_field_counts".to_string(),
                json!(bounded_counts(&after.field_counts)),
            );
            fields.insert(
                "shape_mismatches".to_string(),
                json!(shape_mismatches(&before.field_counts, &after.field_counts)),
            );
            if code == "ERR_TEXT_FILE_STRUCTURE_CHANGE_REQUIRES_EXPLICIT_TOOL" {
                fields.insert(
                    "required_next_step".to_string(),
                    json!("correct_delimited_content_then_retry_ordinary_edit"),
                );
                fields.insert(
                    "instruction".to_string(),
                    json!(
                        "Inspect shape_mismatches. Quote or remove unintended delimiters in new_text, preserve the original field/formula shape, and retry edit_text_file. Do not use edit_text_file_structure unless the user requested that exact structural effect."
                    ),
                );
            }
            if code == "ERR_TEXT_FILE_STRUCTURE_CHANGE" {
                fields.insert(
                    "required_next_step".to_string(),
                    json!("confirm_exact_structural_intent_or_repair_ordinary_content"),
                );
                fields.insert(
                    "instruction".to_string(),
                    json!(
                        "The token does not grant permission. If the user requested shape/formula preservation, do not retry this proposal: correct shape_mismatches in new_text and use edit_text_file."
                    ),
                );
            }
        }
        if let Some(error) = context.parse_error {
            fields.insert("parse_error".to_string(), json!(error));
        }
        if let Some(token) = context.confirmation_token {
            fields.insert("structural_change_token".to_string(), json!(token));
        }
    }
    value
}

fn change_token(format: Format, before: &str, after: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(b"sgt-edit-text-file-structure-v2\0");
    digest.update(format.name().as_bytes());
    digest.update(b"\0");
    digest.update((before.len() as u64).to_le_bytes());
    digest.update(before.as_bytes());
    digest.update((after.len() as u64).to_le_bytes());
    digest.update(after.as_bytes());
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn bounded_counts(counts: &[usize]) -> Vec<usize> {
    counts.iter().copied().take(128).collect()
}

fn shape_mismatches(before: &[usize], after: &[usize]) -> Vec<Value> {
    (0..before.len().max(after.len()))
        .filter_map(|index| {
            let before_fields = before.get(index).copied();
            let after_fields = after.get(index).copied();
            (before_fields != after_fields).then(|| {
                json!({
                    "record_number": index + 1,
                    "before_fields": before_fields,
                    "after_fields": after_fields,
                })
            })
        })
        .take(16)
        .collect()
}

fn parse(text: &str, delimiter: char) -> Result<Profile, &'static str> {
    let mut records = Vec::<Vec<String>>::new();
    let mut record = Vec::<String>::new();
    let mut field = String::new();
    let mut chars = text.chars().peekable();
    let mut in_quotes = false;
    let mut record_started = false;

    while let Some(ch) = chars.next() {
        if in_quotes {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(ch);
            }
            record_started = true;
            continue;
        }
        match ch {
            '"' if field.is_empty() => {
                in_quotes = true;
                record_started = true;
            }
            value if value == delimiter => {
                record.push(std::mem::take(&mut field));
                record_started = true;
            }
            '\r' | '\n' => {
                if ch == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
                record_started = false;
            }
            value => {
                field.push(value);
                record_started = true;
            }
        }
    }
    if in_quotes {
        return Err("unterminated quoted field");
    }
    if record_started || !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }

    let mut formulas = Vec::new();
    let field_counts = records
        .iter()
        .enumerate()
        .map(|(record_index, fields)| {
            for (field_index, value) in fields.iter().enumerate() {
                if value.trim_start().starts_with('=') {
                    formulas.push(FormulaCell {
                        record: record_index,
                        field: field_index,
                        value: value.clone(),
                    });
                }
            }
            fields.len()
        })
        .collect();
    Ok(Profile {
        records,
        field_counts,
        formulas,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_quoted_commas_newlines_and_formulas() {
        let profile = parse("a,b,c\r\n1,\"two, too\",\"=SUM(A1,B1)\"\r\n", ',').unwrap();
        assert_eq!(profile.field_counts, vec![3, 3]);
        assert_eq!(profile.formulas.len(), 1);
        assert_eq!(profile.formulas[0].field, 2);
    }

    #[test]
    fn detects_unterminated_quotes() {
        assert_eq!(parse("a,\"b", ',').err(), Some("unterminated quoted field"));
    }

    #[test]
    fn data_and_formula_changes_cannot_share_one_edit() {
        let path = Path::new("table.csv");
        let before = "name,value,formula\nrow,old,\"=A1\"\n";
        let after = "name,value,formula\nrow,new,\"=B1\"\n";
        let error = validate_explicit_change(path, before, after, None).unwrap_err();
        assert_eq!(error["code"], "ERR_TEXT_FILE_FORMULA_MIXED_EDIT");
        assert!(error.get("structural_change_token").is_none());
        let still_rejected =
            validate_explicit_change(path, before, after, Some("invented")).unwrap_err();
        assert_eq!(still_rejected["code"], "ERR_TEXT_FILE_FORMULA_MIXED_EDIT");
    }

    #[test]
    fn explicit_formula_only_edit_uses_content_bound_confirmation() {
        let path = Path::new("table.csv");
        let before = "name,value,formula\nrow,old,\"=A1\"\n";
        let after = "name,value,formula\nrow,old,\"=B1\"\n";
        let error = validate_explicit_change(path, before, after, None).unwrap_err();
        assert_eq!(error["code"], "ERR_TEXT_FILE_STRUCTURE_CHANGE");
        assert_eq!(
            error["required_next_step"],
            "confirm_exact_structural_intent_or_repair_ordinary_content"
        );
        let token = error["structural_change_token"].as_str().unwrap();
        let audit = validate_explicit_change(path, before, after, Some(token))
            .unwrap()
            .unwrap();
        assert_eq!(audit["structural_change_confirmed"], true);
        assert_eq!(audit["formulas_preserved"], false);
    }

    #[test]
    fn ordinary_field_drift_routes_to_quote_repair_before_structural_editing() {
        let path = Path::new("table.csv");
        let before = "name,note,formula\nrow,old,\"=A2\"\n";
        let after = "name,note,formula\nrow,new, unquoted detail,\"=A2\"\n";
        let error = validate_preserving_change(path, before, after).unwrap_err();

        assert_eq!(
            error["code"],
            "ERR_TEXT_FILE_STRUCTURE_CHANGE_REQUIRES_EXPLICIT_TOOL"
        );
        assert_eq!(
            error["required_next_step"],
            "correct_delimited_content_then_retry_ordinary_edit"
        );
        assert_eq!(
            error["shape_mismatches"],
            json!([{"record_number": 2, "before_fields": 3, "after_fields": 4}])
        );
        assert!(
            error["instruction"]
                .as_str()
                .unwrap()
                .contains("retry edit_text_file")
        );
        assert!(error.get("structural_change_token").is_none());
    }
}
