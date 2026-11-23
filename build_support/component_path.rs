/// A bounded, relative Windows payload path; callers choose canonical separators.
pub(crate) fn valid(value: &str) -> bool {
    if value.is_empty() || value.encode_utf16().count() > 512 {
        return false;
    }
    let mut count = 0;
    for part in value.split(['/', '\\']) {
        count += 1;
        if count > 32
            || part.is_empty()
            || part == "."
            || part == ".."
            || part.encode_utf16().count() > 255
            || part.ends_with(['.', ' '])
            || part
                .chars()
                .any(|ch| ch <= '\u{1f}' || "<>:\"|?*".contains(ch))
        {
            return false;
        }
        let stem = part
            .split('.')
            .next()
            .unwrap()
            .trim_end_matches(' ')
            .to_uppercase();
        if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL") {
            return false;
        }
        for prefix in ["COM", "LPT"] {
            if let Some(suffix) = stem.strip_prefix(prefix)
                && matches!(
                    suffix,
                    "1" | "2"
                        | "3"
                        | "4"
                        | "5"
                        | "6"
                        | "7"
                        | "8"
                        | "9"
                        | "\u{b9}"
                        | "\u{b2}"
                        | "\u{b3}"
                )
            {
                return false;
            }
        }
    }
    true
}
