use super::FragmentEvidence;
use crate::api::vision::repetition::squeeze;

fn oracle(text: &str, onset: usize) -> bool {
    let (before, after): (Vec<_>, Vec<_>) = text
        .split_whitespace()
        .map(|token| {
            (
                token.as_ptr() as usize - text.as_ptr() as usize,
                token.to_lowercase(),
            )
        })
        .partition(|(offset, _)| *offset < onset);
    let haystack = squeeze(
        &before
            .iter()
            .map(|(_, word)| word.as_str())
            .collect::<String>(),
    );
    after.windows(2).any(|pair| {
        let joined = squeeze(&format!("{}{}", pair[0].1, pair[1].1));
        joined.chars().count() > pair[0].1.chars().count()
            && !before.iter().any(|(_, word)| word == &pair[0].1)
            && haystack.contains(&joined)
    })
}

#[test]
fn cached_evidence_equals_original_token_predicate() {
    for mut code in 0..6usize.pow(5) {
        let mut text = String::new();
        for _ in 0..5 {
            text.push_str(["ab", "a", "b", "ba", "🌱a", "🌱"][code % 6]);
            text.push(' ');
            code /= 6;
        }
        let evidence = FragmentEvidence::new(&text);
        for onset in text
            .char_indices()
            .map(|(offset, _)| offset)
            .chain([text.len()])
        {
            assert_eq!(
                evidence.at(onset),
                oracle(&text, onset),
                "{text:?} at {onset}"
            );
        }
    }
}
