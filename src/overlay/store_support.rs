//! Shared rules for the overlay's on-disk stores, so every store agrees on
//! the clock and on which ids are valid. Links are refused through
//! `component_registry::is_reparse_point`.

/// The longest job, dispatch or snapshot id any store accepts.
pub(crate) const MAX_IDENTIFIER_BYTES: usize = 160;

/// Wall-clock milliseconds since the Unix epoch, saturating instead of failing.
pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

/// A nonempty ASCII id of letters, digits, `-` and `_`, safe to use as a
/// file or directory name.
pub(crate) fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_IDENTIFIER_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

#[cfg(test)]
mod tests {
    use super::{MAX_IDENTIFIER_BYTES, now_ms, valid_identifier};

    #[test]
    fn identifiers_share_one_length_and_alphabet() {
        assert!(valid_identifier("job-1_A"));
        assert!(valid_identifier(&"a".repeat(MAX_IDENTIFIER_BYTES)));
        for invalid in ["", "a/b", "a.b", "a b", "é"] {
            assert!(!valid_identifier(invalid), "{invalid}");
        }
        assert!(!valid_identifier(&"a".repeat(MAX_IDENTIFIER_BYTES + 1)));
    }

    #[test]
    fn the_clock_is_set() {
        assert!(now_ms() > 1_700_000_000_000);
    }
}
