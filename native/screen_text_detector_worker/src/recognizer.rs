use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use image::RgbImage;
use ort::session::Session;
use ort::session::builder::GraphOptimizationLevel;
use ort::value::Tensor;

mod batching;

use batching::{batch_tensor, prepare, recognition_batches};

pub(crate) const INPUT_HEIGHT: u32 = 48;
const MIN_INPUT_WIDTH: u32 = 32;
pub(crate) const PRIMARY_INPUT_WIDTH: u32 = 1_600;
pub(crate) const UNKNOWN_PROBE_INPUT_WIDTH: u32 = 320;
const MAX_CHARACTERS: usize = 1_024;
const TOP_CANDIDATE_COUNT: usize = 3;

#[derive(Clone, Debug)]
pub(crate) struct Recognition {
    pub(crate) text: String,
    pub(crate) confidence: f32,
    pub(crate) script_evidence: Vec<u32>,
    pub(crate) token_count: usize,
}

pub(crate) struct TextRecognizer {
    session: Session,
    characters: Vec<String>,
    reverse_output: bool,
    max_input_width: u32,
}

#[derive(Clone, Copy)]
pub(crate) enum Acceleration {
    Cpu,
    CpuProbe,
    DirectMl,
}

impl TextRecognizer {
    pub(crate) fn load(
        model: &Path,
        config: &Path,
        reverse_output: bool,
        acceleration: Acceleration,
        max_input_width: u32,
    ) -> Result<Self> {
        let builder = Session::builder()?
            .with_memory_pattern(false)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        let mut builder = match acceleration {
            Acceleration::Cpu => builder,
            Acceleration::CpuProbe => builder
                .with_intra_threads(1)
                .map_err(|error| anyhow::anyhow!(error.to_string()))?
                .with_inter_threads(1)
                .map_err(|error| anyhow::anyhow!(error.to_string()))?,
            Acceleration::DirectMl => builder
                .with_optimization_level(GraphOptimizationLevel::Disable)
                .map_err(|error| anyhow::anyhow!(error.to_string()))?
                .with_execution_providers([ort::ep::DirectML::default().build()])
                .map_err(|error| anyhow::anyhow!(error.to_string()))?,
        };
        let session = builder
            .commit_from_file(model)
            .with_context(|| format!("load PaddleOCR recognizer '{}'", model.display()))?;
        let yaml: serde_yaml::Value = serde_yaml::from_slice(
            &fs::read(config)
                .with_context(|| format!("read recognizer config '{}'", config.display()))?,
        )?;
        let dictionary = yaml
            .get("PostProcess")
            .and_then(|value| value.get("character_dict"))
            .and_then(serde_yaml::Value::as_sequence)
            .context("recognizer character dictionary is missing")?;
        let mut characters = Vec::with_capacity(dictionary.len() + 2);
        characters.push(String::new());
        for value in dictionary {
            characters.push(
                value
                    .as_str()
                    .context("recognizer dictionary contains a non-string")?
                    .to_string(),
            );
        }
        characters.push(" ".to_string());
        Ok(Self {
            session,
            characters,
            reverse_output,
            max_input_width,
        })
    }

    pub(crate) fn recognize_batch(&mut self, sources: &[RgbImage]) -> Result<Vec<Recognition>> {
        self.recognize_batch_with_width(sources, self.max_input_width)
    }

    pub(crate) fn recognize_batch_with_width(
        &mut self,
        sources: &[RgbImage],
        max_input_width: u32,
    ) -> Result<Vec<Recognition>> {
        let plan = RecognitionPlan::new(sources, max_input_width)?;
        let prepared = plan
            .tiles
            .iter()
            .map(|tile| prepare(&tile.image, max_input_width))
            .collect::<Result<Vec<_>>>()?;
        let mut recognized = (0..prepared.len()).map(|_| None).collect::<Vec<_>>();
        for indices in recognition_batches(&prepared, self.characters.len()) {
            let width = indices
                .iter()
                .map(|index| prepared[*index].width)
                .max()
                .context("recognizer batch is empty")?;
            let chw = batch_tensor(&prepared, &indices, width)?;
            let tensor = Tensor::from_array((
                vec![indices.len(), 3, INPUT_HEIGHT as usize, width as usize],
                chw,
            ))?;
            let outputs = self.session.run(ort::inputs![tensor])?;
            if outputs.len() == 2 {
                decode_compact_batch(&outputs, &indices, &self.characters, &mut recognized)?;
            } else {
                decode_full_batch(&outputs, &indices, &self.characters, &mut recognized)?;
            }
        }
        let recognized = recognized
            .into_iter()
            .map(|result| result.context("recognizer omitted a batch result"))
            .collect::<Result<Vec<_>>>()?;
        plan.assemble(recognized, self.reverse_output)
    }
}

