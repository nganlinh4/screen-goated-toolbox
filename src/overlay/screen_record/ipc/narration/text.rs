use super::*;

pub(super) fn prepare_narration_tts_text(text: &str, method: &TtsMethod) -> String {
    let trimmed = text.trim();
    if *method != TtsMethod::MagpieMultilingual || trimmed.is_empty() {
        return trimmed.to_string();
    }
    if trimmed
        .chars()
        .rev()
        .find(|ch| !ch.is_whitespace() && !matches!(ch, '"' | '\'' | ')' | ']' | '}'))
        .is_some_and(is_sentence_terminal_punctuation)
    {
        trimmed.to_string()
    } else {
        format!("{trimmed}.")
    }
}

pub(super) fn is_sentence_terminal_punctuation(ch: char) -> bool {
    matches!(ch, '.' | '!' | '?' | '…' | '。' | '！' | '？')
}

pub(super) fn normalize_narration_input_text(text: &str, method: &TtsMethod) -> Option<String> {
    let edge_trimmed = trim_narration_noise(text);
    let repaired = repair_cp949_mojibake(&edge_trimmed);
    let repaired_from_mojibake = repaired.is_some();
    let repaired = repaired.unwrap_or(edge_trimmed);
    let normalized = trim_narration_noise(&repaired);
    if !has_speakable_text(&normalized) {
        return None;
    }
    if matches!(
        method,
        TtsMethod::VieneuTts
            | TtsMethod::Supertonic
            | TtsMethod::MagpieMultilingual
            | TtsMethod::StepAudioEditX
    ) && (unresolved_mojibake_score(&normalized) >= 3
        || (repaired_from_mojibake && unresolved_placeholder_score(&normalized) > 0))
    {
        return None;
    }
    Some(normalized)
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

pub(super) fn unresolved_placeholder_score(text: &str) -> usize {
    let chars: Vec<char> = text.chars().collect();
    chars
        .iter()
        .enumerate()
        .filter(|(index, ch)| {
            if **ch != '?' {
                return false;
            }
            let prev_is_word = chars[..*index]
                .iter()
                .rev()
                .find(|candidate| !candidate.is_whitespace())
                .is_some_and(|candidate| candidate.is_alphanumeric());
            let next_is_word = chars[*index + 1..]
                .iter()
                .find(|candidate| !candidate.is_whitespace())
                .is_some_and(|candidate| candidate.is_alphanumeric());
            prev_is_word && next_is_word
        })
        .count()
}
