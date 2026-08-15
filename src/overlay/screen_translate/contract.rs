//! Structured contract for translating detector-owned text regions.

use std::collections::HashSet;

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
    pub member_ids: Vec<u16>,
    pub selections: Vec<TranslationSelection>,
    pub semantic_role: SemanticRole,
    pub source_text: String,
    pub translated_text: String,
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
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum TranslationRequirement {
    TranslationRequired,
    AlreadyTarget,
    NonLinguistic,
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
    region_id: u16,
    candidate_id: String,
    translation_requirement: TranslationRequirement,
    translated_text: String,
}

pub(crate) fn response_schema(region_count: usize) -> serde_json::Value {
    super::schema::response_schema(region_count, MAX_TEXT_CHARS)
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
            let source_candidates = candidate
                .source_alternatives
                .iter()
                .enumerate()
                .map(|(index, text)| {
                    serde_json::json!({
                        "candidateId": candidate_id(candidate.id, index),
                        "characters": text.chars().count(),
                        "text": text
                    })
                })
                .collect::<Vec<_>>();
            serde_json::json!({
                "id": candidate.id,
                "box2d": <NormalizedBounds as Into<[u16; 4]>>::into(candidate.bounds),
                "visualStyle": candidate.appearance,
                "sourceCandidates": source_candidates
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
         Translate the supplied OCR text regions into {target_language}. Interpret the complete ordered region list as shared context, including wrapped lines and mixed source languages, but return exactly one result for each supplied region so every result remains bound to its source geometry. Never merge, split, renumber, reorder, omit, or move region content.\n\
         For each result, copy regionId exactly, choose exactly one supplied candidateId from that same region, and translate only that selected region's text. Neighboring regions provide context only; never move a translation into an earlier or later result.\n\
         Translate the complete selected text faithfully. Do not abbreviate, summarize, or drop meaning to fit the source box, and never add explanations or details absent from that region.\n\
         translationRequirement is translation_required when the selected natural-language text differs from the requested target language, already_target only when it is already in the requested target language, and non_linguistic only when it contains no translatable language. translatedText must express the complete selected source in {target_language}; selecting or copying OCR text is not translation. Copy only symbols, codes, numbers, and names that conventionally remain unchanged.\n\
         Preserve every supplied integer regionId exactly once. Never omit a region.\n\
         Translate all readable language, including ordinary interface labels, menu items, buttons, headings, navigation terms, proper nouns that have a conventional target-language form, and every source language in mixed-language text. If an entire block is already in {target_language} or contains no natural language, it may remain unchanged. Preserve meaning, punctuation, and tone.\n\
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
    if candidates
        .iter()
        .map(|candidate| candidate.id)
        .collect::<HashSet<_>>()
        .len()
        != candidates.len()
    {
        bail!("detector returned duplicate candidate ids");
    }
    let mut seen = HashSet::new();
    let mut regions = Vec::with_capacity(values.len());
    for value in values {
        let Ok(response) = serde_json::from_value::<TranslatedRegionResponse>(value) else {
            continue;
        };
        if seen.contains(&response.region_id) {
            continue;
        }
        let Ok(region) = validated_region(response, candidates) else {
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
    let region = validated_region(response, candidates)?;
    Ok((region.id, region))
}

fn validated_region(
    response: TranslatedRegionResponse,
    candidates: &[DetectedTextRegion],
) -> Result<TranslationRegion> {
    let candidate = candidates
        .iter()
        .find(|candidate| candidate.id == response.region_id)
        .ok_or_else(|| anyhow::anyhow!("translation references an unknown region id"))?;
    let source_text = candidate
        .source_alternatives
        .iter()
        .enumerate()
        .find_map(|(index, text)| {
            (candidate_id(candidate.id, index) == response.candidate_id).then(|| text.clone())
        })
        .ok_or_else(|| anyhow::anyhow!("translation candidate id is unknown"))?;
    let translated_text = clean_text(&response.translated_text, MAX_TEXT_CHARS)
        .ok_or_else(|| anyhow::anyhow!("translated text is empty or too long"))?;
    let source_equivalent = text_is_source_equivalent(&source_text, &translated_text);
    match response.translation_requirement {
        TranslationRequirement::TranslationRequired if source_equivalent => {
            bail!("translation-required block remained source-equivalent");
        }
        TranslationRequirement::AlreadyTarget | TranslationRequirement::NonLinguistic
            if !source_equivalent =>
        {
            bail!("non-translated block changed its selected source text");
        }
        _ => {}
    }
    if !source_text.chars().any(char::is_alphabetic)
        && translated_text.chars().any(char::is_alphabetic)
    {
        bail!("translation text is inconsistent with a non-language source selection");
    }
    let selection = TranslationSelection {
        region_id: candidate.id,
        candidate_id: response.candidate_id,
        source_text: source_text.clone(),
        bounds: candidate.bounds,
    };
    Ok(TranslationRegion {
        id: candidate.id,
        member_ids: vec![candidate.id],
        selections: vec![selection],
        semantic_role: SemanticRole::Standalone,
        source_text,
        translated_text,
        bounds: candidate.bounds,
        background_color: None,
        text_color: None,
    })
}

fn candidate_id(region_id: u16, index: usize) -> String {
    format!("r{region_id}c{index}")
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
                bounds: [100, 20, 170, 180].into(),
                source_text: "first".to_string(),
                source_alternatives: vec!["first".to_string(), "f1rst".to_string()],
                appearance: None,
            },
            DetectedTextRegion {
                id: 2,
                bounds: [168, 22, 238, 200].into(),
                source_text: "second".to_string(),
                source_alternatives: vec!["second".to_string()],
                appearance: None,
            },
        ]
    }

    fn translated(id: u16, candidate: usize, text: &str) -> String {
        format!(
            r#"{{"regionId":{id},"candidateId":"r{id}c{candidate}","translationRequirement":"translation_required","translatedText":"{text}"}}"#
        )
    }

    #[test]
    fn prompt_supplies_fixed_geometry_and_candidate_contract() {
        let instructions = prompt_with_instruction(
            "Target Language",
            &crate::config::types::ScreenTranslateSettings::default_prompt(),
            &candidates(),
        )
        .unwrap();
        assert!(instructions.contains(r#""candidateId":"r1c0","characters":5,"text":"first""#));
        assert!(instructions.contains(r#""box2d":[100,20,170,180]"#));
        assert!(instructions.contains("return exactly one result for each supplied region"));
        assert!(instructions.contains("Never merge, split, renumber, reorder, omit, or move"));
        assert!(instructions.contains("Do not abbreviate, summarize, or drop meaning"));
        assert!(!instructions.contains("allowedLinks"));
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
        let second = translated(2, 0, "two");
        let first = translated(1, 0, "one");
        let result = parse_response(
            &format!(r#"{{"regions":[{second},{first}]}}"#),
            &candidates(),
        )
        .unwrap();
        assert_eq!(result.regions[0].translated_text, "one");
        assert_eq!(result.regions[0].bounds, candidates()[0].bounds);
        assert_eq!(result.regions[0].source_text, "first");
        assert_eq!(result.regions[0].member_ids, vec![1]);
    }

    #[test]
    fn parser_rejects_model_owned_grouping_fields() {
        let parsed = parse_response(
            r#"{"regions":[{"memberIds":[1,2],"translatedText":"bad"}]}"#,
            &candidates(),
        )
        .unwrap();
        assert!(parsed.regions.is_empty());
    }

    #[test]
    fn parser_rejects_semantically_impossible_candidate_binding_per_block() {
        let mut candidates = candidates();
        candidates[0].source_alternatives[0] = "1.+".to_string();
        let invalid = translated(1, 0, "A long translated sentence");
        let valid = translated(2, 0, "two");
        let parsed = parse_response(
            &format!(r#"{{"regions":[{invalid},{valid}]}}"#),
            &candidates,
        )
        .unwrap();
        assert_eq!(parsed.regions.len(), 1);
        assert_eq!(parsed.regions[0].id, 2);
    }

    #[test]
    fn parser_accepts_the_equivalent_top_level_region_array() {
        let candidates = candidates();
        let parsed =
            parse_response(&format!("[{}]", translated(1, 0, "uno")), &candidates).unwrap();
        assert_eq!(parsed.regions.len(), 1);
        assert_eq!(parsed.regions[0].translated_text, "uno");
    }

    #[test]
    fn parser_rejects_a_malformed_top_level_contract() {
        assert!(parse_response(r#"{"regions":"not-an-array"}"#, &candidates()).is_err());
    }

    #[test]
    fn parser_does_not_let_one_changed_block_hide_an_untranslated_block() {
        let unchanged_required = translated(1, 0, "first");
        let translated = translated(2, 0, "dos");
        let parsed = parse_response(
            &format!(r#"{{"regions":[{unchanged_required},{translated}]}}"#),
            &candidates(),
        )
        .unwrap();
        assert_eq!(parsed.regions.len(), 1);
        assert_eq!(parsed.regions[0].id, 2);
    }

    #[test]
    fn parser_accepts_unchanged_text_only_when_the_model_marks_it_nonrequired() {
        let unchanged = r#"{"regionId":1,"candidateId":"r1c0","translationRequirement":"already_target","translatedText":"first"}"#;
        let parsed =
            parse_response(&format!(r#"{{"regions":[{unchanged}]}}"#), &candidates()).unwrap();
        assert_eq!(parsed.regions.len(), 1);
        assert!(text_is_source_equivalent(
            &parsed.regions[0].source_text,
            &parsed.regions[0].translated_text
        ));
    }
}
