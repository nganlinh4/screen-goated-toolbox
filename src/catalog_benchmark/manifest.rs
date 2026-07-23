use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize)]
pub struct Manifest {
    pub version: u32,
    pub rounds: u8,
    pub text_cases: Vec<TextCase>,
    pub coordinate_cases: Vec<CoordinateCase>,
    pub ocr_cases: Vec<OcrCase>,
    #[serde(skip)]
    root: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TextCase {
    pub id: String,
    pub difficulty: u8,
    pub source_language: String,
    pub target_language: String,
    pub input: String,
    pub reference: String,
    pub required_terms: Vec<String>,
    #[serde(default)]
    pub required_exact: Vec<String>,
    #[serde(default)]
    pub forbidden_terms: Vec<String>,
    #[serde(default)]
    pub expected_line_count: Option<usize>,
    pub rubric: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CoordinateCase {
    pub id: String,
    pub difficulty: u8,
    pub image: String,
    pub target: String,
    pub box_px: [f64; 4],
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OcrCase {
    pub id: String,
    pub difficulty: u8,
    pub image: String,
    #[serde(default)]
    pub crop_px: Option<[u32; 4]>,
    pub instruction: String,
    pub reference: String,
    #[serde(default)]
    pub accepted_references: Vec<String>,
}

impl Manifest {
    pub fn load() -> Result<Self> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/catalog-benchmark");
        let bytes = std::fs::read(root.join("manifest.json"))
            .context("read tests/catalog-benchmark/manifest.json")?;
        let mut manifest: Self = serde_json::from_slice(&bytes).context("parse manifest.json")?;
        manifest.root = root;
        Ok(manifest)
    }

    pub fn image_path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.version == 2,
            "unsupported manifest version {}",
            self.version
        );
        ensure!(self.rounds == 10, "catalog benchmark must have ten rounds");
        validate_difficulties(
            "text",
            self.rounds,
            self.text_cases
                .iter()
                .map(|case| (&case.id, case.difficulty)),
        )?;
        validate_difficulties(
            "coordinate",
            self.rounds,
            self.coordinate_cases
                .iter()
                .map(|case| (&case.id, case.difficulty)),
        )?;
        validate_difficulties(
            "ocr",
            self.rounds,
            self.ocr_cases
                .iter()
                .map(|case| (&case.id, case.difficulty)),
        )?;

        for case in &self.coordinate_cases {
            let path = self.image_path(&case.image);
            let image = image::open(&path).with_context(|| format!("decode {}", path.display()))?;
            let [x, y, width, height] = case.box_px;
            ensure!(
                x >= 0.0 && y >= 0.0 && width > 0.0 && height > 0.0,
                "{} has an invalid box",
                case.id
            );
            ensure!(
                x + width <= f64::from(image.width()) && y + height <= f64::from(image.height()),
                "{} box is outside its {}x{} image",
                case.id,
                image.width(),
                image.height()
            );
        }
        for case in &self.ocr_cases {
            let path = self.image_path(&case.image);
            let image = image::open(&path).with_context(|| format!("decode {}", path.display()))?;
            if let Some([x, y, width, height]) = case.crop_px {
                ensure!(width > 0 && height > 0, "{} has an empty OCR crop", case.id);
                ensure!(
                    x.saturating_add(width) <= image.width()
                        && y.saturating_add(height) <= image.height(),
                    "{} crop is outside its {}x{} image",
                    case.id,
                    image.width(),
                    image.height()
                );
            }
            ensure!(
                !case.reference.trim().is_empty(),
                "{} has no OCR reference",
                case.id
            );
            ensure!(
                case.accepted_references
                    .iter()
                    .all(|reference| !reference.trim().is_empty()),
                "{} has a blank alternate OCR reference",
                case.id
            );
        }
        let preset_cases: Vec<_> = self
            .ocr_cases
            .iter()
            .filter(|case| {
                case.instruction == crate::config::preset::defaults::OCR_EXTRACTION_PROMPT
            })
            .collect();
        ensure!(
            preset_cases.len() == 3,
            "exactly three OCR cases must use the canonical OCR preset prompt"
        );
        ensure!(
            preset_cases
                .iter()
                .filter(|case| case.crop_px.is_some())
                .count()
                == 2,
            "exactly two OCR preset cases must use deterministic crops"
        );
        Ok(())
    }
}

fn validate_difficulties<'a>(
    suite: &str,
    rounds: u8,
    cases: impl Iterator<Item = (&'a String, u8)>,
) -> Result<()> {
    let cases: Vec<_> = cases.collect();
    ensure!(
        cases.len() == usize::from(rounds),
        "{suite} suite must contain {rounds} cases"
    );
    let ids: HashSet<_> = cases.iter().map(|(id, _)| id.as_str()).collect();
    ensure!(ids.len() == cases.len(), "{suite} case IDs must be unique");
    let levels: HashSet<_> = cases.iter().map(|(_, difficulty)| *difficulty).collect();
    let expected: HashSet<_> = (1..=rounds).collect();
    ensure!(
        levels == expected,
        "{suite} difficulties must be exactly 1 through {rounds}"
    );
    Ok(())
}
