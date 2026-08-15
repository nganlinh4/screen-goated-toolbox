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
            (load_models(models, acceleration)?, Vec::new())
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
        self.ensure_matching_fallbacks(&primary)?;
        if !Self::batch_needs_alternatives(&primary)
            && !self
                .fallbacks
                .iter()
                .any(|fallback| fallback.matches_capture(&primary))
        {
            return Ok(primary_sets(primary));
        }
        collect_alternatives(&mut self.fallbacks, sources, primary)
    }

    pub(crate) fn batch_needs_alternatives(primary: &[Recognition]) -> bool {
        primary.iter().any(needs_alternatives)
    }

    fn ensure_matching_fallbacks(&mut self, primary: &[Recognition]) -> Result<()> {
        if self.pending_fallbacks.is_empty() {
            return Ok(());
        }
        let (selected, remaining): (Vec<_>, Vec<_>) = self
            .pending_fallbacks
            .drain(..)
            .partition(|model| model.matches_capture(primary));
        self.pending_fallbacks = remaining;
        self.fallbacks
            .extend(load_models(selected, self.acceleration)?);
        Ok(())
    }
}

fn collect_alternatives(
    fallbacks: &mut [FallbackRecognizer],
    sources: &[RgbImage],
    primary: Vec<Recognition>,
) -> Result<Vec<RecognitionSet>> {
    let mut results = primary
        .into_iter()
        .map(|primary| RecognitionSet {
            alternatives: (!primary.text.is_empty())
                .then(|| primary.clone())
                .into_iter()
                .collect(),
            primary,
        })
        .collect::<Vec<_>>();
    for fallback in fallbacks {
        if !results
            .iter()
            .any(|result| coverage_matches(&fallback.coverage, &result.primary.text))
        {
            continue;
        }
        let active = results
            .iter()
            .enumerate()
            .filter_map(|(index, result)| {
                (needs_alternatives(&result.primary)
                    || coverage_matches(&fallback.coverage, &result.primary.text))
                .then_some(index)
            })
            .collect::<Vec<_>>();
        let active_sources = active
            .iter()
            .map(|index| sources[*index].clone())
            .collect::<Vec<_>>();
        if active_sources.is_empty() {
            continue;
        }
        let candidates = fallback.recognizer.recognize_batch(&active_sources)?;
        for (&index, candidate) in active.iter().zip(candidates) {
            if !candidate.text.is_empty()
                && !results[index]
                    .alternatives
                    .iter()
                    .any(|known| known.text == candidate.text)
            {
                results[index].alternatives.push(candidate);
            }
        }
    }
    Ok(results)
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

    fn matches_capture(&self, primary: &[Recognition]) -> bool {
        capture_needs_specialist(primary)
            && primary
                .iter()
                .any(|result| coverage_matches(&self.coverage, &result.text))
    }
}

impl FallbackRecognizer {
    fn matches_capture(&self, primary: &[Recognition]) -> bool {
        capture_needs_specialist(primary)
            && primary
                .iter()
                .any(|result| coverage_matches(&self.coverage, &result.text))
    }
}

fn capture_needs_specialist(primary: &[Recognition]) -> bool {
    primary.iter().any(needs_alternatives)
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
mod tests {
    use super::*;

    #[test]
    fn fast_path_requires_confident_text() {
        assert!(!needs_alternatives(&Recognition {
            text: "ordinary text".to_string(),
            confidence: FAST_PATH_CONFIDENCE,
        }));
        assert!(needs_alternatives(&Recognition {
            text: String::new(),
            confidence: 1.0,
        }));
    }

    #[test]
    fn specialist_coverage_routes_only_matching_unicode() {
        let coverage = [[0x0400, 0x052f]];
        assert!(coverage_matches(&coverage, "Текст"));
        assert!(!coverage_matches(&coverage, "Text"));
        assert!(!coverage_matches(&[], "Текст"));
    }

    #[test]
    fn confident_ambiguous_script_routes_when_the_capture_is_otherwise_weak() {
        let coverage = [[0x4e00, 0x9fff]];
        let model = ResolvedModel {
            model: PathBuf::new(),
            cpu_model: None,
            config: PathBuf::new(),
            reverse_output: false,
            coverage: coverage.to_vec(),
        };
        let ambiguous = Recognition {
            text: "号合号合".to_string(),
            confidence: 0.99,
        };
        let weak = Recognition {
            text: String::new(),
            confidence: 0.0,
        };
        assert!(model.matches_capture(&[ambiguous.clone(), weak]));
        assert!(!model.matches_capture(&[ambiguous]));
    }
}
