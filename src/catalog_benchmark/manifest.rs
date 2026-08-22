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
    pub localization_cases: Vec<LocalizationCase>,
    #[serde(skip)]
    root: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LocalizationCase {
    pub id: String,
    pub difficulty: u8,
    pub image: String,
    pub target_language: String,
    pub regions: Vec<LocalizationRegion>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LocalizationRegion {
    pub source_text: String,
    pub box_px: [u32; 4],
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TextCase {
    pub id: String,
    pub difficulty: u8,
    pub task: TextTask,
    pub instruction: String,
    #[serde(default)]
    pub source_language: Option<String>,
    #[serde(default)]
    pub target_language: Option<String>,
    pub input: String,
    pub reference: String,
    pub required_terms: Vec<String>,
    #[serde(default)]
    pub required_exact: Vec<String>,
    #[serde(default)]
    pub required_exact_any: Vec<Vec<String>>,
    #[serde(default)]
    pub forbidden_terms: Vec<String>,
    #[serde(default)]
    pub expected_line_count: Option<usize>,
    pub rubric: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum TextTask {
    Classification,
    Extraction,
    Translation,
    Rewrite,
    Summarization,
    StructuredExtraction,
    Reasoning,
    Synthesis,
}

impl TextTask {
    const ALL: [Self; 8] = [
        Self::Classification,
        Self::Extraction,
        Self::Translation,
        Self::Rewrite,
        Self::Summarization,
        Self::StructuredExtraction,
        Self::Reasoning,
        Self::Synthesis,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Classification => "classification",
            Self::Extraction => "extraction",
            Self::Translation => "translation",
            Self::Rewrite => "rewrite",
            Self::Summarization => "summarization",
            Self::StructuredExtraction => "structured-extraction",
            Self::Reasoning => "reasoning",
            Self::Synthesis => "synthesis",
        }
    }

    pub fn reference_similarity_weight(self) -> f64 {
        match self {
            Self::Rewrite | Self::Summarization | Self::Synthesis => 0.35,
            _ => 0.65,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CoordinateCase {
    pub id: String,
    pub difficulty: u8,
    pub image: String,
    pub target: String,
    pub context: String,
    pub box_px: [f64; 4],
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OcrCase {
    pub id: String,
    pub difficulty: u8,
    pub image: String,
    pub input_mode: OcrInputMode,
    #[serde(default)]
    pub crop_px: Option<[u32; 4]>,
    pub instruction: String,
    pub reference: String,
    #[serde(default)]
    pub accepted_references: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum OcrInputMode {
    /// A Windows selection or clipboard image, normalized to PNG by the app.
    ScreenCropPng,
    /// A dropped image file, whose original bytes are retained for providers.
    OriginalFile,
}

impl OcrInputMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ScreenCropPng => "screen-crop-png",
            Self::OriginalFile => "original-file",
        }
    }
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
        let latency_policy = super::history::vision_latency_policy()?;
        ensure!(
            self.version == 4,
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
        validate_localization_levels(
            self.localization_cases
                .iter()
                .map(|case| (&case.id, case.difficulty)),
        )?;

        for case in &self.text_cases {
            ensure!(
                !case.instruction.trim().is_empty(),
                "{} has no task instruction",
                case.id
            );
            ensure!(
                !case.input.trim().is_empty(),
                "{} has no text input",
                case.id
            );
            ensure!(
                !case.reference.trim().is_empty(),
                "{} has no text reference",
                case.id
            );
            ensure!(
                case.required_exact_any.iter().all(|alternatives| {
                    !alternatives.is_empty()
                        && alternatives
                            .iter()
                            .all(|alternative| !alternative.is_empty())
                }),
                "{} has an empty exact-alternative group",
                case.id
            );
            match case.task {
                TextTask::Translation => ensure!(
                    case.source_language
                        .as_deref()
                        .is_some_and(|value| !value.trim().is_empty())
                        && case
                            .target_language
                            .as_deref()
                            .is_some_and(|value| !value.trim().is_empty()),
                    "{} translation needs source and target languages",
                    case.id
                ),
                _ => ensure!(
                    case.source_language.is_none() && case.target_language.is_none(),
                    "{} non-translation task cannot claim a translation language pair",
                    case.id
                ),
            }
        }

        let task_kinds = self
            .text_cases
            .iter()
            .map(|case| case.task)
            .collect::<HashSet<_>>();
        ensure!(
            TextTask::ALL.iter().all(|task| task_kinds.contains(task)),
            "text suite must cover every declared task family"
        );
        ensure!(
            self.text_cases
                .iter()
                .filter(|case| case.task == TextTask::Translation)
                .count()
                >= 2,
            "text suite must retain multiple translation cases"
        );
        ensure!(
            has_translation_pair(&self.text_cases, "Korean", "Vietnamese"),
            "text suite must retain Korean-to-Vietnamese coverage"
        );
        ensure!(
            has_translation_pair(&self.text_cases, "Simplified Chinese", "Vietnamese"),
            "text suite must retain Chinese-to-Vietnamese coverage"
        );
        let easiest = self
            .text_cases
            .iter()
            .find(|case| case.difficulty == 1)
            .expect("validated difficulty");
        ensure!(
            easiest.task == TextTask::Classification
                && easiest.input.split_whitespace().count() <= 8,
            "difficulty-one text case must stay a several-word task"
        );
        let hardest = self
            .text_cases
            .iter()
            .find(|case| case.difficulty == self.rounds)
            .expect("validated difficulty");
        ensure!(
            hardest.task == TextTask::Synthesis
                && hardest.input.chars().count() >= 500
                && hardest.rubric.len() >= 5
                && hardest.expected_line_count.is_some(),
            "final text case must remain long, multi-constraint, and structured"
        );

        let mut representative_coordinate_cases = 0;
        for case in &self.coordinate_cases {
            let path = self.image_path(&case.image);
            let image = image::open(&path).with_context(|| format!("decode {}", path.display()))?;
            if image.width().max(image.height()) <= latency_policy.max_edge_px {
                representative_coordinate_cases += 1;
            }
            ensure!(
                !case.target.trim().is_empty() && !case.context.trim().is_empty(),
                "{} needs both a target and realistic task context",
                case.id
            );
            let [x, y, width, height] = case.box_px;
            ensure!(
                x >= 0.0 && y >= 0.0 && width >= 2.0 && height >= 2.0,
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
        let mut representative_ocr_cases = 0;
        let mut screen_crop_cases = 0;
        let mut original_file_cases = 0;
        for case in &self.ocr_cases {
            let path = self.image_path(&case.image);
            let image = image::open(&path).with_context(|| format!("decode {}", path.display()))?;
            ensure!(
                case.input_mode != OcrInputMode::OriginalFile || case.crop_px.is_none(),
                "{} cannot crop an original-file input; app crops are screen-crop PNG inputs",
                case.id
            );
            match case.input_mode {
                OcrInputMode::ScreenCropPng => screen_crop_cases += 1,
                OcrInputMode::OriginalFile => original_file_cases += 1,
            }
            let (effective_width, effective_height) = case
                .crop_px
                .map_or((image.width(), image.height()), |[_, _, width, height]| {
                    (width, height)
                });
            if effective_width.max(effective_height) <= latency_policy.max_edge_px {
                representative_ocr_cases += 1;
            }
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
        for case in &self.localization_cases {
            let path = self.image_path(&case.image);
            let image = image::open(&path).with_context(|| format!("decode {}", path.display()))?;
            ensure!(
                !case.target_language.trim().is_empty(),
                "{} has no target language",
                case.id
            );
            ensure!(!case.regions.is_empty(), "{} has no gold regions", case.id);
            for region in &case.regions {
                ensure!(
                    !region.source_text.trim().is_empty(),
                    "{} has an empty source region",
                    case.id
                );
                let [x, y, width, height] = region.box_px;
                ensure!(width >= 2 && height >= 2, "{} has an empty region", case.id);
                ensure!(
                    x.saturating_add(width) <= image.width()
                        && y.saturating_add(height) <= image.height(),
                    "{} region is outside its {}x{} image",
                    case.id,
                    image.width(),
                    image.height()
                );
            }
        }
        ensure!(
            representative_coordinate_cases >= latency_policy.minimum_cases_per_suite,
            "coordinate suite needs at least {} representative images at or below {}px",
            latency_policy.minimum_cases_per_suite,
            latency_policy.max_edge_px
        );
        ensure!(
            representative_ocr_cases >= latency_policy.minimum_cases_per_suite,
            "OCR suite needs at least {} representative images at or below {}px after cropping",
            latency_policy.minimum_cases_per_suite,
            latency_policy.max_edge_px
        );
        ensure!(
            screen_crop_cases >= 5 && original_file_cases >= 3,
            "OCR suite must retain a daily-use mix of screen crops and original files"
        );
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

fn has_translation_pair(cases: &[TextCase], source: &str, target: &str) -> bool {
    cases.iter().any(|case| {
        case.task == TextTask::Translation
            && case.source_language.as_deref() == Some(source)
            && case.target_language.as_deref() == Some(target)
    })
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

fn validate_localization_levels<'a>(cases: impl Iterator<Item = (&'a String, u8)>) -> Result<()> {
    let cases = cases.collect::<Vec<_>>();
    ensure!(
        cases.len() >= 3,
        "localization suite must contain at least one case per level"
    );
    let ids = cases
        .iter()
        .map(|(id, _)| id.as_str())
        .collect::<HashSet<_>>();
    ensure!(
        ids.len() == cases.len(),
        "localization case IDs must be unique"
    );
    let levels = cases
        .iter()
        .map(|(_, difficulty)| *difficulty)
        .collect::<HashSet<_>>();
    ensure!(
        levels == HashSet::from([1, 2, 3]),
        "localization difficulties must cover exactly levels 1 through 3"
    );
    Ok(())
}
