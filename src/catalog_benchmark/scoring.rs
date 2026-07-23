use unicode_normalization::UnicodeNormalization;

#[derive(Debug)]
pub struct CoordinateScore {
    pub x_1000: f64,
    pub y_1000: f64,
    pub hit: bool,
    pub error_px: f64,
}

pub fn coordinate(
    response: &str,
    width: u32,
    height: u32,
    box_px: [f64; 4],
) -> Option<CoordinateScore> {
    let value = parse_json_object(response)?;
    let x = value.get("x")?.as_f64()?;
    let y = value.get("y")?.as_f64()?;
    if !(0.0..=1000.0).contains(&x) || !(0.0..=1000.0).contains(&y) {
        return None;
    }
    let px = x / 1000.0 * f64::from(width);
    let py = y / 1000.0 * f64::from(height);
    let [bx, by, bw, bh] = box_px;
    let center_x = bx + bw / 2.0;
    let center_y = by + bh / 2.0;
    Some(CoordinateScore {
        x_1000: x,
        y_1000: y,
        hit: px >= bx && px <= bx + bw && py >= by && py <= by + bh,
        error_px: ((px - center_x).powi(2) + (py - center_y).powi(2)).sqrt(),
    })
}

pub fn transcription(response: &str) -> String {
    if let Some(value) = parse_json_object(response)
        && let Some(text) = value.get("text").and_then(serde_json::Value::as_str)
    {
        return text.trim().to_string();
    }
    response
        .trim()
        .trim_start_matches("```text")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim()
        .to_string()
}

pub fn text_similarity(actual: &str, expected: &str) -> f64 {
    let actual: Vec<char> = normalize(actual).chars().collect();
    let expected: Vec<char> = normalize(expected).chars().collect();
    if actual.is_empty() && expected.is_empty() {
        return 1.0;
    }
    let distance = levenshtein(&actual, &expected);
    1.0 - distance as f64 / actual.len().max(expected.len()) as f64
}

pub fn term_coverage(actual: &str, terms: &[String]) -> f64 {
    if terms.is_empty() {
        return 1.0;
    }
    let actual = normalize(actual);
    let found = terms
        .iter()
        .filter(|term| actual.contains(&normalize(term)))
        .count();
    found as f64 / terms.len() as f64
}

pub fn exact_coverage(actual: &str, fragments: &[String]) -> f64 {
    if fragments.is_empty() {
        return 1.0;
    }
    fragments
        .iter()
        .filter(|fragment| actual.contains(fragment.as_str()))
        .count() as f64
        / fragments.len() as f64
}

pub fn forbidden_avoidance(actual: &str, terms: &[String]) -> f64 {
    if terms.is_empty() {
        return 1.0;
    }
    let actual = format!(" {} ", normalize(actual));
    terms
        .iter()
        .filter(|term| !actual.contains(&format!(" {} ", normalize(term))))
        .count() as f64
        / terms.len() as f64
}

pub fn line_count_matches(actual: &str, expected: Option<usize>) -> f64 {
    let Some(expected) = expected else {
        return 1.0;
    };
    f64::from(actual.lines().count() == expected)
}

fn normalize(value: &str) -> String {
    let mut normalized = String::new();
    let mut pending_space = false;
    for character in value.nfkc().flat_map(char::to_lowercase) {
        if character.is_alphanumeric() {
            if pending_space && !normalized.is_empty() {
                normalized.push(' ');
            }
            normalized.push(character);
            pending_space = false;
        } else {
            pending_space = true;
        }
    }
    normalized
}

fn levenshtein(left: &[char], right: &[char]) -> usize {
    let mut previous: Vec<usize> = (0..=right.len()).collect();
    let mut current = vec![0; right.len() + 1];
    for (left_index, left_char) in left.iter().enumerate() {
        current[0] = left_index + 1;
        for (right_index, right_char) in right.iter().enumerate() {
            current[right_index + 1] = (previous[right_index + 1] + 1)
                .min(current[right_index] + 1)
                .min(previous[right_index] + usize::from(left_char != right_char));
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[right.len()]
}

fn parse_json_object(response: &str) -> Option<serde_json::Value> {
    serde_json::from_str(response.trim()).ok().or_else(|| {
        let start = response.find('{')?;
        let end = response.rfind('}')?;
        serde_json::from_str(&response[start..=end]).ok()
    })
}

#[cfg(test)]
mod tests {
    use super::{
        coordinate, exact_coverage, forbidden_avoidance, line_count_matches, term_coverage,
        text_similarity, transcription,
    };

    #[test]
    fn text_scoring_is_unicode_and_punctuation_tolerant() {
        assert_eq!(text_similarity("Ｃafé!", "café"), 1.0);
        assert!(text_similarity("restore only files", "restore files") > 0.7);
        assert_eq!(
            term_coverage(
                "Open [Settings], then Sync",
                &["settings".into(), "sync".into()]
            ),
            1.0
        );
    }

    #[test]
    fn coordinate_scoring_accepts_wrapped_json_and_checks_box() {
        let score = coordinate(
            "answer: {\"x\":250,\"y\":500}",
            400,
            200,
            [90.0, 90.0, 20.0, 20.0],
        )
        .unwrap();
        assert!(score.hit);
        assert_eq!(score.error_px, 0.0);
        assert!(coordinate("{\"x\":1001,\"y\":2}", 10, 10, [0.0, 0.0, 1.0, 1.0]).is_none());
    }

    #[test]
    fn transcription_accepts_json_or_plain_text() {
        assert_eq!(transcription("{\"text\":\"Hello\"}"), "Hello");
        assert_eq!(transcription("```text\nHello\n```"), "Hello");
    }

    #[test]
    fn constraint_scoring_checks_exact_fragments_forbidden_terms_and_lines() {
        assert_eq!(
            exact_coverage(
                "Keep ${BUILD_ID} and <draft_id>",
                &["${BUILD_ID}".into(), "<draft_id>".into()]
            ),
            1.0
        );
        assert_eq!(
            forbidden_avoidance("inspect it", &["literal lid".into()]),
            1.0
        );
        assert_eq!(line_count_matches("one\ntwo", Some(2)), 1.0);
        assert_eq!(line_count_matches("one two", Some(2)), 0.0);
    }
}
