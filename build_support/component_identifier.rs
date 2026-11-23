/// Validate a component identifier used as a single Windows path segment.
pub(super) fn valid(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        // Win32 treats trailing periods as insignificant, including `.` and
        // `..`, so they cannot identify a distinct owned component directory.
        && !value.ends_with('.')
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'-' | b'_' | b'.')
        })
}

#[cfg(test)]
mod tests {
    use super::valid;

    #[test]
    fn rejects_dot_path_components_and_windows_trailing_dot_aliases() {
        for unsafe_value in ["", ".", "..", "...", "1.13.2."] {
            assert!(!valid(unsafe_value), "{unsafe_value:?}");
        }
        for supported in ["onnx-directml-runtime", "1.13.2", "2.7.1-cu128-abi2"] {
            assert!(valid(supported), "{supported:?}");
        }
    }
}
