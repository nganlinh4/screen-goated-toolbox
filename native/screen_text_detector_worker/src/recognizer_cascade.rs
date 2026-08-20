use std::collections::{HashMap, HashSet};
use std::fs;
use std::os::windows::fs::MetadataExt as _;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use image::RgbImage;
use serde::Deserialize;

use crate::recognizer::{
    Acceleration, PRIMARY_INPUT_WIDTH, Recognition, TextRecognizer, UNKNOWN_PROBE_INPUT_WIDTH,
};

mod unknown_probe;

const CATALOG_LIMIT_BYTES: u64 = 32 * 1024;
const MODEL_LIMIT_BYTES: u64 = 64 * 1024 * 1024;
const MAX_FALLBACKS: usize = 15;
const MAX_COVERAGE_RANGES: usize = 16;
const WARMUP_WIDTH: u32 = 320;
const FAST_PATH_CONFIDENCE: f32 = 0.80;
const SPECIALIST_ALTERNATIVE_CONFIDENCE: f32 = 0.60;
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

pub(crate) struct RecognizerCascade {
    primary: TextRecognizer,
    fallbacks: Vec<FallbackRecognizer>,
    pending_fallbacks: Vec<ResolvedModel>,
    acceleration: Acceleration,
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
    #[serde(default)]
    routing: Vec<[u32; 2]>,
}

#[derive(Clone)]
struct ResolvedModel {
    model: PathBuf,
    cpu_model: Option<PathBuf>,
    config: PathBuf,
    reverse_output: bool,
    coverage: Vec<[u32; 2]>,
    routing: Vec<[u32; 2]>,
}

struct FallbackRecognizer {
    recognizer: TextRecognizer,
    coverage: Vec<[u32; 2]>,
    routing: Vec<[u32; 2]>,
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
            PRIMARY_INPUT_WIDTH,
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
            acceleration,
        })
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
        region_indices: &[usize],
    ) -> Result<Vec<RecognitionSet>> {
        if primary.len() != sources.len() || region_indices.len() != sources.len() {
            bail!("recognizer cascade input lengths differ");
        }
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
            let representatives = representative_indices(&results, region_indices);
            let capture_evidence = fallback.matches_capture(&results, &representatives);
            known_specialist_applied |= capture_evidence && unresolved.iter().any(|value| *value);
            apply_fallback(
                fallback,
                sources,
                &mut results,
                &mut unresolved,
                capture_evidence,
                PRIMARY_INPUT_WIDTH,
            )?;
        }
        let mut matching = Vec::new();
        let mut remaining = Vec::new();
        let representatives = representative_indices(&results, region_indices);
        for model in std::mem::take(&mut self.pending_fallbacks) {
            if model.matches_capture(&results, &representatives) {
                matching.push(model);
            } else {
                remaining.push(model);
            }
        }
        self.pending_fallbacks = remaining;
        let mut loaded = load_models_parallel(matching, self.acceleration)?;
        for fallback in &mut loaded {
            let representatives = representative_indices(&results, region_indices);
            let capture_evidence = fallback.matches_capture(&results, &representatives);
            known_specialist_applied |= capture_evidence && unresolved.iter().any(|value| *value);
            apply_fallback(
                fallback,
                sources,
                &mut results,
                &mut unresolved,
                capture_evidence,
                PRIMARY_INPUT_WIDTH,
            )?;
        }
        self.fallbacks.extend(loaded);
        let representatives = representative_indices(&results, region_indices);
        if !known_specialist_applied
            && unknown_probe::needed(&unresolved, sources, &representatives)
        {
            let samples = unknown_probe::samples(sources, &unresolved, &results, &representatives);
            let selected =
                unknown_probe::select(&mut self.fallbacks, &self.pending_fallbacks, &samples)?;
            let selected_index = match selected {
                Some(unknown_probe::Selection::Loaded(index)) => Some(index),
                Some(unknown_probe::Selection::Pending(index)) => {
                    let model = self.pending_fallbacks.remove(index);
                    let mut loaded = load_models(vec![model], self.acceleration)?;
                    self.fallbacks.push(
                        loaded
                            .pop()
                            .context("selected specialist recognizer did not load")?,
                    );
                    Some(self.fallbacks.len() - 1)
                }
                None => None,
            };
            if let Some(index) = selected_index {
                apply_fallback(
                    &mut self.fallbacks[index],
                    sources,
                    &mut results,
                    &mut unresolved,
                    true,
                    UNKNOWN_PROBE_INPUT_WIDTH,
                )?;
            }
        }
        Ok(results)
    }
}