fn decode_full_batch(
    outputs: &ort::session::SessionOutputs<'_>,
    indices: &[usize],
    characters: &[String],
    recognized: &mut [Option<Recognition>],
) -> Result<()> {
    let (shape, scores) = outputs[0]
        .try_extract_tensor::<f32>()
        .context("read PaddleOCR recognition scores")?;
    let dimensions = shape.as_ref();
    if dimensions.len() != 3
        || dimensions[0] != indices.len() as i64
        || dimensions[2] != characters.len() as i64
    {
        bail!("PaddleOCR returned an unexpected recognition shape");
    }
    let steps = dimensions[1] as usize;
    let result_size = steps.saturating_mul(characters.len());
    for (batch_index, source_index) in indices.iter().enumerate() {
        let start = batch_index.saturating_mul(result_size);
        let end = start.saturating_add(result_size);
        recognized[*source_index] = Some(decode(
            scores
                .get(start..end)
                .context("recognizer output batch is truncated")?,
            steps,
            characters,
            false,
        )?);
    }
    Ok(())
}

fn decode_compact_batch(
    outputs: &ort::session::SessionOutputs<'_>,
    source_indices: &[usize],
    characters: &[String],
    recognized: &mut [Option<Recognition>],
) -> Result<()> {
    let (value_shape, values) = outputs[0]
        .try_extract_tensor::<f32>()
        .context("read compact PaddleOCR candidate scores")?;
    let (index_shape, indices) = outputs[1]
        .try_extract_tensor::<i64>()
        .context("read compact PaddleOCR candidate indices")?;
    let dimensions = value_shape.as_ref();
    if dimensions != index_shape.as_ref()
        || dimensions.len() != 3
        || dimensions[0] != source_indices.len() as i64
        || dimensions[2] != TOP_CANDIDATE_COUNT as i64
    {
        bail!("PaddleOCR returned an unexpected compact recognition shape");
    }
    let steps = dimensions[1] as usize;
    let result_size = steps.saturating_mul(TOP_CANDIDATE_COUNT);
    for (batch_index, source_index) in source_indices.iter().enumerate() {
        let start = batch_index.saturating_mul(result_size);
        let end = start.saturating_add(result_size);
        recognized[*source_index] = Some(decode_compact(
            values
                .get(start..end)
                .context("compact recognizer scores are truncated")?,
            indices
                .get(start..end)
                .context("compact recognizer indices are truncated")?,
            steps,
            characters,
        )?);
    }
    Ok(())
}

struct RecognitionPlan {
    source_count: usize,
    tiles: Vec<RecognitionTile>,
}

struct RecognitionTile {
    source_index: usize,
    image: RgbImage,
    separated_from_previous: bool,
}

impl RecognitionPlan {
    fn new(sources: &[RgbImage], max_input_width: u32) -> Result<Self> {
        let mut tiles = Vec::new();
        for (source_index, source) in sources.iter().enumerate() {
            if source.width() == 0 || source.height() == 0 {
                bail!("recognizer crop is empty");
            }
            let ranges = recognition_ranges(source, max_input_width);
            for (index, range) in ranges.into_iter().enumerate() {
                tiles.push(RecognitionTile {
                    source_index,
                    image: image::imageops::crop_imm(
                        source,
                        range.start,
                        0,
                        range.end - range.start,
                        source.height(),
                    )
                    .to_image(),
                    separated_from_previous: index > 0 && range.separated,
                });
            }
        }
        Ok(Self {
            source_count: sources.len(),
            tiles,
        })
    }

