pub(super) fn handle_read_subtitle_file_path(
    args: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let path = args["path"].as_str().ok_or("Missing path")?;
    let path = std::path::Path::new(path);
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if extension != "srt" && extension != "vtt" {
        return Err("Only .srt and .vtt subtitle files can be imported".to_string());
    }
    const MAX_SUBTITLE_BYTES: u64 = 10 * 1024 * 1024;
    let metadata = std::fs::metadata(path)
        .map_err(|error| format!("Subtitle file is unavailable: {error}"))?;
    if metadata.len() > MAX_SUBTITLE_BYTES {
        return Err("Subtitle file is too large".to_string());
    }
    let bytes =
        std::fs::read(path).map_err(|error| format!("Failed to read subtitle file: {error}"))?;
    let content = decode_subtitle_text(&bytes);
    let fallback_name = if extension == "vtt" {
        "subtitles.vtt"
    } else {
        "subtitles.srt"
    };
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(fallback_name);
    Ok(serde_json::json!({
        "fileName": file_name,
        "content": content,
        "format": extension,
    }))
}

/// Subtitle files from other tools are often UTF-16 (with a BOM) or carry a
/// UTF-8 BOM. `Encoding::decode` sniffs and strips a BOM; BOM-less text is read
/// as UTF-8, replacing invalid bytes instead of rejecting the whole file.
fn decode_subtitle_text(bytes: &[u8]) -> String {
    let (text, _, _) = encoding_rs::UTF_8.decode(bytes);
    text.into_owned()
}

#[cfg(test)]
mod tests {
    use super::decode_subtitle_text;

    const CUE: &str = "1\n00:00:01,000 --> 00:00:02,000\nHello \u{c548}\n";

    #[test]
    fn utf8_with_or_without_bom_decodes_without_the_bom() {
        assert_eq!(decode_subtitle_text(CUE.as_bytes()), CUE);
        let with_bom = [&[0xEF, 0xBB, 0xBF][..], CUE.as_bytes()].concat();
        assert_eq!(decode_subtitle_text(&with_bom), CUE);
    }

    #[test]
    fn utf16_files_with_a_bom_decode() {
        let le: Vec<u8> = [0xFF, 0xFE]
            .into_iter()
            .chain(CUE.encode_utf16().flat_map(u16::to_le_bytes))
            .collect();
        let be: Vec<u8> = [0xFE, 0xFF]
            .into_iter()
            .chain(CUE.encode_utf16().flat_map(u16::to_be_bytes))
            .collect();
        assert_eq!(decode_subtitle_text(&le), CUE);
        assert_eq!(decode_subtitle_text(&be), CUE);
    }

    #[test]
    fn invalid_utf8_bytes_are_replaced_not_rejected() {
        let text = decode_subtitle_text(b"1\n00:00:01,000 --> 00:00:02,000\ncaf\xE9\n");
        assert!(text.contains("caf\u{FFFD}"));
    }
}
