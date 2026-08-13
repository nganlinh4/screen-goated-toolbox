//! Structured contract for translating detector-owned text regions.

use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

pub(crate) const MAX_CANDIDATES: usize = 240;
pub(crate) const MAX_SOURCE_CANDIDATES: usize = 3;
const MAX_TRANSLATED_REGIONS: usize = MAX_CANDIDATES;
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
    pub source_text: String,
    pub translated_text: String,
    #[serde(rename = "box_2d")]
    pub bounds: NormalizedBounds,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_color: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DetectedTextRegion {
    pub id: u16,
    pub bounds: NormalizedBounds,
    pub source_text: String,
    pub source_alternatives: Vec<String>,
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
    regions: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum TranslationResponseEnvelope {
    Object(TranslationResponse),
    Array(Vec<serde_json::Value>),
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TranslatedRegionResponse {
    id: u16,
    source_candidate_index: usize,
    translated_text: String,
}

pub(crate) fn response_schema(region_count: usize) -> serde_json::Value {
    let max_candidate_index = MAX_SOURCE_CANDIDATES - 1;
    serde_json::json!({
        "type": "object",
        "properties": {
            "regions": {
                "type": "array",
                "minItems": region_count,
                "maxItems": region_count,
                "items": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "integer" },
                        "sourceCandidateIndex": {
                            "type": "integer",
                            "minimum": 0,
                            "maximum": max_candidate_index
                        },
                        "translatedText": { "type": "string", "minLength": 1, "maxLength": MAX_TEXT_CHARS }
                    },
                    "required": ["id", "sourceCandidateIndex", "translatedText"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["regions"],
        "additionalProperties": false
    })
}

pub(crate) fn prompt_with_instruction(
    target_language: &str,
    translation_instruction: &str,
    candidates: &[DetectedTextRegion],
) -> Result<String> {
    if candidates.is_empty() || candidates.len() > MAX_CANDIDATES {
        bail!("detector candidate count is outside the translation contract");
    }
    let source_regions = candidates
        .iter()
        .map(|candidate| {
            serde_json::json!({
                "id": candidate.id,
                "sourceCandidates": candidate.source_alternatives
            })
        })
        .collect::<Vec<_>>();
    let source_regions = serde_json::to_string(&source_regions)?;
    let translation_instruction = translation_instruction
        .replace("{target_language}", target_language)
        .trim()
        .to_string();
    Ok(format!(
        "Translation preference:\n{translation_instruction}\n\n\
         Required screen-translation contract:\n\
         Translate the supplied OCR text regions into {target_language}. Treat every region independently because one image can contain multiple source languages. Each region contains an ordered array of bounded recognition candidates for the same image box. Select the most linguistically coherent candidate and return its zero-based array position as sourceCandidateIndex. A region is readable when any one candidate is coherent, even if its other candidates are gibberish.\n\
         Preserve every supplied integer id exactly once. Never omit a region. If a region is noise, an icon, a counter, a proper noun, an identifier, or already in the target language, return the selected candidate unchanged as translatedText.\n\
         Translate all readable language, including ordinary interface labels, menu items, buttons, headings, navigation terms, and mixed-language text; preserve product or brand names, identifiers, numbers, punctuation, and tone.\n\
         translatedText contains only the translation of the selected source candidate.\n\
         Return one JSON object matching the supplied schema, without Markdown or commentary.\n\
         OCR regions:\n{source_regions}"
    ))
}

pub(crate) fn parse_response(
    response: &str,
    candidates: &[DetectedTextRegion],
) -> Result<TranslationDocument> {
    let json = unwrap_json(response);
    let response: TranslationResponseEnvelope =
        serde_json::from_str(json).context("response did not match the translation schema")?;
    let values = match response {
        TranslationResponseEnvelope::Object(response) => response.regions,
        TranslationResponseEnvelope::Array(regions) => regions,
    };
    if values.len() > MAX_TRANSLATED_REGIONS {
        bail!("response contains too many translated regions");
    }
    let candidate_data = candidates
        .iter()
        .map(|candidate| (candidate.id, candidate))
        .collect::<HashMap<_, _>>();
    if candidate_data.len() != candidates.len() {
        bail!("detector returned duplicate candidate ids");
    }
    let mut seen = HashSet::new();
    let mut regions = Vec::with_capacity(values.len());
    for value in values {
        let Ok(region) = serde_json::from_value::<TranslatedRegionResponse>(value) else {
            continue;
        };
        if seen.contains(&region.id) {
            continue;
        }
        let Some(candidate) = candidate_data.get(&region.id).copied() else {
            continue;
        };
        let Ok(region) = validated_region(region, candidate) else {
            continue;
        };
        seen.insert(region.id);
        regions.push(region);
    }
    regions.sort_by_key(|region| (region.bounds.top, region.bounds.left));
    Ok(TranslationDocument { regions })
}

pub(crate) fn parse_streamed_region(
    value: &str,
    candidates: &[DetectedTextRegion],
) -> Result<(u16, TranslationRegion)> {
    let response: TranslatedRegionResponse = serde_json::from_str(value)
        .context("streamed region did not match the translation schema")?;
    let candidate = candidates
        .iter()
        .find(|candidate| candidate.id == response.id)
        .ok_or_else(|| anyhow::anyhow!("streamed region references an unknown id"))?;
    let id = response.id;
    Ok((id, validated_region(response, candidate)?))
}

fn validated_region(
    response: TranslatedRegionResponse,
    candidate: &DetectedTextRegion,
) -> Result<TranslationRegion> {
    let source_text = candidate
        .source_alternatives
        .get(response.source_candidate_index)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("source candidate index is outside its detector region"))?;
    let translated_text = clean_text(&response.translated_text, MAX_TEXT_CHARS)
        .ok_or_else(|| anyhow::anyhow!("translated text is empty or too long"))?;
    Ok(TranslationRegion {
        id: response.id,
        source_text,
        translated_text,
        bounds: candidate.bounds,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn candidates() -> Vec<DetectedTextRegion> {
        vec![
            DetectedTextRegion {
                id: 1,
                bounds: [100, 20, 170, 180].into(),
                source_text: "first".to_string(),
                source_alternatives: vec!["first".to_string(), "f1rst".to_string()],
            },
            DetectedTextRegion {
                id: 2,
                bounds: [500, 40, 580, 200].into(),
                source_text: "second".to_string(),
                source_alternatives: vec!["second".to_string()],
            },
        ]
    }

    #[test]
    fn prompt_supplies_detector_owned_text_without_geometry() {
        let instructions = prompt_with_instruction(
            "Target Language",
            &crate::config::types::ScreenTranslateSettings::default_prompt(),
            &candidates(),
        )
        .unwrap();
        assert!(instructions.contains(r#"{"id":1,"sourceCandidates":["first","f1rst"]}"#));
        assert!(instructions.contains("sourceCandidateIndex"));
        assert!(
            !response_schema(candidates().len())
                .to_string()
                .contains("sourceText")
        );
        assert!(!instructions.contains("box_2d"));
    }

    #[test]
    fn prompt_expands_the_configured_target_language_placeholder() {
        let instructions = prompt_with_instruction(
            "Korean",
            "Use concise wording in {target_language}.",
            &candidates(),
        )
        .unwrap();
        assert!(instructions.contains("Use concise wording in Korean."));
        assert!(!instructions.contains("{target_language}"));
    }

    #[test]
    fn parser_maps_ids_to_detector_bounds_and_orders_them() {
        let result = parse_response(
            r##"{"regions":[{"id":2,"sourceCandidateIndex":0,"translatedText":"two"},{"id":1,"sourceCandidateIndex":0,"translatedText":"one"}]}"##,
            &candidates(),
        )
        .unwrap();
        assert_eq!(result.regions[0].translated_text, "one");
        assert_eq!(result.regions[0].bounds, candidates()[0].bounds);
        assert_eq!(result.regions[0].source_text, "first");
    }

    #[test]
    fn parser_keeps_valid_regions_when_neighbors_are_invalid() {
        let result = parse_response(
            r#"{"regions":[
                {"id":9,"sourceCandidateIndex":0,"translatedText":"unknown"},
                {"id":1,"sourceCandidateIndex":1,"translatedText":"valid"},
                {"id":1,"sourceCandidateIndex":0,"translatedText":"duplicate"},
                {"id":2,"sourceCandidateIndex":9,"translatedText":"bad index"},
                {"id":2,"sourceCandidateIndex":0,"translatedText":"extra","box_2d":[1,2,3,4]}
            ]}"#,
            &candidates(),
        )
        .unwrap();

        assert_eq!(result.regions.len(), 1);
        assert_eq!(result.regions[0].id, 1);
        assert_eq!(result.regions[0].source_text, "f1rst");
        assert_eq!(result.regions[0].translated_text, "valid");
    }

    #[test]
    fn parser_returns_valid_partial_output_for_missing_or_empty_neighbors() {
        let missing = parse_response(
            r#"{"regions":[{"id":2,"sourceCandidateIndex":0,"translatedText":"two"}]}"#,
            &candidates(),
        )
        .unwrap();
        assert_eq!(missing.regions.len(), 1);
        assert_eq!(missing.regions[0].id, 2);

        let empty_neighbor = parse_response(
            r#"{"regions":[{"id":1,"sourceCandidateIndex":0,"translatedText":""},{"id":2,"sourceCandidateIndex":0,"translatedText":"two"}]}"#,
            &candidates(),
        )
        .unwrap();
        assert_eq!(empty_neighbor.regions.len(), 1);
        assert_eq!(empty_neighbor.regions[0].id, 2);
    }

    #[test]
    fn parser_accepts_the_equivalent_top_level_region_array() {
        let candidates = candidates();
        let parsed = parse_response(
            r#"[{"id":1,"sourceCandidateIndex":0,"translatedText":"uno"}]"#,
            &candidates,
        )
        .unwrap();
        assert_eq!(parsed.regions.len(), 1);
        assert_eq!(parsed.regions[0].translated_text, "uno");
    }

    #[test]
    fn parser_rejects_a_malformed_top_level_contract() {
        assert!(parse_response(r#"{"regions":"not-an-array"}"#, &candidates()).is_err());
    }
}
