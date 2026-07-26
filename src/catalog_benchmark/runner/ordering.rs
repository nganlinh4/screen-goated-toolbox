use super::{ModelConfig, OcrCase, TextCase};
use crate::catalog_benchmark::manifest::CoordinateCase;

pub(super) fn case_at_difficulty<T>(cases: &[T], difficulty: u8) -> &T
where
    T: Difficulty,
{
    cases
        .iter()
        .find(|case| case.difficulty() == difficulty)
        .expect("validated difficulty")
}

pub(super) trait Difficulty {
    fn difficulty(&self) -> u8;
}

impl Difficulty for TextCase {
    fn difficulty(&self) -> u8 {
        self.difficulty
    }
}

impl Difficulty for CoordinateCase {
    fn difficulty(&self) -> u8 {
        self.difficulty
    }
}

impl Difficulty for OcrCase {
    fn difficulty(&self) -> u8 {
        self.difficulty
    }
}

pub(super) fn rotated(models: &[ModelConfig], round: u8) -> impl Iterator<Item = &ModelConfig> {
    let skip = usize::from(round.saturating_sub(1)) % models.len();
    models.iter().cycle().skip(skip).take(models.len())
}
