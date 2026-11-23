pub(super) fn normalize_narration_input_text(text: &str) -> Option<String> {
    let edge_trimmed = trim_narration_noise(text);
    let repaired = repair_cp949_mojibake(&edge_trimmed).unwrap_or(edge_trimmed);
    let normalized = trim_narration_noise(&repaired);
    has_speakable_text(&normalized).then_some(normalized)
}

pub(super) fn trim_narration_noise(text: &str) -> String {
    let lines = text
        .lines()
        .map(trim_narration_edge_noise)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    trim_narration_edge_noise(&lines).to_string()
}

pub(super) fn trim_narration_edge_noise(text: &str) -> &str {
    text.trim_matches(|ch: char| {
        ch.is_whitespace()
            || ch == '\u{fffd}'
            || ch == '\u{feff}'
            || matches!(
                ch,
                '?' | '¿'
                    | '¡'
                    | '!'
                    | '"'
                    | '\''
                    | '`'
                    | '*'
                    | '_'
                    | '~'
                    | '|'
                    | '·'
                    | '•'
                    | '♪'
                    | '♫'
                    | '♩'
                    | '♬'
                    | '♭'
                    | '♮'
                    | '♯'
            )
    })
}

pub(super) fn has_speakable_text(text: &str) -> bool {
    text.chars().any(|ch| ch.is_alphanumeric())
}

pub(super) fn repair_cp949_mojibake(text: &str) -> Option<String> {
    if unresolved_mojibake_score(text) < 2 {
        return None;
    }
    let (bytes, _, had_encode_errors) = encoding_rs::EUC_KR.encode(text);
    if had_encode_errors {
        return None;
    }
    let repaired = std::str::from_utf8(&bytes).ok()?.trim();
    if repaired.is_empty() || !has_speakable_text(repaired) {
        return None;
    }
    if unresolved_mojibake_score(repaired) < unresolved_mojibake_score(text) {
        Some(repaired.to_string())
    } else {
        None
    }
}

pub(super) fn unresolved_mojibake_score(text: &str) -> usize {
    text.chars()
        .filter(|ch| {
            ('\u{3400}'..='\u{9fff}').contains(ch)
                || ('\u{ac00}'..='\u{d7af}').contains(ch)
                || *ch == '\u{fffd}'
        })
        .count()
}