    fn assemble(
        self,
        recognized: Vec<Recognition>,
        reverse_output: bool,
    ) -> Result<Vec<Recognition>> {
        if recognized.len() != self.tiles.len() {
            bail!("recognizer tile result count mismatch");
        }
        let mut results = (0..self.source_count)
            .map(|_| Recognition {
                text: String::new(),
                confidence: 0.0,
                script_evidence: Vec::new(),
                token_count: 0,
            })
            .collect::<Vec<_>>();
        for (tile, result) in self.tiles.into_iter().zip(recognized) {
            let target = &mut results[tile.source_index];
            if tile.separated_from_previous
                && !target.text.is_empty()
                && !result.text.is_empty()
                && (target.text.chars().any(char::is_whitespace)
                    || result.text.chars().any(char::is_whitespace))
            {
                target.text.push(' ');
            }
            target.text.push_str(result.text.trim());
            target.confidence += result.confidence * result.token_count as f32;
            target.token_count += result.token_count;
            target.script_evidence.extend(result.script_evidence);
        }
        for result in &mut results {
            result.confidence = if result.token_count == 0 {
                0.0
            } else {
                (result.confidence / result.token_count as f32).clamp(0.0, 1.0)
            };
            result.text = if reverse_output {
                reverse_directional(&result.text)
            } else {
                std::mem::take(&mut result.text)
            }
            .trim()
            .to_string();
        }
        Ok(results)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RecognitionRange {
    start: u32,
    end: u32,
    separated: bool,
}

fn recognition_ranges(source: &RgbImage, max_input_width: u32) -> Vec<RecognitionRange> {
    let max_source_width =
        (max_input_width as u64 * source.height() as u64 / INPUT_HEIGHT as u64) as u32;
    if source.width() <= max_source_width.max(1) {
        return vec![RecognitionRange {
            start: 0,
            end: source.width(),
            separated: false,
        }];
    }

    let mut ranges = Vec::new();
    let mut start = 0;
    let mut separated_from_previous = false;
    while source.width() - start > max_source_width {
        let earliest = start + max_source_width.saturating_mul(3) / 4;
        let latest = (start + max_source_width).min(source.width() - 1);
        let cut = quietest_cut(source, earliest, latest);
        ranges.push(RecognitionRange {
            start,
            end: cut,
            separated: separated_from_previous,
        });
        start = cut;
        separated_from_previous = true;
    }
    ranges.push(RecognitionRange {
        start,
        end: source.width(),
        separated: separated_from_previous,
    });
    ranges
}

fn quietest_cut(source: &RgbImage, earliest: u32, latest: u32) -> u32 {
    (earliest..=latest)
        .min_by_key(|&x| column_activity(source, x))
        .unwrap_or(latest)
}

fn column_activity(source: &RgbImage, x: u32) -> u64 {
    let radius = (source.height() / 6).clamp(1, 4);
    let start = x.saturating_sub(radius);
    let end = (x + radius).min(source.width() - 1);
    (start..end)
        .flat_map(|column| (0..source.height()).map(move |y| (column, y)))
        .map(|(column, y)| {
            let a = source.get_pixel(column, y);
            let b = source.get_pixel(column + 1, y);
            (0..3)
                .map(|channel| u64::from(a[channel].abs_diff(b[channel])))
                .sum::<u64>()
        })
        .sum()
}

fn decode(
    scores: &[f32],
    steps: usize,
    characters: &[String],
    reverse_output: bool,
) -> Result<Recognition> {
    let classes = characters.len();
    if scores.len() != steps.saturating_mul(classes) {
        bail!("recognizer output length is invalid");
    }
    let mut tokens = Vec::new();
    let mut confidence = 0.0_f32;
    let mut count = 0_usize;
    let mut previous = usize::MAX;
    let mut script_evidence = Vec::new();
    for row in scores.chunks_exact(classes) {
        let candidates = top_candidates(row).context("recognizer returned an empty timestep")?;
        let (index, score) = candidates[0];
        if index != 0 && index != previous && tokens.len() < MAX_CHARACTERS {
            tokens.push(characters[index].as_str());
            confidence += score;
            count += 1;
            collect_script_evidence(&candidates, score, characters, &mut script_evidence);
        }
        previous = index;
    }
    let text = tokens.concat();
    Ok(Recognition {
        text: if reverse_output {
            reverse_directional(&text)
        } else {
            text
        }
        .trim()
        .to_string(),
        confidence: if count == 0 {
            0.0
        } else {
            (confidence / count as f32).clamp(0.0, 1.0)
        },
        script_evidence,
        token_count: count,
    })
}

fn decode_compact(
    scores: &[f32],
    indices: &[i64],
    steps: usize,
    characters: &[String],
) -> Result<Recognition> {
    if scores.len() != steps.saturating_mul(TOP_CANDIDATE_COUNT) || scores.len() != indices.len() {
        bail!("compact recognizer output length is invalid");
    }
    let mut tokens = Vec::new();
    let mut confidence = 0.0_f32;
    let mut count = 0_usize;
    let mut previous = usize::MAX;
    let mut script_evidence = Vec::new();
    for (row_scores, row_indices) in scores
        .chunks_exact(TOP_CANDIDATE_COUNT)
        .zip(indices.chunks_exact(TOP_CANDIDATE_COUNT))
    {
        let mut candidates = [(0_usize, 0.0_f32); TOP_CANDIDATE_COUNT];
        for candidate in 0..TOP_CANDIDATE_COUNT {
            let index = usize::try_from(row_indices[candidate])
                .context("compact recognizer returned a negative class index")?;
            if index >= characters.len() {
                bail!("compact recognizer returned an invalid class index");
            }
            candidates[candidate] = (index, row_scores[candidate]);
        }
        let (index, score) = candidates[0];
        if index != 0 && index != previous && tokens.len() < MAX_CHARACTERS {
            tokens.push(characters[index].as_str());
            confidence += score;
            count += 1;
            collect_script_evidence(&candidates, score, characters, &mut script_evidence);
        }
        previous = index;
    }
    Ok(Recognition {
        text: tokens.concat().trim().to_string(),
        confidence: if count == 0 {
            0.0
        } else {
            (confidence / count as f32).clamp(0.0, 1.0)
        },
        script_evidence,
        token_count: count,
    })
}

fn collect_script_evidence(
    candidates: &[(usize, f32); 3],
    winning_score: f32,
    characters: &[String],
    evidence: &mut Vec<u32>,
) {
    const SCORE_RATIO: f32 = 0.5;

    for &(index, score) in candidates {
        if index != 0 && score >= winning_score * SCORE_RATIO {
            evidence.extend(characters[index].chars().map(|character| character as u32));
        }
    }
}

fn top_candidates(scores: &[f32]) -> Option<[(usize, f32); 3]> {
    let first = *scores.first()?;
    let mut best = [(0, first), (0, f32::NEG_INFINITY), (0, f32::NEG_INFINITY)];
    for (index, score) in scores.iter().copied().enumerate().skip(1) {
        if score > best[0].1 {
            best[2] = best[1];
            best[1] = best[0];
            best[0] = (index, score);
        } else if score > best[1].1 {
            best[2] = best[1];
            best[1] = (index, score);
        } else if score > best[2].1 {
            best[2] = (index, score);
        }
    }
    Some(best)
}

fn reverse_directional(text: &str) -> String {
    let mut groups = Vec::new();
    let mut directional = String::new();
    for character in text.chars() {
        if character.is_ascii_alphanumeric()
            || matches!(character, ' ' | ':' | '*' | '.' | '/' | '%' | '+' | '-')
        {
            directional.push(character);
        } else {
            if !directional.is_empty() {
                groups.push(std::mem::take(&mut directional));
            }
            groups.push(character.to_string());
        }
    }
    if !directional.is_empty() {
        groups.push(directional);
    }
    groups.into_iter().rev().collect()
}

#[cfg(test)]
#[path = "recognizer_tests.rs"]
mod tests;
