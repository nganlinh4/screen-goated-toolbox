use std::path::Path;

pub(super) const MAX_INPUT_BYTES: u64 = 4_490_000;

pub(super) fn error(path: &Path) -> Option<&'static str> {
    if std::fs::metadata(path).is_ok_and(|metadata| metadata.len() > MAX_INPUT_BYTES) {
        return Some("image_too_large");
    }
    crate::overlay::creation_source::inspect_image(path)
        .err()
        .map(|_| "image_invalid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_oversized_input_before_decoding() {
        let path =
            std::env::temp_dir().join(format!("svg-input-policy-{}.png", std::process::id()));
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(MAX_INPUT_BYTES + 1).unwrap();
        assert_eq!(error(&path), Some("image_too_large"));
        file.set_len(MAX_INPUT_BYTES).unwrap();
        assert_eq!(error(&path), Some("image_invalid"));
        drop(file);
        std::fs::remove_file(path).unwrap();
    }
}
