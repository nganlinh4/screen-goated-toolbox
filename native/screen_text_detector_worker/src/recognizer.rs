use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use image::RgbImage;
use image::imageops::FilterType;
use ort::session::Session;
use ort::session::builder::GraphOptimizationLevel;
use ort::value::Tensor;

pub(crate) const INPUT_HEIGHT: u32 = 48;
const MIN_INPUT_WIDTH: u32 = 32;
const MAX_INPUT_WIDTH: u32 = 1_600;
const MAX_CHARACTERS: usize = 1_024;
const MAX_BATCH_SIZE: usize = 16;

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
}

#[derive(Clone, Copy)]
pub(crate) enum Acceleration {
    Cpu,
    DirectMl,
}

impl TextRecognizer {
    pub(crate) fn load(
        model: &Path,
        config: &Path,
        reverse_output: bool,
        acceleration: Acceleration,
    ) -> Result<Self> {
        let builder = Session::builder()?
            .with_memory_pattern(false)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        let mut builder = match acceleration {
            Acceleration::Cpu => builder,
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
        })
    }

    pub(crate) fn recognize_batch(&mut self, sources: &[RgbImage]) -> Result<Vec<Recognition>> {
        let plan = RecognitionPlan::new(sources)?;
        let prepared = plan
            .tiles
            .iter()
            .map(|tile| prepare(&tile.image))
            .collect::<Result<Vec<_>>>()?;
        let mut order = (0..prepared.len()).collect::<Vec<_>>();
        order.sort_unstable_by_key(|index| prepared[*index].width);
        let mut recognized = (0..prepared.len()).map(|_| None).collect::<Vec<_>>();
        for indices in order.chunks(MAX_BATCH_SIZE) {
            let width = indices
                .iter()
                .map(|index| prepared[*index].width)
                .max()
                .context("recognizer batch is empty")?;
            let chw = batch_tensor(&prepared, indices, width)?;
            let tensor = Tensor::from_array((
                vec![indices.len(), 3, INPUT_HEIGHT as usize, width as usize],
                chw,
            ))?;
            let outputs = self.session.run(ort::inputs![tensor])?;
            let (shape, scores) = outputs[0]
                .try_extract_tensor::<f32>()
                .context("read PaddleOCR recognition scores")?;
            let dimensions = shape.as_ref();
            if dimensions.len() != 3
                || dimensions[0] != indices.len() as i64
                || dimensions[2] != self.characters.len() as i64
            {
                bail!("PaddleOCR returned an unexpected recognition shape");
            }
            let steps = dimensions[1] as usize;
            let result_size = steps.saturating_mul(self.characters.len());
            for (batch_index, source_index) in indices.iter().enumerate() {
                let start = batch_index.saturating_mul(result_size);
                let end = start.saturating_add(result_size);
                recognized[*source_index] = Some(decode(
                    scores
                        .get(start..end)
                        .context("recognizer output batch is truncated")?,
                    steps,
                    &self.characters,
                    false,
                )?);
            }
        }
        let recognized = recognized
            .into_iter()
            .map(|result| result.context("recognizer omitted a batch result"))
            .collect::<Result<Vec<_>>>()?;
        plan.assemble(recognized, self.reverse_output)
    }
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
    fn new(sources: &[RgbImage]) -> Result<Self> {
        let mut tiles = Vec::new();
        for (source_index, source) in sources.iter().enumerate() {
            if source.width() == 0 || source.height() == 0 {
                bail!("recognizer crop is empty");
            }
            let ranges = recognition_ranges(source);
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

fn recognition_ranges(source: &RgbImage) -> Vec<RecognitionRange> {
    let max_source_width =
        (MAX_INPUT_WIDTH as u64 * source.height() as u64 / INPUT_HEIGHT as u64) as u32;
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

struct PreparedTextLine {
    width: u32,
    chw: Vec<f32>,
}

fn prepare(source: &RgbImage) -> Result<PreparedTextLine> {
    if source.width() == 0 || source.height() == 0 {
        bail!("recognizer crop is empty");
    }
    let scaled =
        (source.width() as f64 * f64::from(INPUT_HEIGHT) / source.height() as f64).ceil() as u32;
    let resized_width = scaled.clamp(1, MAX_INPUT_WIDTH);
    let input_width = resized_width
        .div_ceil(32)
        .saturating_mul(32)
        .clamp(MIN_INPUT_WIDTH, MAX_INPUT_WIDTH);
    let resized =
        image::imageops::resize(source, resized_width, INPUT_HEIGHT, FilterType::Triangle);
    let plane = input_width as usize * INPUT_HEIGHT as usize;
    let mut chw = vec![0.0_f32; plane * 3];
    for (y, row) in resized.rows().enumerate() {
        for (x, pixel) in row.enumerate() {
            let offset = y * input_width as usize + x;
            let bgr = [pixel[2], pixel[1], pixel[0]];
            for channel in 0..3 {
                chw[channel * plane + offset] = f32::from(bgr[channel]) / 127.5 - 1.0;
            }
        }
    }
    Ok(PreparedTextLine {
        width: input_width,
        chw,
    })
}

fn batch_tensor(
    prepared: &[PreparedTextLine],
    indices: &[usize],
    batch_width: u32,
) -> Result<Vec<f32>> {
    let height = INPUT_HEIGHT as usize;
    let batch_plane = batch_width as usize * height;
    let mut tensor = vec![0.0_f32; indices.len() * batch_plane * 3];
    for (batch_index, source_index) in indices.iter().enumerate() {
        let source = prepared
            .get(*source_index)
            .context("recognizer batch index is invalid")?;
        let source_plane = source.width as usize * height;
        for channel in 0..3 {
            for row in 0..height {
                let source_start = channel * source_plane + row * source.width as usize;
                let target_start = batch_index * batch_plane * 3
                    + channel * batch_plane
                    + row * batch_width as usize;
                tensor[target_start..target_start + source.width as usize].copy_from_slice(
                    &source.chw[source_start..source_start + source.width as usize],
                );
            }
        }
    }
    Ok(tensor)
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
mod tests {
    use super::*;

    #[test]
    fn ctc_decode_removes_blank_and_repeated_classes() {
        let characters = vec![String::new(), "A".to_string(), "B".to_string()];
        let scores = [0.0, 0.9, 0.1, 0.0, 0.8, 0.2, 0.9, 0.05, 0.05, 0.0, 0.1, 0.9];
        let result = decode(&scores, 4, &characters, false).unwrap();
        assert_eq!(result.text, "AB");
        assert!((result.confidence - 0.9).abs() < 0.001);
    }

    #[test]
    fn ctc_decode_can_reverse_directional_groups() {
        let characters = vec![
            String::new(),
            "A".to_string(),
            "•".to_string(),
            "B".to_string(),
        ];
        let scores = [
            0.0, 0.9, 0.05, 0.05, 0.0, 0.05, 0.9, 0.05, 0.0, 0.05, 0.05, 0.9,
        ];
        let result = decode(&scores, 3, &characters, true).unwrap();
        assert_eq!(result.text, "B•A");
    }

    #[test]
    fn ctc_decode_retains_close_character_candidates_as_script_evidence() {
        let characters = vec![
            String::new(),
            "K".to_string(),
            "К".to_string(),
            "A".to_string(),
            "А".to_string(),
        ];
        let scores = [0.0, 0.60, 0.39, 0.01, 0.0, 0.0, 0.01, 0.0, 0.58, 0.41];
        let result = decode(&scores, 2, &characters, false).unwrap();
        assert_eq!(result.text, "KA");
        assert!(result.script_evidence.contains(&('К' as u32)));
        assert!(result.script_evidence.contains(&('А' as u32)));
        assert_eq!(result.token_count, 2);
    }

    #[test]
    fn recognition_input_is_stride_aligned_and_bounded() {
        let narrow = prepare(&RgbImage::new(8, 40)).unwrap();
        let wide = prepare(&RgbImage::new(4_000, 20)).unwrap();
        assert_eq!(narrow.width, MIN_INPUT_WIDTH);
        assert_eq!(wide.width, MAX_INPUT_WIDTH);
    }

    #[test]
    fn extreme_line_is_tiled_without_resizing_any_tile_past_the_cap() {
        let source = RgbImage::from_pixel(1_427, 20, image::Rgb([255, 255, 255]));
        let ranges = recognition_ranges(&source);
        assert_eq!(ranges.first().unwrap().start, 0);
        assert_eq!(ranges.last().unwrap().end, source.width());
        assert!(ranges.len() > 1);
        assert!(ranges.windows(2).all(|pair| pair[0].end == pair[1].start));
        for range in ranges {
            let crop = image::imageops::crop_imm(
                &source,
                range.start,
                0,
                range.end - range.start,
                source.height(),
            )
            .to_image();
            let expected = crop.width() as f64 * f64::from(INPUT_HEIGHT) / f64::from(crop.height());
            assert!(expected <= f64::from(MAX_INPUT_WIDTH));
        }
    }

    #[test]
    fn tiled_results_reassemble_in_source_order_with_weighted_confidence() {
        let plan = RecognitionPlan {
            source_count: 1,
            tiles: vec![
                RecognitionTile {
                    source_index: 0,
                    image: RgbImage::new(10, 10),
                    separated_from_previous: false,
                },
                RecognitionTile {
                    source_index: 0,
                    image: RgbImage::new(10, 10),
                    separated_from_previous: true,
                },
            ],
        };
        let results = plan
            .assemble(
                vec![
                    Recognition {
                        text: "first part".to_string(),
                        confidence: 0.9,
                        script_evidence: vec![1],
                        token_count: 9,
                    },
                    Recognition {
                        text: "second part".to_string(),
                        confidence: 0.6,
                        script_evidence: vec![2],
                        token_count: 10,
                    },
                ],
                false,
            )
            .unwrap();
        assert_eq!(results[0].text, "first part second part");
        assert_eq!(results[0].token_count, 19);
        assert!((results[0].confidence - (14.1 / 19.0)).abs() < 0.001);
        assert_eq!(results[0].script_evidence, vec![1, 2]);
    }

    #[test]
    fn batch_tensor_preserves_each_source_plane_and_pads_with_neutral_values() {
        let first = PreparedTextLine {
            width: 32,
            chw: vec![1.0; 32 * INPUT_HEIGHT as usize * 3],
        };
        let second = PreparedTextLine {
            width: 64,
            chw: vec![2.0; 64 * INPUT_HEIGHT as usize * 3],
        };
        let tensor = batch_tensor(&[first, second], &[0, 1], 64).unwrap();
        let plane = 64 * INPUT_HEIGHT as usize;
        assert_eq!(tensor[0], 1.0);
        assert_eq!(tensor[31], 1.0);
        assert_eq!(tensor[32], 0.0);
        assert_eq!(tensor[plane * 3], 2.0);
    }
}
