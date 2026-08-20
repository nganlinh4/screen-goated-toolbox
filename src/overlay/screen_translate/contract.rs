//! Translation-only model contract backed by locally owned visual cells.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

pub(crate) const MAX_CANDIDATES: usize = 240;
pub(crate) const MAX_SOURCE_CANDIDATES: usize = 2;
const MAX_TEXT_CHARS: usize = 2_000;

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct TranslationDocument {
    pub regions: Vec<TranslationRegion>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct TranslationRegion {
    #[serde(skip)]
    pub id: u16,
    pub member_ids: Vec<u16>,
    pub member_joins: Vec<MemberJoin>,
    pub selections: Vec<TranslationSelection>,
    pub semantic_role: SemanticRole,
    pub source_text: String,
    pub translated_segments: Vec<String>,
    #[serde(rename = "box_2d")]
    pub bounds: NormalizedBounds,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_color: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct TranslationSelection {
    pub region_id: u16,
    pub candidate_id: String,
    pub source_text: String,
    pub bounds: NormalizedBounds,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SemanticRole {
    Standalone,
    Heading,
    Paragraph,
    ListItem,
    Label,
    Value,
    Dialogue,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MemberJoin {
    SameLine,
    WrappedLine,
    SameColumn,
    SameBlock,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DetectedTextRegion {
    pub id: u16,
    pub bounds: NormalizedBounds,
    pub source_text: String,
    pub source_alternatives: Vec<String>,
    pub recognition: RecognitionEvidence,
    pub appearance: Option<super::appearance::VisualSignature>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct RecognitionEvidence {
    pub locator_confidence: f32,
    pub selected_confidence: f32,
    pub competing_confidence: f32,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(from = "[u16; 4]", into = "[u16; 4]")]
pub(crate) struct NormalizedBounds {
    pub left: u16,
    pub top: u16,
    pub right: u16,
    pub bottom: u16,
}

impl From<[u16; 4]> for NormalizedBounds {
    fn from([top, left, bottom, right]: [u16; 4]) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }
}

impl From<NormalizedBounds> for [u16; 4] {
    fn from(bounds: NormalizedBounds) -> Self {
        [bounds.top, bounds.left, bounds.bottom, bounds.right]
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TranslationResponse {
    translations: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum TranslationResponseEnvelope {
    Object(TranslationResponse),
    Array(Vec<serde_json::Value>),
}

pub(crate) fn response_schema(region_count: usize) -> serde_json::Value {
    super::schema::response_schema(region_count.clamp(1, MAX_CANDIDATES), MAX_TEXT_CHARS)
}

pub(crate) fn prompt_with_instruction(
    target_language: &str,
    translation_instruction: &str,
    candidates: &[DetectedTextRegion],
) -> Result<String> {
    if candidates.is_empty() || candidates.len() > MAX_CANDIDATES {
        bail!("detector candidate count is outside the translation contract");
    }
    let cells = super::cell_proposals::propose(candidates)
        .into_iter()
        .map(|proposal| {
            let members = proposal
                .member_ids_in_reading_order
                .iter()
                .filter_map(|id| candidates.iter().find(|candidate| candidate.id == *id))
                .map(|candidate| {
                    let slot = candidates
                        .iter()
                        .position(|item| item.id == candidate.id)
                        .expect("proposal members come from the candidate set");
                    serde_json::json!({
                        "slot": slot,
                        "text": candidate.source_text,
                        "ocrReadings": candidate.source_alternatives.iter().skip(1).collect::<Vec<_>>()
                    })
                })
                .collect::<Vec<_>>();
            serde_json::json!({
                "members": members
            })
        })
        .collect::<Vec<_>>();
    let instruction = translation_instruction
        .replace("{target_language}", target_language)
        .trim()
        .to_string();
    Ok(format!(
        "Translation preference:\n{instruction}\n\n\
         Translate every supplied member completely into {target_language}. When ocrReadings differ, use the most complete coherent reading; they are alternate OCR observations of the same pixels, not additional text. Cells provide context only. Return one translations string per slot, in exact numeric slot order from 0 through {}. Each string must contain only its corresponding member translation; never move, merge, duplicate, or drop content between slots or cells. Preserve names, usernames, handles, codes, punctuation, tone, and mixed-language meaning. Do not summarize, abbreviate, or invent. Geometry and rendering are handled locally.\n\
         Return exactly one JSON object shaped as {{\"translations\":[\"slot 0 translation\",\"slot 1 translation\"]}}, extended to every supplied slot.\n\
         Cells:\n{}",
        candidates.len() - 1,
        serde_json::to_string(&cells)?
    ))
}

pub(crate) fn parse_response(
    response: &str,
    candidates: &[DetectedTextRegion],
) -> Result<TranslationDocument> {
    let envelope: TranslationResponseEnvelope = serde_json::from_str(unwrap_json(response))
        .context("response did not match the translation schema")?;
    let values = match envelope {
        TranslationResponseEnvelope::Object(response) => response.translations,
        TranslationResponseEnvelope::Array(members) => members,
    };
    let mut regions = Vec::new();
    for (index, value) in values.into_iter().take(candidates.len()).enumerate() {
        let Ok(translation) = serde_json::from_value::<String>(value) else {
            continue;
        };
        if let Ok(parsed) = validated_translation(index, &translation, candidates) {
            regions.push(parsed);
        }
    }
    regions.sort_by_key(|region| (region.bounds.top, region.bounds.left));
    Ok(TranslationDocument { regions })
}

pub(crate) fn parse_streamed_translation(
    index: usize,
    value: &str,
    candidates: &[DetectedTextRegion],
) -> Result<(u16, TranslationRegion)> {
    let translation: String = serde_json::from_str(value)
        .context("streamed translation did not match the translation schema")?;
    let region = validated_translation(index, &translation, candidates)?;
    Ok((region.id, region))
}

fn validated_translation(
    index: usize,
    translation: &str,
    candidates: &[DetectedTextRegion],
) -> Result<TranslationRegion> {
    let candidate = candidates
        .get(index)
        .context("translation references an unknown local slot")?;
    let selection = TranslationSelection {
        region_id: candidate.id,
        candidate_id: format!("r{}c0", candidate.id),
        source_text: candidate.source_text.clone(),
        bounds: candidate.bounds,
    };
    let translation =
        clean_text(translation, MAX_TEXT_CHARS).context("translation is empty or too long")?;
    build_region(&[selection], &[], &[translation], candidates)
}

fn build_region(
    selections: &[TranslationSelection],
    joins: &[MemberJoin],
    translated_segments: &[String],
    candidates: &[DetectedTextRegion],
) -> Result<TranslationRegion> {
    let member_ids = selections
        .iter()
        .map(|item| item.region_id)
        .collect::<Vec<_>>();
    let bounds = super::cell_validation::validate_cell_members(&member_ids, joins, candidates)?;
    let source_text = selections
        .iter()
        .map(|item| item.source_text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    Ok(TranslationRegion {
        id: member_ids[0],
        member_ids,
        member_joins: joins.to_vec(),
        selections: selections.to_vec(),
        semantic_role: if selections.len() > 1 {
            SemanticRole::Paragraph
        } else {
            SemanticRole::Standalone
        },
        source_text,
        translated_segments: translated_segments.to_vec(),
        bounds,
        background_color: None,
        text_color: None,
    })
}

fn unwrap_json(response: &str) -> &str {
    let trimmed = response.trim();
    let Some(fenced) = trimmed.strip_prefix("```") else {
        return trimmed;
    };
    let body = fenced
        .strip_prefix("json")
        .or_else(|| fenced.strip_prefix("JSON"))
        .unwrap_or(fenced)
        .trim_start_matches([' ', '\t', '\r', '\n']);
    body.strip_suffix("```").unwrap_or(body).trim()
}

fn clean_text(value: &str, max_chars: usize) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty() && trimmed.chars().count() <= max_chars).then(|| trimmed.to_string())
}

pub(crate) fn text_is_source_equivalent(left: &str, right: &str) -> bool {
    comparable_text(left) == comparable_text(right)
}

fn comparable_text(text: &str) -> String {
    text.chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidates() -> Vec<DetectedTextRegion> {
        vec![
            DetectedTextRegion {
                id: 1,
                bounds: [10, 10, 30, 200].into(),
                source_text: "first line".into(),
                source_alternatives: vec!["first line".into()],
                recognition: Default::default(),
                appearance: None,
            },
            DetectedTextRegion {
                id: 2,
                bounds: [34, 10, 54, 200].into(),
                source_text: "second line".into(),
                source_alternatives: vec!["second line".into()],
                recognition: Default::default(),
                appearance: None,
            },
        ]
    }

    #[test]
    fn prompt_contains_translation_only_cells() {
        let prompt = prompt_with_instruction(
            "Vietnamese",
            "Translate to {target_language}.",
            &candidates(),
        )
        .unwrap();
        assert!(prompt.contains(r#""slot":0"#));
        assert!(prompt.contains(r#""slot":1"#));
        assert!(prompt.contains(r#""text":"second line""#));
        assert!(prompt.contains(r#""ocrReadings":[]"#));
        assert!(!prompt.contains("candidateIds"));
        assert!(!prompt.contains("memberJoins"));
    }

    #[test]
    fn member_translations_preserve_exact_local_correspondence() {
        let parsed =
            parse_response(r#"{"translations":["dòng một","dòng hai"]}"#, &candidates()).unwrap();
        assert_eq!(parsed.regions.len(), 2);
        assert_eq!(parsed.regions[0].member_ids, [1]);
        assert_eq!(parsed.regions[1].translated_segments, ["dòng hai"]);
    }

    #[test]
    fn source_equivalence_ignores_layout_whitespace_and_punctuation() {
        assert!(text_is_source_equivalent(
            "첫째 줄\n둘째 줄.",
            "첫째 줄 둘째 줄"
        ));
        assert!(!text_is_source_equivalent(
            "첫째 줄 둘째 줄.",
            "Dòng thứ nhất, dòng thứ hai."
        ));
    }

    #[test]
    fn unknown_member_is_rejected_without_losing_valid_members() {
        let parsed = parse_response(r#"{"translations":["dòng một",3]}"#, &candidates()).unwrap();
        assert_eq!(parsed.regions.len(), 1);
    }
}
