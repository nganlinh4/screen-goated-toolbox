use serde::Serialize;

use crate::catalog_benchmark::manifest::LocalizationCase;
use crate::overlay::screen_translate::contract::TranslationDocument;
use crate::overlay::screen_translate::geometry::{PixelRegion, normalized_region};

#[derive(Clone, Debug, Serialize)]
pub(super) struct MatchScore {
    pub gold_index: usize,
    pub prediction_index: usize,
    pub text_similarity: f64,
    pub raw_iou: f64,
    pub raw_gold_coverage: f64,
    pub raw_overpaint_ratio: f64,
    pub painted_iou: f64,
    pub painted_gold_coverage: f64,
    pub painted_overpaint_ratio: f64,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct Metrics {
    pub expected_regions: usize,
    pub predicted_regions: usize,
    pub matched_regions: usize,
    pub region_recall: f64,
    pub region_precision: f64,
    pub region_f1: f64,
    pub mean_text_similarity: f64,
    pub raw_mean_iou: f64,
    pub raw_mean_gold_coverage: f64,
    pub raw_mean_overpaint_ratio: f64,
    pub painted_mean_iou: f64,
    pub painted_mean_gold_coverage: f64,
    pub painted_mean_overpaint_ratio: f64,
    pub expansion_overpaint_delta: f64,
}

pub(super) struct Evaluation {
    pub raw: Vec<PixelRegion>,
    pub painted: Vec<PixelRegion>,
    pub matches: Vec<MatchScore>,
    pub metrics: Metrics,
}

#[derive(Clone, Copy)]
struct Candidate {
    gold_index: usize,
    prediction_index: usize,
    quality: f64,
    text_similarity: f64,
}

pub(super) fn evaluate(
    case: &LocalizationCase,
    document: &TranslationDocument,
    image_width: u32,
    image_height: u32,
) -> Evaluation {
    let raw = document
        .regions
        .iter()
        .map(|region| normalized_region(region.bounds, image_width, image_height))
        .collect::<Vec<_>>();
    // The erase mask is now independent of the translated-text layout box.
    // Painting therefore stays on the detected source region.
    let painted = raw.clone();

    let mut candidates = Vec::new();
    for (gold_index, gold) in case.regions.iter().enumerate() {
        let gold_rect = pixel_region(gold.box_px);
        for (prediction_index, prediction) in document.regions.iter().enumerate() {
            let text_similarity =
                super::super::scoring::text_similarity(&prediction.source_text, &gold.source_text);
            if text_similarity < 0.55 {
                continue;
            }
            let spatial = intersection_over_union(gold_rect, raw[prediction_index]);
            candidates.push(Candidate {
                gold_index,
                prediction_index,
                quality: 0.75 * text_similarity + 0.25 * spatial,
                text_similarity,
            });
        }
    }
    candidates.sort_by(|left, right| right.quality.total_cmp(&left.quality));

    let mut used_gold = vec![false; case.regions.len()];
    let mut used_predictions = vec![false; document.regions.len()];
    let mut matches = Vec::new();
    for candidate in candidates {
        if used_gold[candidate.gold_index] || used_predictions[candidate.prediction_index] {
            continue;
        }
        used_gold[candidate.gold_index] = true;
        used_predictions[candidate.prediction_index] = true;
        let gold = pixel_region(case.regions[candidate.gold_index].box_px);
        let raw_rect = raw[candidate.prediction_index];
        let painted_rect = painted[candidate.prediction_index];
        matches.push(MatchScore {
            gold_index: candidate.gold_index,
            prediction_index: candidate.prediction_index,
            text_similarity: candidate.text_similarity,
            raw_iou: intersection_over_union(gold, raw_rect),
            raw_gold_coverage: gold_coverage(gold, raw_rect),
            raw_overpaint_ratio: overpaint_ratio(gold, raw_rect),
            painted_iou: intersection_over_union(gold, painted_rect),
            painted_gold_coverage: gold_coverage(gold, painted_rect),
            painted_overpaint_ratio: overpaint_ratio(gold, painted_rect),
        });
    }
    matches.sort_by_key(|score| score.gold_index);

    let recall = ratio(matches.len(), case.regions.len());
    let precision = ratio(matches.len(), document.regions.len());
    let f1 = if recall + precision > 0.0 {
        2.0 * recall * precision / (recall + precision)
    } else {
        0.0
    };
    let raw_overpaint = mean(&matches, |score| score.raw_overpaint_ratio);
    let painted_overpaint = mean(&matches, |score| score.painted_overpaint_ratio);
    let metrics = Metrics {
        expected_regions: case.regions.len(),
        predicted_regions: document.regions.len(),
        matched_regions: matches.len(),
        region_recall: recall,
        region_precision: precision,
        region_f1: f1,
        mean_text_similarity: mean(&matches, |score| score.text_similarity),
        raw_mean_iou: mean(&matches, |score| score.raw_iou),
        raw_mean_gold_coverage: mean(&matches, |score| score.raw_gold_coverage),
        raw_mean_overpaint_ratio: raw_overpaint,
        painted_mean_iou: mean(&matches, |score| score.painted_iou),
        painted_mean_gold_coverage: mean(&matches, |score| score.painted_gold_coverage),
        painted_mean_overpaint_ratio: painted_overpaint,
        expansion_overpaint_delta: painted_overpaint - raw_overpaint,
    };
    Evaluation {
        raw,
        painted,
        matches,
        metrics,
    }
}

fn pixel_region([x, y, width, height]: [u32; 4]) -> PixelRegion {
    PixelRegion {
        x,
        y,
        width,
        height,
    }
}

fn intersection(left: PixelRegion, right: PixelRegion) -> u64 {
    let x1 = left.x.max(right.x);
    let y1 = left.y.max(right.y);
    let x2 = (left.x + left.width).min(right.x + right.width);
    let y2 = (left.y + left.height).min(right.y + right.height);
    u64::from(x2.saturating_sub(x1)) * u64::from(y2.saturating_sub(y1))
}

fn area(region: PixelRegion) -> u64 {
    u64::from(region.width) * u64::from(region.height)
}

fn intersection_over_union(left: PixelRegion, right: PixelRegion) -> f64 {
    let overlap = intersection(left, right);
    let union = area(left) + area(right) - overlap;
    ratio(overlap as usize, union as usize)
}

fn gold_coverage(gold: PixelRegion, prediction: PixelRegion) -> f64 {
    intersection(gold, prediction) as f64 / area(gold).max(1) as f64
}

fn overpaint_ratio(gold: PixelRegion, prediction: PixelRegion) -> f64 {
    let outside = area(prediction).saturating_sub(intersection(gold, prediction));
    outside as f64 / area(gold).max(1) as f64
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn mean(scores: &[MatchScore], value: impl Fn(&MatchScore) -> f64) -> f64 {
    if scores.is_empty() {
        return 0.0;
    }
    scores.iter().map(value).sum::<f64>() / scores.len() as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlap_metrics_distinguish_coverage_from_overpaint() {
        let gold = PixelRegion {
            x: 10,
            y: 10,
            width: 20,
            height: 10,
        };
        let exact = gold;
        let wide = PixelRegion {
            x: 0,
            y: 10,
            width: 40,
            height: 10,
        };
        assert_eq!(intersection_over_union(gold, exact), 1.0);
        assert_eq!(gold_coverage(gold, wide), 1.0);
        assert_eq!(overpaint_ratio(gold, wide), 1.0);
    }
}