fn apply_fallback(
    fallback: &mut FallbackRecognizer,
    sources: &[RgbImage],
    results: &mut [RecognitionSet],
    unresolved: &mut [bool],
    probe_unresolved: bool,
    input_width: u32,
) -> Result<()> {
    let active = results
        .iter()
        .enumerate()
        .filter_map(|(index, result)| {
            (coverage_matches(&fallback.routing, &result.primary.text)
                || (evidence_matches(&fallback.routing, &result.primary)
                    && !has_conflicting_non_ascii_text(&fallback.routing, &result.primary.text))
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
    let candidates = fallback
        .recognizer
        .recognize_batch_with_width(&active_sources, input_width)?;
    for (&index, candidate) in active.iter().zip(candidates) {
        let primary_matches = coverage_matches(&fallback.routing, &results[index].primary.text);
        let script_consistent = candidate.confidence >= FAST_PATH_CONFIDENCE
            && coverage_matches(&fallback.coverage, &candidate.text);
        if specialist_alternative_is_usable(&fallback.coverage, &candidate)
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

fn specialist_alternative_is_usable(coverage: &[[u32; 2]], candidate: &Recognition) -> bool {
    candidate.confidence >= SPECIALIST_ALTERNATIVE_CONFIDENCE
        && coverage_matches(coverage, &candidate.text)
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
    let input_width = match acceleration {
        Acceleration::CpuProbe => UNKNOWN_PROBE_INPUT_WIDTH,
        Acceleration::Cpu | Acceleration::DirectMl => PRIMARY_INPUT_WIDTH,
    };
    models
        .into_iter()
        .map(|model| {
            let recognizer = TextRecognizer::load(
                model.path_for(acceleration),
                &model.config,
                model.reverse_output,
                acceleration,
                input_width,
            )?;
            Ok(FallbackRecognizer {
                recognizer,
                coverage: model.coverage,
                routing: model.routing,
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
    result.confidence < FAST_PATH_CONFIDENCE || !result.text.chars().any(char::is_alphabetic)
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
        || spec.routing.len() > MAX_COVERAGE_RANGES
        || spec
            .routing
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
        routing: spec.routing,
    })
}

impl ResolvedModel {
    fn path_for(&self, acceleration: Acceleration) -> &Path {
        match acceleration {
            Acceleration::Cpu | Acceleration::CpuProbe => {
                self.cpu_model.as_deref().unwrap_or(&self.model)
            }
            Acceleration::DirectMl => &self.model,
        }
    }

    fn matches_capture(&self, results: &[RecognitionSet], representatives: &[usize]) -> bool {
        capture_routing_matches_at(&self.routing, results, representatives)
    }
}

impl FallbackRecognizer {
    fn matches_capture(&self, results: &[RecognitionSet], representatives: &[usize]) -> bool {
        capture_routing_matches_at(&self.routing, results, representatives)
    }
}

#[cfg(test)]
fn capture_routing_matches(routing: &[[u32; 2]], results: &[RecognitionSet]) -> bool {
    capture_routing_matches_at(routing, results, &(0..results.len()).collect::<Vec<_>>())
}

fn capture_routing_matches_at(
    routing: &[[u32; 2]],
    results: &[RecognitionSet],
    representatives: &[usize],
) -> bool {
    if routing.is_empty() {
        return false;
    }
    let mut direct_score = 0.0_f32;
    let mut matching_regions = 0_usize;
    for result in representatives
        .iter()
        .filter_map(|index| results.get(*index))
    {
        let matching_characters = result
            .primary
            .text
            .chars()
            .filter(|character| codepoint_matches(routing, *character as u32))
            .count();
        if matching_characters > 0 && result.primary.confidence >= 0.5 {
            matching_regions += 1;
            direct_score += result.primary.confidence * matching_characters.min(4) as f32;
        }
        if evidence_matches(routing, &result.primary)
            && !has_conflicting_non_ascii_text(routing, &result.primary.text)
        {
            direct_score += result.primary.confidence.max(0.5);
        }
    }
    direct_score >= 1.4 || (matching_regions >= 2 && direct_score >= 1.0)
}

fn representative_indices(results: &[RecognitionSet], groups: &[usize]) -> Vec<usize> {
    let mut positions: HashMap<usize, usize> = HashMap::new();
    let mut selected: Vec<usize> = Vec::new();
    for (index, (result, group)) in results.iter().zip(groups).enumerate() {
        match positions.get(group).copied() {
            Some(position)
                if recognition_signal(&results[selected[position]].primary)
                    < recognition_signal(&result.primary) =>
            {
                selected[position] = index;
            }
            Some(_) => {}
            None => {
                positions.insert(*group, selected.len());
                selected.push(index);
            }
        }
    }
    selected
}

fn recognition_signal(recognition: &Recognition) -> f32 {
    let useful = recognition
        .text
        .chars()
        .filter(|character| character.is_alphanumeric())
        .count()
        .max(1);
    recognition.confidence.max(0.0) * (useful as f32).sqrt()
}

fn has_conflicting_non_ascii_text(routing: &[[u32; 2]], text: &str) -> bool {
    text.chars().any(|character| {
        character.is_alphabetic()
            && !character.is_ascii()
            && !codepoint_matches(routing, character as u32)
    })
}

fn coverage_matches(coverage: &[[u32; 2]], text: &str) -> bool {
    !coverage.is_empty()
        && text
            .chars()
            .any(|character| codepoint_matches(coverage, character as u32))
}

fn codepoint_matches(coverage: &[[u32; 2]], codepoint: u32) -> bool {
    coverage
        .iter()
        .any(|[start, end]| codepoint >= *start && codepoint <= *end)
}

fn evidence_matches(coverage: &[[u32; 2]], result: &Recognition) -> bool {
    let matches = result
        .script_evidence
        .iter()
        .filter(|codepoint| codepoint_matches(coverage, **codepoint))
        .count();
    matches >= 2 && matches.saturating_mul(4) >= result.token_count
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
