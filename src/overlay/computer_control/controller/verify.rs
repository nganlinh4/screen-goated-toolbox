//! Read-back verification — did the action actually land?
//!
//! After a Fill/Set the controller reads the element's value (and form-validity)
//! back from ground truth and compares it to what was requested. This is the move
//! that makes the harness reliable instead of hopeful: a fill that silently no-ops
//! (wrong target, field not focused, surface ignored the input) is caught HERE,
//! in code, and reported to the model — instead of being assumed successful.

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

/// Compare a requested fill value against what was read back. Lenient on
/// surrounding whitespace and on fields that reformat (the read-back containing
/// the requested text counts as confirmed — e.g. a phone field that adds spacing).
pub fn verify_fill(expected: &str, rb: &ReadBack) -> VerifyOutcome {
    if let Some(msg) = rb.validity.as_deref().filter(|m| !m.is_empty()) {
        return VerifyOutcome::Invalid {
            message: msg.to_string(),
        };
    }
    match &rb.value {
        Some(got) => {
            let (g, e) = (got.trim(), expected.trim());
            if g == e || (!e.is_empty() && g.contains(e)) {
                VerifyOutcome::Confirmed
            } else {
                VerifyOutcome::Mismatch {
                    expected: e.to_string(),
                    got: g.to_string(),
                }
            }
        }
        None => VerifyOutcome::Unknown,
    }
}
