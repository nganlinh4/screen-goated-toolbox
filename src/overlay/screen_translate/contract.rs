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
    member_ids_in_reading_order: Vec<u16>,
    candidate_ids: Vec<String>,
    member_joins: Vec<MemberJoin>,
    semantic_role: SemanticRole,
    translation_requirement: TranslationRequirement,
    translated_segments: Vec<String>,
}

pub(crate) fn response_schema(region_count: usize) -> serde_json::Value {
    super::schema::response_schema(region_count.max(1), MAX_TEXT_CHARS)
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
                "regionId": candidate.id,
                "box2d": <NormalizedBounds as Into<[u16; 4]>>::into(candidate.bounds),
                "sourceFlow": super::geometry::source_flow(candidate.bounds),
                "visualSignature": candidate.appearance,
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
         Translate the supplied atomic OCR regions into {target_language} and organize them into logical visual cells. A cell is one independently readable item: one heading, label, value, button, list item, dialogue turn, paragraph, or other standalone passage. Join only fragments that belong to that same item. Never join separate bullets, menu entries, controls, speakers, dialogue turns, or neighboring paragraphs merely because their boxes are close or visually similar. Adjacent columns of one vertical-script passage are one cell and must be ordered in that script's reading order with same_column joins; unrelated columns remain separate. Small pronunciation, reading, or annotation text must not become a second full-size copy of the passage it annotates.\n\
         For every cell, regionId must equal its first memberIdsInReadingOrder value. List every member exactly once across the complete response. Choose one candidateId and return one translatedSegments string for each member in that same order. Translate the complete logical cell coherently, then partition that translation across translatedSegments so each segment fits the meaning and reading position of its corresponding source member. The combined segments must preserve the cell's complete meaning; never move neighboring-cell content into them. memberJoins describes every adjacent pair and therefore has exactly one fewer item than memberIdsInReadingOrder: same_line for fragments on one baseline, wrapped_line for continuation lines, same_column for a single vertical-script passage, and same_block only for fragments of one semantic passage that are not simple line continuations. semanticRole describes the complete cell.\n\
         Use box2d, sourceFlow, visualSignature, text, punctuation, bullets, speaker prefixes, and reading flow together. Preserve separate items as separate results even when translating them in one request. Do not abbreviate, summarize, or add details to fit geometry. Preserve each member's local meaning in its corresponding translatedSegments entry; never exchange translations between member positions.\n\
         Preserve usernames, product names, brand names, personal names, and incomplete OCR fragments unless a conventional target-language form is clear from the supplied text. Never expand a short or damaged fragment into an invented word. Preserve list markers and speaker prefixes when they belong to the source cell.\n\
         translationRequirement is translation_required when the selected natural-language text differs from the requested target language, already_target only when it is already in the requested target language, and non_linguistic only when it contains no translatable language. The combined translatedSegments must express the complete selected source in {target_language}; selecting or copying OCR text is not translation. Copy only symbols, codes, numbers, and names that conventionally remain unchanged.\n\
         Preserve every supplied integer regionId exactly once as a member. Never omit, duplicate, or invent a member.\n\
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
    let mut seen_members = HashSet::new();
    let mut regions = Vec::with_capacity(values.len());
    for value in values {
        let Ok(response) = serde_json::from_value::<TranslatedRegionResponse>(value) else {
            continue;
        };
        let Ok(region) = validated_region(response, candidates) else {
            continue;
        };
        if region
            .member_ids
            .iter()
            .any(|member| seen_members.contains(member))
        {
            continue;
        }
        seen_members.extend(region.member_ids.iter().copied());
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
    if response.member_ids_in_reading_order.first().copied() != Some(response.region_id) {
        bail!("translation cell id must be its first member id");
    }
    let bounds = super::cell_validation::validate_cell_members(
        &response.member_ids_in_reading_order,
        &response.member_joins,
        candidates,
    )?;
    if response.candidate_ids.len() != response.member_ids_in_reading_order.len() {
        bail!("translation candidate count does not match its cell");
    }
    if response.translated_segments.len() != response.member_ids_in_reading_order.len() {
        bail!("translated segment count does not match its cell");
    }
    let mut selections = Vec::with_capacity(response.member_ids_in_reading_order.len());
    for (&region_id, selected_id) in response
        .member_ids_in_reading_order
        .iter()
        .zip(&response.candidate_ids)
    {
        let candidate = candidates
            .iter()
            .find(|candidate| candidate.id == region_id)
            .ok_or_else(|| anyhow::anyhow!("translation unit member is unknown"))?;
        let source_text = candidate
            .source_alternatives
            .iter()
            .enumerate()
            .find_map(|(index, text)| {
                (candidate_id(candidate.id, index) == *selected_id).then(|| text.clone())
            })
            .ok_or_else(|| anyhow::anyhow!("translation candidate id is unknown"))?;
        selections.push(TranslationSelection {
            region_id,
            candidate_id: selected_id.clone(),
            source_text,
            bounds: candidate.bounds,
        });
    }
    let source_text = selections
        .iter()
        .map(|selection| selection.source_text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let translated_segments = response
        .translated_segments
        .iter()
        .map(|segment| {
            clean_text(segment, MAX_TEXT_CHARS)
                .ok_or_else(|| anyhow::anyhow!("translated segment is empty or too long"))
        })
        .collect::<Result<Vec<_>>>()?;
    let translated_text = translated_segments.join(" ");
    if translated_text.chars().count() > MAX_TEXT_CHARS {
        bail!("combined translated text is too long");
    }
    let source_equivalent = text_is_source_equivalent(&source_text, &translated_text);
    match response.translation_requirement {
        TranslationRequirement::TranslationRequired if source_equivalent => {
            bail!("translation-required block remained source-equivalent");
        }
        TranslationRequirement::TranslationRequired
            if suspiciously_preserves_source(&source_text, &translated_text) =>
        {
            bail!("translation-required block retained nearly all source text");
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
    Ok(TranslationRegion {
        id: response.region_id,
        member_ids: response.member_ids_in_reading_order,
        member_joins: response.member_joins,
        selections,
        semantic_role: response.semantic_role,
        source_text,
        translated_segments,
        bounds,
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

fn suspiciously_preserves_source(source: &str, translated: &str) -> bool {
    let source = comparable_text(source).chars().collect::<Vec<_>>();
    let translated = comparable_text(translated).chars().collect::<Vec<_>>();
    const GRAM: usize = 3;
    if source.len() < 12 || translated.len() < GRAM {
        return false;
    }
    let translated_grams = translated
        .windows(GRAM)
        .map(|window| window.to_vec())
        .collect::<HashSet<_>>();
    let source_grams = source.windows(GRAM);
    let total = source_grams.len();
    let retained = source_grams
        .filter(|window| translated_grams.contains(*window))
        .count();
    retained.saturating_mul(100) >= total.saturating_mul(92)
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
                bounds: [300, 22, 370, 200].into(),
                source_text: "second".to_string(),
                source_alternatives: vec!["second".to_string()],
                appearance: None,
            },
        ]
    }

    fn translated(id: u16, candidate: usize, text: &str) -> String {
        format!(
            r#"{{"regionId":{id},"memberIdsInReadingOrder":[{id}],"candidateIds":["r{id}c{candidate}"],"memberJoins":[],"semanticRole":"standalone","translationRequirement":"translation_required","translatedSegments":["{text}"]}}"#
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
        assert!(instructions.contains("organize them into logical visual cells"));
        assert!(instructions.contains("Never join separate bullets"));
        assert!(instructions.contains("Do not abbreviate, summarize"));
        assert!(instructions.contains("memberJoins"));
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
        assert_eq!(result.regions[0].translated_segments, ["one"]);
        assert_eq!(result.regions[0].bounds, candidates()[0].bounds);
        assert_eq!(result.regions[0].source_text, "first");
        assert_eq!(result.regions[0].member_ids, vec![1]);
    }

    #[test]
    fn parser_rejects_a_segment_count_that_does_not_match_cell_members() {
        let mut candidates = candidates();
        candidates[1].bounds = [168, 22, 238, 200].into();
        let parsed = parse_response(
            r#"{"regions":[{"regionId":1,"memberIdsInReadingOrder":[1,2],"candidateIds":["r1c0","r2c0"],"memberJoins":["wrapped_line"],"semanticRole":"paragraph","translationRequirement":"translation_required","translatedSegments":["only one"]}]}"#,
            &candidates,
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
        assert_eq!(parsed.regions[0].translated_segments, ["uno"]);
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
    fn parser_rejects_a_formatting_correction_claimed_as_translation() {
        let mut candidates = candidates();
        candidates[0].source_alternatives[0] =
            "installation configration directory reference".to_string();
        let response = translated(1, 0, "installation configuration directory reference");
        let parsed =
            parse_response(&format!(r#"{{"regions":[{response}]}}"#), &candidates).unwrap();
        assert!(parsed.regions.is_empty());
    }

    #[test]
    fn parser_accepts_unchanged_text_only_when_the_model_marks_it_nonrequired() {
        let unchanged = r#"{"regionId":1,"memberIdsInReadingOrder":[1],"candidateIds":["r1c0"],"memberJoins":[],"semanticRole":"standalone","translationRequirement":"already_target","translatedSegments":["first"]}"#;
        let parsed =
            parse_response(&format!(r#"{{"regions":[{unchanged}]}}"#), &candidates()).unwrap();
        assert_eq!(parsed.regions.len(), 1);
        assert!(text_is_source_equivalent(
            &parsed.regions[0].source_text,
            &parsed.regions[0].translated_segments.join(" ")
        ));
    }

    #[test]
    fn adjacent_fragments_can_form_one_model_owned_logical_cell() {
        let mut candidates = candidates();
        candidates[1].bounds = [168, 22, 238, 200].into();
        let parsed = parse_response(
            r#"{"regions":[{"regionId":1,"memberIdsInReadingOrder":[1,2],"candidateIds":["r1c0","r2c0"],"memberJoins":["wrapped_line"],"semanticRole":"paragraph","translationRequirement":"translation_required","translatedSegments":["one coherent","passage"]}]}"#,
            &candidates,
        )
        .unwrap();
        assert_eq!(parsed.regions.len(), 1);
        assert_eq!(parsed.regions[0].member_ids, [1, 2]);
        assert_eq!(parsed.regions[0].source_text, "first second");
        assert_eq!(
            parsed.regions[0].translated_segments,
            ["one coherent", "passage"]
        );
        assert_eq!(parsed.regions[0].semantic_role, SemanticRole::Paragraph);
        assert_eq!(parsed.regions[0].member_joins, [MemberJoin::WrappedLine]);
    }

    #[test]
    fn neighboring_list_items_remain_independent_when_the_model_separates_them() {
        let response = format!(
            r#"{{"regions":[{},{}]}}"#,
            translated(1, 0, "uno"),
            translated(2, 0, "dos")
        );
        let parsed = parse_response(&response, &candidates()).unwrap();
        assert_eq!(parsed.regions.len(), 2);
        assert!(
            parsed
                .regions
                .iter()
                .all(|region| region.member_ids.len() == 1)
        );
    }

    #[test]
    fn duplicate_members_cannot_be_claimed_by_multiple_cells() {
        let candidates = candidates();
        let first = translated(1, 0, "uno");
        let duplicate = r#"{"regionId":1,"memberIdsInReadingOrder":[1,2],"candidateIds":["r1c0","r2c0"],"memberJoins":["same_block"],"semanticRole":"paragraph","translationRequirement":"translation_required","translatedSegments":["duplicate","claim"]}"#;
        let parsed = parse_response(
            &format!(r#"{{"regions":[{first},{duplicate}]}}"#),
            &candidates,
        )
        .unwrap();
        assert_eq!(parsed.regions.len(), 1);
        assert_eq!(parsed.regions[0].member_ids, [1]);
    }
}
