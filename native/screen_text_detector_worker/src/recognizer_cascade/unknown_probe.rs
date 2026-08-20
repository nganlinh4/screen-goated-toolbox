use anyhow::Result;
use image::RgbImage;

use super::{
    Acceleration, FallbackRecognizer, RecognitionSet, ResolvedModel, coverage_matches,
    is_text_line_candidate, load_models,
};
use crate::recognizer::UNKNOWN_PROBE_INPUT_WIDTH;

const MIN_SCORE: f32 = 0.3;
const REUSE_SCORE: f32 = 0.8;

pub(super) enum Selection {
    Loaded(usize),
    Pending(usize),
}

pub(super) fn needed(unresolved: &[bool], sources: &[RgbImage], representatives: &[usize]) -> bool {
    let count = representatives
        .iter()
        .filter(|index| unresolved.get(**index).copied().unwrap_or(false))
        .count();
    if count >= 3 && count.saturating_mul(20) >= representatives.len() {
        return true;
    }
    let line_area = representatives
        .iter()
        .filter_map(|index| sources.get(*index))
        .filter(|source| is_text_line_candidate(source))
        .map(image_area)
        .sum::<u64>();
    let unresolved_area = representatives
        .iter()
        .filter(|index| unresolved.get(**index).copied().unwrap_or(false))
        .filter_map(|index| sources.get(*index))
        .filter(|source| is_text_line_candidate(source))
        .map(image_area)
        .sum::<u64>();
    count > 0 && line_area > 0 && unresolved_area.saturating_mul(2) >= line_area
}

pub(super) fn samples(
    sources: &[RgbImage],
    unresolved: &[bool],
    results: &[RecognitionSet],
    representatives: &[usize],
) -> Vec<RgbImage> {
    let all = representatives
        .iter()
        .copied()
        .filter(|index| unresolved.get(*index).copied().unwrap_or(false))
        .collect::<Vec<_>>();
    let mut ranked = all.clone();
    ranked.sort_by(|left, right| {
        results[*left]
            .primary
            .confidence
            .total_cmp(&results[*right].primary.confidence)
            .then_with(|| image_area(&sources[*right]).cmp(&image_area(&sources[*left])))
    });
    let mut selected = ranked.iter().copied().take(3).collect::<Vec<_>>();
    ranked.sort_by_key(|index| std::cmp::Reverse(image_area(&sources[*index])));
    for index in ranked.into_iter().take(6) {
        if !selected.contains(&index) {
            selected.push(index);
        }
        if selected.len() == 6 {
            break;
        }
    }
    if all.len() > 1 {
        for step in 0..4 {
            let index = all[step * (all.len() - 1) / 3];
            if !selected.contains(&index) {
                selected.push(index);
            }
        }
    }
    selected
        .into_iter()
        .map(|index| sources[index].clone())
        .collect()
}

pub(super) fn select(
    loaded: &mut [FallbackRecognizer],
    pending: &[ResolvedModel],
    samples: &[RgbImage],
) -> Result<Option<Selection>> {
    let loaded_score = score_loaded(loaded, samples)?;
    if let Some((index, score)) = loaded_score
        && score >= REUSE_SCORE
    {
        return Ok(Some(Selection::Loaded(index)));
    }
    let pending_score = score_pending(pending, samples)?;
    Ok(match (loaded_score, pending_score) {
        (Some((_, loaded)), Some((pending_index, pending))) if pending > loaded => {
            Some(Selection::Pending(pending_index))
        }
        (Some((index, _)), _) => Some(Selection::Loaded(index)),
        (None, Some((index, _))) => Some(Selection::Pending(index)),
        (None, None) => None,
    })
}

fn score_loaded(
    fallbacks: &mut [FallbackRecognizer],
    samples: &[RgbImage],
) -> Result<Option<(usize, f32)>> {
    let scores = std::thread::scope(|scope| {
        let tasks = fallbacks
            .iter_mut()
            .enumerate()
            .map(|(index, fallback)| {
                scope.spawn(move || Ok::<_, anyhow::Error>((index, score(fallback, samples)?)))
            })
            .collect::<Vec<_>>();
        join_scores(tasks)
    })?;
    Ok(best(scores))
}

fn score_pending(pending: &[ResolvedModel], samples: &[RgbImage]) -> Result<Option<(usize, f32)>> {
    let scores = std::thread::scope(|scope| {
        let tasks = pending
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, model)| {
                scope.spawn(move || {
                    let mut loaded = load_models(vec![model], Acceleration::CpuProbe)?;
                    let mut fallback = loaded
                        .pop()
                        .ok_or_else(|| anyhow::anyhow!("specialist probe did not load"))?;
                    let score = score(&mut fallback, samples)?;
                    Ok::<_, anyhow::Error>((index, score))
                })
            })
            .collect::<Vec<_>>();
        join_scores(tasks)
    })?;
    Ok(best(scores))
}

fn score(fallback: &mut FallbackRecognizer, samples: &[RgbImage]) -> Result<f32> {
    let candidates = fallback
        .recognizer
        .recognize_batch_with_width(samples, UNKNOWN_PROBE_INPUT_WIDTH)?;
    let mut matching = candidates
        .iter()
        .filter(|candidate| coverage_matches(&fallback.routing, &candidate.text))
        .map(|candidate| candidate.confidence)
        .collect::<Vec<_>>();
    matching.sort_by(|left, right| right.total_cmp(left));
    let support = matching.len().min(3);
    if support == 0 {
        return Ok(0.0);
    }
    let mean = matching.iter().take(support).sum::<f32>() / support as f32;
    Ok(mean * (support as f32 / 2.0).min(1.0))
}

fn join_scores<'scope>(
    tasks: Vec<std::thread::ScopedJoinHandle<'scope, Result<(usize, f32)>>>,
) -> Result<Vec<(usize, f32)>> {
    tasks
        .into_iter()
        .map(|task| {
            task.join()
                .map_err(|_| anyhow::anyhow!("specialist recognizer probe panicked"))?
        })
        .collect()
}

fn best(scores: Vec<(usize, f32)>) -> Option<(usize, f32)> {
    scores
        .into_iter()
        .filter(|(_, score)| *score >= MIN_SCORE)
        .max_by(|left, right| left.1.total_cmp(&right.1))
}

fn image_area(source: &RgbImage) -> u64 {
    u64::from(source.width()) * u64::from(source.height())
}
