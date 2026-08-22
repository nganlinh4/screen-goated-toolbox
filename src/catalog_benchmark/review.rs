use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use super::report::{Attempt, AttemptKey};

const REVIEW_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReviewVerdict {
    Pass,
    Partial,
    Fail,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct HumanReview {
    pub suite: String,
    pub model_id: String,
    pub round: u8,
    pub case_id: String,
    pub verdict: Option<ReviewVerdict>,
    /// Human quality judgment from 1 (unusable) to 5 (excellent).
    pub rating: Option<u8>,
    /// One judgment per authored rubric item. `null` means not reviewed yet.
    pub rubric_checks: Vec<Option<bool>>,
    pub notes: String,
    /// Immutable context copied into the template for convenient offline review.
    pub response: String,
    pub reference: Option<String>,
    pub rubric: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ReviewFile {
    version: u32,
    reviews: Vec<HumanReview>,
}

#[derive(Default)]
pub struct ReviewState {
    entries: BTreeMap<AttemptKey, HumanReview>,
}

impl ReviewState {
    pub fn score(&self, attempt: &Attempt) -> Option<f64> {
        let review = self.entries.get(&attempt_key(attempt))?;
        complete_review(review, attempt).then(|| f64::from(review.rating.unwrap()) / 5.0)
    }

    pub fn counts(&self, attempts: &[Attempt]) -> (usize, usize) {
        let required = attempts
            .iter()
            .filter(|attempt| review_is_required(attempt))
            .count();
        let completed = attempts
            .iter()
            .filter(|attempt| {
                review_is_required(attempt)
                    && self
                        .entries
                        .get(&attempt_key(attempt))
                        .is_some_and(|review| complete_review(review, attempt))
            })
            .count();
        (completed, required)
    }
}

pub fn load(output: &Path, attempts: &[Attempt]) -> Result<ReviewState> {
    let path = output.join("reviews.json");
    if !path.exists() {
        return Ok(ReviewState::default());
    }
    let file: ReviewFile = serde_json::from_slice(
        &std::fs::read(&path).with_context(|| format!("read {}", path.display()))?,
    )
    .with_context(|| format!("parse {}", path.display()))?;
    ensure!(file.version == REVIEW_VERSION, "unsupported review version");
    let expected = attempts
        .iter()
        .filter(|attempt| review_is_required(attempt))
        .map(attempt_key)
        .collect::<BTreeSet<_>>();
    let mut entries = BTreeMap::new();
    for review in file.reviews {
        let key = review_key(&review);
        ensure!(
            expected.contains(&key),
            "review names an unknown attempt: {key:?}"
        );
        ensure!(
            entries.insert(key.clone(), review).is_none(),
            "duplicate review: {key:?}"
        );
    }
    Ok(ReviewState { entries })
}

pub fn write_template(output: &Path, attempts: &[Attempt]) -> Result<()> {
    let reviews = attempts
        .iter()
        .filter(|attempt| review_is_required(attempt))
        .map(|attempt| HumanReview {
            suite: attempt.suite.clone(),
            model_id: attempt.model_id.clone(),
            round: attempt.round,
            case_id: attempt.case_id.clone(),
            verdict: None,
            rating: None,
            rubric_checks: vec![None; attempt.rubric.len()],
            notes: String::new(),
            response: attempt.response.clone().unwrap_or_default(),
            reference: attempt.reference.clone(),
            rubric: attempt.rubric.clone(),
        })
        .collect::<Vec<_>>();
    if reviews.is_empty() {
        return Ok(());
    }
    let file = ReviewFile {
        version: REVIEW_VERSION,
        reviews,
    };
    std::fs::write(
        output.join("review-template.json"),
        serde_json::to_vec_pretty(&file)?,
    )?;
    std::fs::write(output.join("human-review.md"), markdown(&file))?;
    Ok(())
}

fn complete_review(review: &HumanReview, attempt: &Attempt) -> bool {
    review.verdict.is_some()
        && review
            .rating
            .is_some_and(|rating| (1..=5).contains(&rating))
        && review.rubric_checks.len() == attempt.rubric.len()
        && review.rubric_checks.iter().all(Option::is_some)
        && review.response == attempt.response.as_deref().unwrap_or_default()
        && review.reference == attempt.reference
        && review.rubric == attempt.rubric
}

fn review_is_required(attempt: &Attempt) -> bool {
    attempt.status == "success" && attempt.manual_review_required
}

fn attempt_key(attempt: &Attempt) -> AttemptKey {
    (
        attempt.suite.clone(),
        attempt.model_id.clone(),
        attempt.round,
        attempt.case_id.clone(),
    )
}

fn review_key(review: &HumanReview) -> AttemptKey {
    (
        review.suite.clone(),
        review.model_id.clone(),
        review.round,
        review.case_id.clone(),
    )
}

fn markdown(file: &ReviewFile) -> String {
    let mut output = String::from(
        "# Catalog benchmark human review\n\nCopy `review-template.json` to `reviews.json`. For every entry, set `verdict`, a 1–5 `rating`, every `rubric_checks` value, and optional notes. Automatic metrics are triage aids only.\n\n",
    );
    for review in &file.reviews {
        output.push_str(&format!(
            "## {} · {} · {}\n\nReference:\n\n```text\n{}\n```\n\nResponse:\n\n```text\n{}\n```\n\nRubric:\n\n",
            review.model_id,
            review.case_id,
            review.suite,
            review.reference.as_deref().unwrap_or("(none)"),
            review.response,
        ));
        for item in &review.rubric {
            output.push_str(&format!("- [ ] {item}\n"));
        }
        output.push('\n');
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incomplete_human_judgment_never_becomes_a_score() {
        let attempt = Attempt {
            suite: "text".into(),
            round: 1,
            difficulty: 1,
            case_id: "case".into(),
            model_id: "model".into(),
            model_name: "api/model".into(),
            provider: "provider".into(),
            reasoning_policy: "none".into(),
            status: "success".into(),
            latency_ms: 10,
            output_chars: Some(2),
            end_to_end_chars_per_second: Some(200.0),
            score: None,
            strict_pass: None,
            response: Some("ok".into()),
            error: None,
            details: serde_json::json!({}),
            reference: Some("ok".into()),
            rubric: vec!["correct".into()],
            manual_review_required: true,
        };
        let mut state = ReviewState::default();
        state.entries.insert(
            attempt_key(&attempt),
            HumanReview {
                suite: "text".into(),
                model_id: "model".into(),
                round: 1,
                case_id: "case".into(),
                verdict: Some(ReviewVerdict::Pass),
                rating: Some(5),
                rubric_checks: vec![None],
                notes: String::new(),
                response: "ok".into(),
                reference: Some("ok".into()),
                rubric: vec!["correct".into()],
            },
        );
        assert_eq!(state.score(&attempt), None);
        assert_eq!(state.counts(&[attempt]), (0, 1));
    }
}
