//! Translation-only model contract backed by locally owned visual cells.

use std::collections::HashSet;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

pub(crate) const MAX_CANDIDATES: usize = 240;
pub(crate) const MAX_SOURCE_CANDIDATES: usize = 3;
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DetectedTextRegion {
    pub id: u16,
    pub bounds: NormalizedBounds,
    pub source_text: String,
    pub source_alternatives: Vec<String>,
    pub appearance: Option<super::appearance::VisualSignature>,
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
    cells: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum TranslationResponseEnvelope {
    Object(TranslationResponse),
    Array(Vec<serde_json::Value>),
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TranslatedCellResponse {
    cell_id: u16,
    translation: String,
    split_after_members: Vec<u16>,
}

pub(crate) fn response_schema(region_count: usize) -> serde_json::Value {
    let cells = region_count.clamp(1, MAX_CANDIDATES);
    super::schema::response_schema(cells, MAX_TEXT_CHARS)
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
                    serde_json::json!({
                        "memberId": candidate.id,
                        "text": candidate.source_text
                    })
                })
                .collect::<Vec<_>>();
            serde_json::json!({
                "cellId": proposal.member_ids_in_reading_order[0],
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
         Translate every supplied cell completely into {target_language}. Return one result for every cellId. A cell is already in reading order. Translate it as one coherent passage. Do not summarize, abbreviate, invent, or move content between cells. Preserve names, codes, punctuation, tone, and mixed-language meaning.\n\
         splitAfterMembers is normally empty. Add a memberId only when the following member begins a genuinely separate item, such as another speaker, control, label, bullet, or paragraph. Use only supplied memberIds, in source order, and never include a cell's final member. Geometry, grouping validation, text distribution, and rendering are handled locally.\n\
         Return only JSON matching the supplied schema.\n\
         Cells:\n{}",
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
        TranslationResponseEnvelope::Object(response) => response.cells,
        TranslationResponseEnvelope::Array(cells) => cells,
    };
    let mut seen = HashSet::new();
    let mut regions = Vec::new();
    for value in values.into_iter().take(MAX_CANDIDATES) {
        let Ok(response) = serde_json::from_value::<TranslatedCellResponse>(value) else {
            continue;
        };
        if !seen.insert(response.cell_id) {
            continue;
        }
        if let Ok(mut parsed) = validated_cell(response, candidates) {
            regions.append(&mut parsed);
        }
    }
    regions.sort_by_key(|region| (region.bounds.top, region.bounds.left));
    Ok(TranslationDocument { regions })
}

pub(crate) fn parse_streamed_region(
    value: &str,
    candidates: &[DetectedTextRegion],
) -> Result<Vec<(u16, TranslationRegion)>> {
    let response: TranslatedCellResponse = serde_json::from_str(value)
        .context("streamed cell did not match the translation schema")?;
    Ok(validated_cell(response, candidates)?
        .into_iter()
        .map(|region| (region.id, region))
        .collect())
}

fn validated_cell(
    response: TranslatedCellResponse,
    candidates: &[DetectedTextRegion],
) -> Result<Vec<TranslationRegion>> {
    let proposal = super::cell_proposals::propose(candidates)
        .into_iter()
        .find(|proposal| proposal.member_ids_in_reading_order.first() == Some(&response.cell_id))
        .context("translation references an unknown local cell")?;
    let translation = clean_text(&response.translation, MAX_TEXT_CHARS)
        .context("translation is empty or too long")?;
    let selections = proposal
        .member_ids_in_reading_order
        .iter()
        .map(|id| {
            let candidate = candidates
                .iter()
                .find(|candidate| candidate.id == *id)
                .context("local cell member is missing")?;
            Ok(TranslationSelection {
                region_id: *id,
                candidate_id: format!("r{id}c0"),
                source_text: candidate.source_text.clone(),
                bounds: candidate.bounds,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let translated_segments =
        super::flow_layout::distribute_text(&translation, &selections, &proposal.member_joins);
    let boundaries = valid_boundaries(
        &response.split_after_members,
        &proposal.member_ids_in_reading_order,
    );
    let mut starts = vec![0];
    starts.extend(boundaries.into_iter().map(|index| index + 1));
    starts.push(selections.len());
    starts
        .windows(2)
        .map(|range| {
            build_region(
                &selections[range[0]..range[1]],
                &proposal.member_joins[range[0]..range[1].saturating_sub(1)],
                &translated_segments[range[0]..range[1]],
                candidates,
            )
        })
        .collect()
}

fn valid_boundaries(requested: &[u16], members: &[u16]) -> Vec<usize> {
    let mut boundaries = requested
        .iter()
        .filter_map(|id| members.iter().position(|member| member == id))
        .filter(|index| index + 1 < members.len())
        .collect::<Vec<_>>();
    boundaries.sort_unstable();
    boundaries.dedup();
    boundaries
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
                appearance: None,
            },
            DetectedTextRegion {
                id: 2,
                bounds: [34, 10, 54, 200].into(),
                source_text: "second line".into(),
                source_alternatives: vec!["second line".into()],
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
        assert!(prompt.contains(r#""cellId":1"#));
        assert!(prompt.contains(r#""memberId":2,"text":"second line""#));
        assert!(!prompt.contains("candidateIds"));
        assert!(!prompt.contains("memberJoins"));
    }

    #[test]
    fn one_translation_is_distributed_over_local_members() {
        let parsed = parse_response(
            r#"{"cells":[{"cellId":1,"translation":"một bản dịch hoàn chỉnh","splitAfterMembers":[]}]}"#,
            &candidates(),
        )
        .unwrap();
        assert_eq!(parsed.regions.len(), 1);
        assert_eq!(parsed.regions[0].member_ids, [1, 2]);
        assert_eq!(parsed.regions[0].translated_segments.len(), 2);
    }

    #[test]
    fn advisory_split_creates_local_subcells_without_retranslating() {
        let parsed = parse_response(
            r#"{"cells":[{"cellId":1,"translation":"một hai ba bốn","splitAfterMembers":[1]}]}"#,
            &candidates(),
        )
        .unwrap();
        assert_eq!(parsed.regions.len(), 2);
        assert!(
            parsed
                .regions
                .iter()
                .all(|region| region.member_ids.len() == 1)
        );
    }

    #[test]
    fn invalid_split_ids_are_ignored_without_rejection() {
        let parsed = parse_response(
            r#"{"cells":[{"cellId":1,"translation":"một hai ba bốn","splitAfterMembers":[999]}]}"#,
            &candidates(),
        )
        .unwrap();
        assert_eq!(parsed.regions.len(), 1);
    }
}
