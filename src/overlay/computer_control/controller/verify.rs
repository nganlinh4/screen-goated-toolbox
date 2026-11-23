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
    /// Display text for a selected browser option, when applicable.
    pub selected_text: Option<String>,
    /// The provider explicitly identifies a protected field whose value is unavailable.
    pub protected: bool,
    /// The form-validation message, if the field is now invalid ("" / None = valid).
    pub validity: Option<String>,
}

pub fn verify_select(expected: &str, rb: &ReadBack) -> VerifyOutcome {
    if let Some(message) = rb.validity.as_deref().filter(|message| !message.is_empty()) {
        return VerifyOutcome::Invalid {
            message: message.to_string(),
        };
    }
    let expected_normalized = normalize_option(expected);
    if [&rb.value, &rb.selected_text]
        .into_iter()
        .flatten()
        .any(|value| normalize_option(value) == expected_normalized)
    {
        VerifyOutcome::Confirmed
    } else if let Some(got) = rb.selected_text.as_ref().or(rb.value.as_ref()) {
        VerifyOutcome::Mismatch {
            expected: expected.to_string(),
            got: got.clone(),
        }
    } else {
        VerifyOutcome::Unknown
    }
}

pub(super) fn normalize_option(value: &str) -> String {
    value
        .nfkc()
        .flat_map(char::to_lowercase)
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The verdict on a Fill/Set, derived from the read-back.
pub enum VerifyOutcome {
    Confirmed,
    Mismatch { expected: String, got: String },
    Invalid { message: String },
    Protected,
    Unknown,
}

impl VerifyOutcome {
    pub fn is_ok(&self) -> bool {
        matches!(self, VerifyOutcome::Confirmed)
    }

    pub fn permits_progress(&self) -> bool {
        matches!(self, VerifyOutcome::Confirmed | VerifyOutcome::Protected)
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
                "could not read the field back to confirm; observe again before continuing"
                    .to_string()
            }
            VerifyOutcome::Protected => {
                "the field is protected; input was dispatched but its value cannot be verified"
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
    if rb.protected {
        return VerifyOutcome::Protected;
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
    use super::{ReadBack, VerifyOutcome, verify_fill, verify_select};

    #[test]
    fn protected_readback_is_not_confused_with_missing_or_invalid_readback() {
        let protected = verify_fill(
            "secret",
            &ReadBack {
                protected: true,
                ..ReadBack::default()
            },
        );
        assert!(protected.permits_progress());
        assert!(!protected.is_ok());
        assert!(!protected.describe().contains("secret"));
        let missing = verify_fill("secret", &ReadBack::default());
        assert!(!missing.permits_progress());
        let invalid = verify_fill(
            "secret",
            &ReadBack {
                protected: true,
                validity: Some("invalid".into()),
                ..ReadBack::default()
            },
        );
        assert!(matches!(invalid, VerifyOutcome::Invalid { .. }));
    }

    #[test]
    fn select_verifies_label_or_value_with_option_normalization() {
        let read_back = ReadBack {
            value: Some("ca".to_string()),
            selected_text: Some("  Canada  ".to_string()),
            protected: false,
            validity: None,
        };
        assert!(matches!(
            verify_select("CANADA", &read_back),
            VerifyOutcome::Confirmed
        ));
        assert!(matches!(
            verify_select("ca", &read_back),
            VerifyOutcome::Confirmed
        ));
        assert!(matches!(
            verify_select("Canada West", &read_back),
            VerifyOutcome::Mismatch { .. }
        ));
        assert!(matches!(
            verify_fill("CANADA", &read_back),
            VerifyOutcome::Mismatch { .. }
        ));
    }

    #[test]
    fn a_superset_is_not_a_confirmed_fill() {
        let result = verify_fill(
            "1",
            &ReadBack {
                value: Some("10".to_string()),
                selected_text: None,
                protected: false,
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
                selected_text: None,
                protected: false,
                validity: None,
            },
        );
        assert!(matches!(result, VerifyOutcome::Confirmed));
    }
}
