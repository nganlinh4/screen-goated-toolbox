//! Read-back verification — did the action actually land?
//!
//! After a Fill/Set the controller reads the element's value (and form-validity)
//! back from ground truth and compares it to what was requested. This is the move
//! that makes the harness reliable instead of hopeful: a fill that silently no-ops
//! (wrong target, field not focused, surface ignored the input) is caught HERE,
//! in code, and reported to the model — instead of being assumed successful.

use unicode_normalization::UnicodeNormalization;

/// What a surface read back for an element after acting on it.
#[derive(Debug, Default)]
pub struct ReadBack {
    pub value: Option<String>,
    /// The form-validation message, if the field is now invalid ("" / None = valid).
    pub validity: Option<String>,
}

/// The verdict on a Fill/Set, derived from the read-back.
pub enum VerifyOutcome {
    Confirmed,
    Mismatch { expected: String, got: String },
    Invalid { message: String },
    Unknown,
}

impl VerifyOutcome {
    pub fn is_ok(&self) -> bool {
        matches!(self, VerifyOutcome::Confirmed)
    }

    /// One line the model reads to know whether the action took.
    pub fn describe(&self) -> String {
        match self {
            VerifyOutcome::Confirmed => "confirmed; the field now holds the value".to_string(),
            VerifyOutcome::Mismatch { expected, got } => format!(
                "MISMATCH: requested {expected:?} but the field reads {got:?}; the input likely did NOT register \
(wrong target, or it wasn't focused). Do not assume success — click the field first, or pick a different target."
            ),
            VerifyOutcome::Invalid { message } => {
                format!("the field reports invalid input: {message}")
            }
            VerifyOutcome::Unknown => {
                "could not read the field back to confirm; proceed, but verify the result"
                    .to_string()
            }
        }
    }
}

/// Compare a requested fill value against what was read back. Unicode form,
/// line endings, and surrounding whitespace are normalized, but extra or
/// missing content is never accepted.
pub fn verify_fill(expected: &str, rb: &ReadBack) -> VerifyOutcome {
    if let Some(msg) = rb.validity.as_deref().filter(|m| !m.is_empty()) {
        return VerifyOutcome::Invalid {
            message: msg.to_string(),
        };
    }
    match &rb.value {
        Some(got) => {
            let (g, e) = (normalize_exact(got), normalize_exact(expected));
            if g == e {
                VerifyOutcome::Confirmed
            } else {
                VerifyOutcome::Mismatch {
                    expected: e,
                    got: g,
                }
            }
        }
        None => VerifyOutcome::Unknown,
    }
}

fn normalize_exact(value: &str) -> String {
    value
        .nfkc()
        .collect::<String>()
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::{ReadBack, VerifyOutcome, verify_fill};

    #[test]
    fn a_superset_is_not_a_confirmed_fill() {
        let result = verify_fill(
            "1",
            &ReadBack {
                value: Some("10".to_string()),
                validity: None,
            },
        );
        assert!(matches!(result, VerifyOutcome::Mismatch { .. }));
    }

    #[test]
    fn harmless_unicode_and_line_ending_forms_compare_equal() {
        let result = verify_fill(
            "Ａ\r\nvalue",
            &ReadBack {
                value: Some("A\nvalue".to_string()),
                validity: None,
            },
        );
        assert!(matches!(result, VerifyOutcome::Confirmed));
    }
}
