use std::fs;

pub(super) fn validate_file_name(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || value.encode_utf16().count() > 255 {
        return Err("File name must contain 1 to 255 characters".to_string());
    }
    if value == "."
        || value == ".."
        || value.ends_with([' ', '.'])
        || value
            .chars()
            .any(|character| character.is_control() || r#"<>:"/\|?*"#.contains(character))
    {
        return Err("File name contains characters Windows does not allow".to_string());
    }
    let stem = value
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || stem
            .strip_prefix("COM")
            .or_else(|| stem.strip_prefix("LPT"))
            .is_some_and(|number| number.len() == 1 && matches!(number.as_bytes()[0], b'1'..=b'9'));
    if reserved {
        return Err("File name is reserved by Windows".to_string());
    }
    Ok(value.to_string())
}

pub(super) fn safe_suggested_file_name(
    value: &str,
    fallback_stem: &str,
    required_extension: &str,
) -> String {
    let leaf = value.rsplit(['/', '\\']).next().unwrap_or_default();
    let stem = leaf
        .strip_suffix(&format!(".{required_extension}"))
        .unwrap_or(leaf);
    let cleaned = stem
        .chars()
        .map(|character| {
            if character.is_control() || r#"<>:"/\|?*"#.contains(character) {
                '-'
            } else {
                character
            }
        })
        .take(120)
        .collect::<String>();
    let cleaned = cleaned.trim_matches([' ', '.', '-']);
    let candidate = format!(
        "{}.{}",
        if cleaned.is_empty() {
            fallback_stem
        } else {
            cleaned
        },
        required_extension
    );
    if validate_file_name(&candidate).is_ok() {
        candidate
    } else {
        format!("{fallback_stem}.{required_extension}")
    }
}

#[cfg(windows)]
pub(super) fn metadata_is_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
pub(super) fn metadata_is_reparse(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_reject_traversal_and_windows_devices() {
        for value in ["../outside.mp4", r"..\outside.mp4", "CON.mp4", "bad?.mp4"] {
            assert!(validate_file_name(value).is_err(), "{value}");
        }
        assert_eq!(
            validate_file_name("My recording.mp4").unwrap(),
            "My recording.mp4"
        );
    }

    #[test]
    fn suggestions_are_reduced_to_safe_leaf_names() {
        assert_eq!(
            safe_suggested_file_name(r"..\..\My: captions.srt", "subtitles", "srt"),
            "My- captions.srt"
        );
    }
}
