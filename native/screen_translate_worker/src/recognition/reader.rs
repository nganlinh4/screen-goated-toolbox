use anyhow::{Context, Result, bail};
use image::RgbImage;
use ort::{
    session::{Session, builder::GraphOptimizationLevel},
    value::{Tensor, TensorRef},
};
use sgt_screen_text_detector_protocol::{
    MAX_REGION_TEXT_BYTES,
    recognition::{Completion, Reading},
    stream::Region,
};
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

use super::catalog::ReaderSpec;
use super::lines::{self, HEIGHT};
const BATCH: usize = 8;

pub(super) struct Reader {
    session: Session,
    characters: Vec<String>,
    reverse: bool,
    input: Vec<f32>,
}

impl Reader {
    pub(super) fn alphabet(&self) -> std::collections::HashSet<char> {
        self.characters
            .iter()
            .flat_map(|token| token.chars())
            .collect()
    }

    pub(super) fn load(root: &Path, spec: &ReaderSpec) -> Result<Self> {
        let model = super::catalog::file(root, &spec.model)?;
        let session = Session::builder()?
            .with_intra_threads(2)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?
            .with_inter_threads(1)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?
            .with_memory_pattern(false)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?
            .with_parallel_execution(false)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?
            .with_execution_providers([ort::ep::DirectML::default().build().error_on_failure()])
            .map_err(|e| anyhow::anyhow!(e.to_string()))?
            .commit_from_file(&model)?;
        let dictionary = if let Some(config) = &spec.config {
            let value: serde_yaml::Value =
                serde_yaml::from_slice(&std::fs::read(super::catalog::file(root, config)?)?)?;
            value["PostProcess"]["character_dict"]
                .as_sequence()
                .context("missing reader dictionary")?
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .map(str::to_owned)
                        .context("invalid dictionary token")
                })
                .collect::<Result<Vec<_>>>()?
        } else {
            session
                .metadata()?
                .custom("character")
                .context("reader model is missing its character dictionary")?
                .lines()
                .map(str::to_owned)
                .collect()
        };
        let characters = std::iter::once(String::new())
            .chain(dictionary)
            .chain(std::iter::once(" ".into()))
            .collect::<Vec<_>>();
        if characters.len() < 3 {
            bail!("reader dictionary is empty");
        }
        let mut reader = Self {
            session,
            characters,
            reverse: spec.reverse,
            input: Vec::new(),
        };
        // Warm the same bounded tensor shapes used for ordinary line batches.
        for (count, width) in [(1, 320), (BATCH, 320), (BATCH, 640)] {
            let tensor = Tensor::from_array((
                vec![count, 3, HEIGHT, width],
                vec![0.0_f32; count * 3 * HEIGHT * width],
            ))?;
            reader.session.run(ort::inputs![tensor])?;
        }
        Ok(reader)
    }

    pub(crate) fn recognize(
        &mut self,
        capture_id: u64,
        crops: &[&RgbImage],
        regions: &[Region],
        cancel: &AtomicBool,
        mut emit: impl FnMut(&Completion) -> Result<()>,
    ) -> Result<()> {
        // Plan from geometry, retaining every admitted region. Sorting only
        // reduces padding; completion identity does not depend on batch order.
        let mut order = (0..regions.len()).collect::<Vec<_>>();
        let ratio = |index: usize| crops[index].width() as f32 / crops[index].height() as f32;
        order.sort_by(|&a, &b| ratio(a).total_cmp(&ratio(b)));
        for indices in order.chunks(BATCH) {
            check_cancel(cancel)?;
            let mut tiles = Vec::new();
            for (index, &source) in indices.iter().enumerate() {
                tiles.extend(
                    lines::prepare(crops[source])?
                        .into_iter()
                        .map(|tile| (index, tile)),
                );
            }
            let mut readings = vec![Decoder::default(); indices.len()];
            for batch in tiles.chunks(BATCH) {
                check_cancel(cancel)?;
                let width = batch
                    .iter()
                    .map(|(_, tile)| tile.image.width() as usize)
                    .max()
                    .unwrap_or(320)
                    .max(320);
                let plane = HEIGHT * width;
                self.input.resize(batch.len() * 3 * plane, 0.0);
                self.input.fill(0.0);
                for (index, (_, tile)) in batch.iter().enumerate() {
                    for (x, y, pixel) in tile.image.enumerate_pixels() {
                        for channel in 0..3 {
                            self.input[index * 3 * plane
                                + channel * plane
                                + y as usize * width
                                + x as usize] = f32::from(pixel[2 - channel]) / 127.5 - 1.0;
                        }
                    }
                }
                check_cancel(cancel)?;
                let tensor = TensorRef::from_array_view((
                    [batch.len(), 3, HEIGHT, width],
                    self.input.as_slice(),
                ))?;
                let outputs = self.session.run(ort::inputs![tensor])?;
                let (shape, scores) = outputs[0].try_extract_tensor::<f32>()?;
                let dimensions = shape.as_ref();
                let compact = outputs.len() == 2;
                if dimensions.len() != 3
                    || dimensions[0] != batch.len() as i64
                    || dimensions[1] <= 0
                    || dimensions[2]
                        != if compact {
                            3
                        } else {
                            self.characters.len() as i64
                        }
                {
                    bail!("reader returned an invalid batch shape");
                }
                let classes = dimensions[2] as usize;
                let compact_indices = if compact {
                    let (index_shape, values) = outputs[1].try_extract_tensor::<i64>()?;
                    if index_shape.as_ref() != dimensions {
                        bail!("invalid compact reader indices");
                    }
                    Some(values)
                } else {
                    None
                };
                let stride = dimensions[1] as usize * classes;
                for (batch_index, ((index, tile), probabilities)) in
                    batch.iter().zip(scores.chunks_exact(stride)).enumerate()
                {
                    check_cancel(cancel)?;
                    for (step, row) in probabilities.chunks_exact(classes).enumerate() {
                        if tile.owns_step(step, dimensions[1] as usize, width) {
                            if let Some(tokens) = compact_indices {
                                if row.iter().any(|v| !v.is_finite()) {
                                    bail!("invalid compact reader scores");
                                }
                                let token =
                                    usize::try_from(tokens[batch_index * stride + step * classes])?;
                                readings[*index].token(token, &self.characters)?;
                            } else {
                                readings[*index].push(row, &self.characters)?;
                            }
                        }
                    }
                }
            }
            for (&index, decoded) in indices.iter().zip(readings) {
                check_cancel(cancel)?;
                let text = if self.reverse {
                    reverse_directional(&decoded.text)
                } else {
                    decoded.text
                };
                let text = text.trim().to_owned();
                let reading = if text.is_empty() {
                    Reading::Unresolved("reader produced no text".into())
                } else {
                    Reading::Text(text)
                };
                emit(&Completion {
                    capture_id,
                    region_id: regions[index].id,
                    reading,
                })?;
            }
        }
        Ok(())
    }
}

fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Acquire) {
        bail!("capture cancelled");
    }
    Ok(())
}

#[derive(Clone)]
struct Decoder {
    previous: usize,
    text: String,
}

impl Default for Decoder {
    fn default() -> Self {
        Self {
            previous: usize::MAX,
            text: String::new(),
        }
    }
}

impl Decoder {
    fn push(&mut self, row: &[f32], characters: &[String]) -> Result<()> {
        if row.len() != characters.len() || row.iter().any(|score| !score.is_finite()) {
            bail!("reader returned invalid scores");
        }
        let index = row
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .context("reader returned an empty timestep")?
            .0;
        self.token(index, characters)
    }

    fn token(&mut self, index: usize, characters: &[String]) -> Result<()> {
        if index >= characters.len() {
            bail!("reader token exceeds dictionary");
        }
        if index != 0 && index != self.previous {
            self.text.push_str(&characters[index]);
        }
        self.previous = index;
        if self.text.len() > MAX_REGION_TEXT_BYTES || self.text.contains('\0') {
            bail!("reader output exceeds the text contract");
        }
        Ok(())
    }
}

fn reverse_directional(text: &str) -> String {
    let mut groups = Vec::new();
    let mut run = String::new();
    for character in text.chars() {
        if character.is_ascii_alphanumeric()
            || matches!(character, ' ' | ':' | '*' | '.' | '/' | '%' | '+' | '-')
        {
            run.push(character);
        } else {
            if !run.is_empty() {
                groups.push(std::mem::take(&mut run));
            }
            groups.push(character.to_string());
        }
    }
    if !run.is_empty() {
        groups.push(run);
    }
    groups.into_iter().rev().collect()
}

#[cfg(test)]
fn decode(scores: &[f32], characters: &[String]) -> Result<String> {
    if characters.is_empty() || !scores.len().is_multiple_of(characters.len()) {
        bail!("reader output does not match its dictionary");
    }
    let mut decoder = Decoder::default();
    for row in scores.chunks_exact(characters.len()) {
        decoder.push(row, characters)?;
    }
    Ok(decoder.text.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn blank_separates_repeated_characters_without_a_confidence_gate() {
        let characters = vec!["".into(), "a".into(), "b".into()];
        assert_eq!(
            decode(
                &[0.1, 0.2, 0.1, 0.1, 0.2, 0.1, 0.3, 0.2, 0.1, 0.1, 0.2, 0.1],
                &characters
            )
            .unwrap(),
            "aa"
        );
    }
    #[test]
    fn invalid_output_fails_instead_of_assigning_text() {
        let characters = vec!["".into(), "x".into()];
        assert!(decode(&[0.0], &characters).is_err());
        assert!(decode(&[0.0, f32::NAN], &characters).is_err());
        assert_eq!(decode(&[1.0, 0.0], &characters).unwrap(), "");
    }

    #[test]
    fn decoder_continues_across_windows_without_text_deduplication() {
        let characters = vec!["".into(), "a".into()];
        let mut decoder = Decoder::default();
        decoder.push(&[0.0, 1.0], &characters).unwrap();
        decoder.push(&[0.0, 1.0], &characters).unwrap();
        assert_eq!(decoder.text, "a");
        decoder.push(&[1.0, 0.0], &characters).unwrap();
        decoder.push(&[0.0, 1.0], &characters).unwrap();
        assert_eq!(decoder.text, "aa");
    }
}
