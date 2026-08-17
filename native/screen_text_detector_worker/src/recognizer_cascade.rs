use std::collections::HashSet;
use std::fs;
use std::os::windows::fs::MetadataExt as _;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use image::RgbImage;
use serde::Deserialize;

use crate::recognizer::{Acceleration, Recognition, TextRecognizer};

const CATALOG_LIMIT_BYTES: u64 = 32 * 1024;
const MODEL_LIMIT_BYTES: u64 = 64 * 1024 * 1024;
const MAX_FALLBACKS: usize = 15;
const MAX_COVERAGE_RANGES: usize = 16;
const WARMUP_WIDTH: u32 = 320;
const FAST_PATH_CONFIDENCE: f32 = 0.80;
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

pub(crate) struct RecognizerCascade {
    primary: TextRecognizer,
    fallbacks: Vec<FallbackRecognizer>,
    pending_fallbacks: Vec<ResolvedModel>,
}

pub(crate) struct RecognitionSet {
    pub(crate) primary: Recognition,
    pub(crate) alternatives: Vec<Recognition>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Catalog {
    schema_version: u32,
    primary: ModelSpec,
    fallbacks: Vec<ModelSpec>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ModelSpec {
    model: String,
    #[serde(default)]
    cpu_model: Option<String>,
    config: String,
    #[serde(default)]
    reverse_output: bool,
    #[serde(default)]
    coverage: Vec<[u32; 2]>,
}

struct ResolvedModel {
    model: PathBuf,
    cpu_model: Option<PathBuf>,
    config: PathBuf,
    reverse_output: bool,
    coverage: Vec<[u32; 2]>,
}

struct FallbackRecognizer {
    recognizer: TextRecognizer,
    coverage: Vec<[u32; 2]>,
}

impl RecognizerCascade {
    pub(crate) fn load(
        root: &Path,
        catalog_path: &Path,
        acceleration: Acceleration,
        eager_fallbacks: bool,
    ) -> Result<Self> {
        let metadata = fs::symlink_metadata(catalog_path)
            .with_context(|| format!("inspect recognizer catalog '{}'", catalog_path.display()))?;
        if !metadata.is_file()
            || is_reparse_point(&metadata)
            || metadata.len() == 0
            || metadata.len() > CATALOG_LIMIT_BYTES
        {
            bail!("recognizer catalog is unsafe or too large");
        }
        let catalog: Catalog =
            serde_json::from_slice(&fs::read(catalog_path).with_context(|| {
                format!("read recognizer catalog '{}'", catalog_path.display())
            })?)
            .context("parse recognizer catalog")?;
        if catalog.schema_version != 1
            || catalog.fallbacks.is_empty()
            || catalog.fallbacks.len() > MAX_FALLBACKS
        {
            bail!("recognizer catalog has unsupported bounds");
        }

        let mut seen = HashSet::new();
        let mut models = std::iter::once(catalog.primary)
            .chain(catalog.fallbacks)
            .map(|spec| resolve_model(root, spec, &mut seen))
            .collect::<Result<Vec<_>>>()?;
        let primary_model = models.remove(0);
        let primary = TextRecognizer::load(
            primary_model.path_for(acceleration),
            &primary_model.config,
            primary_model.reverse_output,
            acceleration,
        )?;
        let (fallbacks, pending_fallbacks) = if eager_fallbacks {
            (load_models_parallel(models, acceleration)?, Vec::new())
        } else {
            (Vec::new(), models)
        };
        Ok(Self {
            primary,
            fallbacks,
            pending_fallbacks,
        })
    }

    pub(crate) fn recognize_batch(&mut self, sources: &[RgbImage]) -> Result<Vec<RecognitionSet>> {
        let primary = self.recognize_primary(sources)?;
        self.recognize_alternatives(sources, primary)
    }

    pub(crate) fn recognize_primary(&mut self, sources: &[RgbImage]) -> Result<Vec<Recognition>> {
        self.primary.recognize_batch(sources)
    }

    pub(crate) fn warm_all(&mut self) -> Result<()> {
        let source = RgbImage::from_pixel(
            WARMUP_WIDTH,
            crate::recognizer::INPUT_HEIGHT,
            image::Rgb([255, 255, 255]),
        );
        self.primary
            .recognize_batch(std::slice::from_ref(&source))?;
        for fallback in &mut self.fallbacks {
            fallback
                .recognizer
                .recognize_batch(std::slice::from_ref(&source))?;
        }
        Ok(())
    }

    pub(crate) fn recognize_alternatives(
        &mut self,
        sources: &[RgbImage],
        primary: Vec<Recognition>,
    ) -> Result<Vec<RecognitionSet>> {
        let mut results = primary_sets(primary);
        let mut unresolved = results
            .iter()
            .zip(sources)
            .map(|(result, source)| {
                needs_alternatives(&result.primary) && is_text_line_candidate(source)
            })
            .collect::<Vec<_>>();
        let mut known_specialist_applied = false;
        for fallback in &mut self.fallbacks {
            let capture_evidence = fallback.matches_capture(&results);
            known_specialist_applied |= capture_evidence && unresolved.iter().any(|value| *value);
            apply_fallback(
                fallback,
                sources,
                &mut results,
                &mut unresolved,
                capture_evidence,
            )?;
        }
        let mut matching = Vec::new();
        let mut remaining = Vec::new();
        for model in std::mem::take(&mut self.pending_fallbacks) {
            if model.matches_capture(&results) {
                matching.push(model);
            } else {
                remaining.push(model);
            }
        }
        self.pending_fallbacks = remaining;
        let mut loaded = load_models_parallel(matching, Acceleration::Cpu)?;
        for fallback in &mut loaded {
            let capture_evidence = fallback.matches_capture(&results);
            known_specialist_applied |= capture_evidence && unresolved.iter().any(|value| *value);
            apply_fallback(
                fallback,
                sources,
                &mut results,
                &mut unresolved,
                capture_evidence,
            )?;
        }
        self.fallbacks.extend(loaded);
        if !known_specialist_applied && unknown_probe_needed(&unresolved, &results) {
            let pending = std::mem::take(&mut self.pending_fallbacks);
            let loaded = load_models_parallel(pending, Acceleration::Cpu)?;
            self.fallbacks.extend(loaded);
            if let Some(index) =
                select_unknown_fallback(&mut self.fallbacks, sources, &unresolved, &results)?
            {
                apply_fallback(
                    &mut self.fallbacks[index],
                    sources,
                    &mut results,
                    &mut unresolved,
                    true,
                )?;
            }
        }
        Ok(results)
    }
}

fn select_unknown_fallback(
    fallbacks: &mut [FallbackRecognizer],
    sources: &[RgbImage],
    unresolved: &[bool],
    results: &[RecognitionSet],
) -> Result<Option<usize>> {
    let all_unresolved = unresolved
        .iter()
        .enumerate()
        .filter_map(|(index, unresolved)| unresolved.then_some(index))
        .collect::<Vec<_>>();
    let mut sample_indices = all_unresolved.clone();
    sample_indices.sort_by(|left, right| {
        results[*left]
            .primary
            .confidence
            .total_cmp(&results[*right].primary.confidence)
            .then_with(|| {
                let area = |index: usize| {
                    u64::from(sources[index].width()) * u64::from(sources[index].height())
                };
                area(*right).cmp(&area(*left))
            })
    });
    let mut selected = sample_indices.iter().copied().take(3).collect::<Vec<_>>();
    sample_indices.sort_by_key(|index| {
        std::cmp::Reverse(u64::from(sources[*index].width()) * u64::from(sources[*index].height()))
    });
    for index in sample_indices.into_iter().take(6) {
        if !selected.contains(&index) {
            selected.push(index);
        }
        if selected.len() == 6 {
            break;
        }
    }
    if all_unresolved.len() > 1 {
        for step in 0..4 {
            let index = all_unresolved[step * (all_unresolved.len() - 1) / 3];
            if !selected.contains(&index) {
                selected.push(index);
            }
        }
    }
    let sample_indices = selected;
    let samples = sample_indices
        .iter()
        .map(|index| sources[*index].clone())
        .collect::<Vec<_>>();
    let scores = std::thread::scope(|scope| {
        let tasks = fallbacks
            .iter_mut()
            .enumerate()
            .map(|(index, fallback)| {
                let samples = &samples;
                scope.spawn(move || {
                    let candidates = fallback.recognizer.recognize_batch(samples)?;
                    let score = candidates
                        .iter()
                        .filter(|candidate| coverage_matches(&fallback.coverage, &candidate.text))
                        .map(|candidate| candidate.confidence)
                        .max_by(f32::total_cmp)
                        .unwrap_or(0.0);
                    Ok::<_, anyhow::Error>((index, score))
                })
            })
            .collect::<Vec<_>>();
        tasks
            .into_iter()
            .map(|task| {
                task.join()
                    .map_err(|_| anyhow::anyhow!("specialist recognizer probe panicked"))?
            })
            .collect::<Result<Vec<_>>>()
    })?;
    let best = scores
        .into_iter()
        .filter(|(_, score)| *score >= 0.3)
        .max_by(|left, right| left.1.total_cmp(&right.1));
    Ok(best.map(|(index, _)| index))
}

fn unknown_probe_needed(unresolved: &[bool], _results: &[RecognitionSet]) -> bool {
    let count = unresolved.iter().filter(|unresolved| **unresolved).count();
    count >= 3 && count.saturating_mul(20) >= unresolved.len()
}

fn apply_fallback(
    fallback: &mut FallbackRecognizer,
    sources: &[RgbImage],
    results: &mut [RecognitionSet],
    unresolved: &mut [bool],
    probe_unresolved: bool,
) -> Result<()> {
    let active = results
        .iter()
        .enumerate()
        .filter_map(|(index, result)| {
            (coverage_matches(&fallback.coverage, &result.primary.text)
                || (probe_unresolved && unresolved[index]))
                .then_some(index)
        })
        .collect::<Vec<_>>();
    if active.is_empty() {
        return Ok(());
    }
    let active_sources = active
        .iter()
        .map(|index| sources[*index].clone())
        .collect::<Vec<_>>();
    let candidates = fallback.recognizer.recognize_batch(&active_sources)?;
    for (&index, candidate) in active.iter().zip(candidates) {
        let primary_matches = coverage_matches(&fallback.coverage, &results[index].primary.text);
        let script_consistent = candidate.confidence >= FAST_PATH_CONFIDENCE
            && coverage_matches(&fallback.coverage, &candidate.text);
        if !candidate.text.is_empty()
            && !results[index]
                .alternatives
                .iter()
                .any(|known| known.text == candidate.text)
        {
            results[index].alternatives.push(candidate);
        }
        // A capture can contain multiple scripts. A specialist result for a
        // blank/ambiguous crop is evidence, but must not prevent another
        // capture-relevant specialist from contributing its own alternative.
        // Direct script matches may resolve normally.
        unresolved[index] &= !(script_consistent && primary_matches);
    }
    Ok(())
}

fn is_text_line_candidate(source: &RgbImage) -> bool {
    source.width() >= source.height().saturating_mul(3) / 2
        && (source.width() >= 32 || source.height() >= 20)
}

fn primary_sets(primary: Vec<Recognition>) -> Vec<RecognitionSet> {
    primary
        .into_iter()
        .map(|primary| RecognitionSet {
            alternatives: (!primary.text.is_empty())
                .then(|| primary.clone())
                .into_iter()
                .collect(),
            primary,
        })
        .collect()
}

fn load_models(
    models: Vec<ResolvedModel>,
    acceleration: Acceleration,
) -> Result<Vec<FallbackRecognizer>> {
    models
        .into_iter()
        .map(|model| {
            let recognizer = TextRecognizer::load(
                model.path_for(acceleration),
                &model.config,
                model.reverse_output,
                acceleration,
            )?;
            Ok(FallbackRecognizer {
                recognizer,
                coverage: model.coverage,
            })
        })
        .collect()
}

fn load_models_parallel(
    models: Vec<ResolvedModel>,
    acceleration: Acceleration,
) -> Result<Vec<FallbackRecognizer>> {
    if models.len() <= 1 {
        return load_models(models, acceleration);
    }
    std::thread::scope(|scope| {
        let tasks = models
            .into_iter()
            .map(|model| scope.spawn(move || load_models(vec![model], acceleration)))
            .collect::<Vec<_>>();
        tasks
            .into_iter()
            .map(|task| {
                task.join()
                    .map_err(|_| anyhow::anyhow!("specialist recognizer loader panicked"))?
                    .and_then(|mut loaded| {
                        loaded
                            .pop()
                            .ok_or_else(|| anyhow::anyhow!("specialist recognizer did not load"))
                    })
            })
            .collect()
    })
}

fn needs_alternatives(result: &Recognition) -> bool {
    result.text.is_empty() || result.confidence < FAST_PATH_CONFIDENCE
}

fn resolve_model(
    root: &Path,
    spec: ModelSpec,
    seen: &mut HashSet<PathBuf>,
) -> Result<ResolvedModel> {
    if spec.coverage.len() > MAX_COVERAGE_RANGES
        || spec
            .coverage
            .iter()
            .any(|[start, end]| start > end || *end > char::MAX as u32)
    {
        bail!("recognizer catalog contains invalid Unicode coverage");
    }
    let model = resolve_regular_file(root, &spec.model)?;
    let cpu_model = spec
        .cpu_model
        .as_deref()
        .map(|path| resolve_regular_file(root, path))
        .transpose()?;
    let config = resolve_regular_file(root, &spec.config)?;
    if !seen.insert(model.clone())
        || cpu_model
            .as_ref()
            .is_some_and(|path| !seen.insert(path.clone()))
        || !seen.insert(config.clone())
    {
        bail!("recognizer catalog repeats a model file");
    }
    Ok(ResolvedModel {
        model,
        cpu_model,
        config,
        reverse_output: spec.reverse_output,
        coverage: spec.coverage,
    })
}

impl ResolvedModel {
    fn path_for(&self, acceleration: Acceleration) -> &Path {
        match acceleration {
            Acceleration::Cpu => self.cpu_model.as_deref().unwrap_or(&self.model),
            Acceleration::DirectMl => &self.model,
        }
    }

    fn matches_capture(&self, results: &[RecognitionSet]) -> bool {
        results
            .iter()
            .any(|result| coverage_matches(&self.coverage, &result.primary.text))
    }
}

impl FallbackRecognizer {
    fn matches_capture(&self, results: &[RecognitionSet]) -> bool {
        results
            .iter()
            .any(|result| coverage_matches(&self.coverage, &result.primary.text))
    }
}

fn coverage_matches(coverage: &[[u32; 2]], text: &str) -> bool {
    !coverage.is_empty()
        && text.chars().any(|character| {
            let codepoint = character as u32;
            coverage
                .iter()
                .any(|[start, end]| codepoint >= *start && codepoint <= *end)
        })
}

fn resolve_regular_file(root: &Path, relative: &str) -> Result<PathBuf> {
    let relative = Path::new(relative);
    if relative.as_os_str().is_empty()
        || relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!("recognizer catalog contains an unsafe path");
    }
    let mut resolved = root.to_path_buf();
    let count = relative.components().count();
    for (index, component) in relative.components().enumerate() {
        let Component::Normal(name) = component else {
            unreachable!();
        };
        resolved.push(name);
        let metadata = fs::symlink_metadata(&resolved)
            .with_context(|| format!("inspect recognizer file '{}'", resolved.display()))?;
        if is_reparse_point(&metadata)
            || (index + 1 == count && (!metadata.is_file() || metadata.len() == 0))
            || (index + 1 < count && !metadata.is_dir())
        {
            bail!("recognizer catalog path is unsafe");
        }
    }
    if fs::metadata(&resolved)?.len() > MODEL_LIMIT_BYTES {
        bail!("recognizer model file exceeds its size limit");
    }
    Ok(resolved)
}

fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(test)]
#[path = "recognizer_cascade_tests.rs"]
mod tests;
