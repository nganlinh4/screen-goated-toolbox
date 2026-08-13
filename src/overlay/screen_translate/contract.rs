//! Structured contract for translating detector-owned text regions.

use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

pub(crate) const MAX_CANDIDATES: usize = 240;
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
    regions: Vec<TranslatedRegionResponse>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TranslatedRegionResponse {
    id: u16,
    source_text: String,
    translated_text: String,
}

pub(crate) fn response_schema(region_count: usize) -> serde_json::Value {
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
                        "sourceText": { "type": "string", "minLength": 1, "maxLength": MAX_TEXT_CHARS },
                        "translatedText": { "type": "string", "minLength": 1, "maxLength": MAX_TEXT_CHARS }
                    },
                    "required": ["id", "sourceText", "translatedText"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["regions"],
        "additionalProperties": false
    })
}

pub(crate) fn prompt(target_language: &str, candidates: &[DetectedTextRegion]) -> Result<String> {
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
    Ok(format!(
        "Translate the supplied OCR text regions into {target_language}. Treat every region independently because one image can contain multiple source languages. Each region contains bounded recognition candidates for the same image box. Select the most linguistically coherent candidate and return it as sourceText. A region is readable when any one candidate is coherent, even if its other candidates are gibberish.\n\
         Preserve every supplied integer id exactly once. Never omit a region. If a region is noise, an icon, a counter, a proper noun, an identifier, or already in the target language, return the selected sourceText unchanged.\n\
         Translate all readable language, including ordinary interface labels, menu items, buttons, headings, navigation terms, and mixed-language text; preserve product or brand names, identifiers, numbers, punctuation, and tone.\n\
         sourceText must exactly equal one supplied sourceCandidate. translatedText contains only its translation.\n\
         Return one JSON object matching the supplied schema, without Markdown or commentary.\n\
         OCR regions:\n{source_regions}"
    ))
}

pub(crate) fn parse_response(
    response: &str,
    candidates: &[DetectedTextRegion],
) -> Result<TranslationDocument> {
    let json = unwrap_json(response);
    let response: TranslationResponse =
        serde_json::from_str(json).context("response did not match the translation schema")?;
    if response.regions.len() > MAX_TRANSLATED_REGIONS {
        bail!("response contains too many translated regions");
    }
    let candidate_data = candidates
        .iter()
        .map(|candidate| {
            (
                candidate.id,
                (candidate.bounds, candidate.source_alternatives.as_slice()),
            )
        })
        .collect::<HashMap<_, _>>();
    if candidate_data.len() != candidates.len() {
        bail!("detector returned duplicate candidate ids");
    }
    let mut seen = HashSet::new();
    let mut regions = Vec::with_capacity(response.regions.len());
    for region in response.regions {
        let (bounds, alternatives) = candidate_data.get(&region.id).copied().ok_or_else(|| {
            anyhow::anyhow!("response references unknown detector region {}", region.id)
        })?;
        if !seen.insert(region.id) {
            bail!("response repeats detector region {}", region.id);
        }
        let source_text = region.source_text.trim();
        if !alternatives
            .iter()
            .any(|candidate| candidate == source_text)
        {
            bail!(
                "response selected an unknown recognition candidate for region {}",
                region.id
            );
        }
        let translated_text = region.translated_text.trim();
        if source_text.is_empty() || translated_text.is_empty() {
            bail!(
                "response contains an empty text for detector region {}",
                region.id
            );
        }
        regions.push(TranslationRegion {
            id: region.id,
            source_text: clean_text(source_text, MAX_TEXT_CHARS)
                .ok_or_else(|| anyhow::anyhow!("a source text region is empty or too long"))?,
            translated_text: clean_text(translated_text, MAX_TEXT_CHARS)
                .ok_or_else(|| anyhow::anyhow!("a translated text region is empty or too long"))?,
            bounds,
            background_color: None,
            text_color: None,
        });
    }
    if seen.len() != candidate_data.len() {
        bail!("response omitted one or more detector regions");
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
    let source_text = clean_text(&response.source_text, MAX_TEXT_CHARS)
        .ok_or_else(|| anyhow::anyhow!("streamed source text is empty or too long"))?;
    if !candidate
        .source_alternatives
        .iter()
        .any(|known| known == &source_text)
    {
        bail!("streamed region selected an unknown recognition candidate");
    }
    let translated_text = clean_text(&response.translated_text, MAX_TEXT_CHARS)
        .ok_or_else(|| anyhow::anyhow!("streamed translated text is empty or too long"))?;
    Ok((
        response.id,
        TranslationRegion {
            id: response.id,
            source_text,
            translated_text,
            bounds: candidate.bounds,
            background_color: None,
            text_color: None,
        },
    ))
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
        let instructions = prompt("Target Language", &candidates()).unwrap();
        assert!(instructions.contains(r#"{"id":1,"sourceCandidates":["first","f1rst"]}"#));
        assert!(!instructions.contains("box_2d"));
    }

    #[test]
    fn parser_maps_ids_to_detector_bounds_and_orders_them() {
        let result = parse_response(
            r##"{"regions":[{"id":2,"sourceText":"second","translatedText":"two"},{"id":1,"sourceText":"first","translatedText":"one"}]}"##,
            &candidates(),
        )
        .unwrap();
        assert_eq!(result.regions[0].translated_text, "one");
        assert_eq!(result.regions[0].bounds, candidates()[0].bounds);
        assert_eq!(result.regions[0].source_text, "first");
    }

    #[test]
    fn parser_rejects_unknown_duplicate_and_model_owned_geometry() {
        let cases = [
            r#"{"regions":[{"id":9,"sourceText":"first","translatedText":"b"}]}"#,
            r#"{"regions":[{"id":1,"sourceText":"first","translatedText":"b"},{"id":1,"sourceText":"first","translatedText":"b"}]}"#,
            r#"{"regions":[{"id":1,"sourceText":"first","translatedText":"b","box_2d":[1,2,3,4]}]}"#,
            r#"{"regions":[{"id":1,"sourceText":"invented","translatedText":"b"},{"id":2,"sourceText":"second","translatedText":"c"}]}"#,
        ];
        for value in cases {
            assert!(parse_response(value, &candidates()).is_err());
        }
    }

    #[test]
    fn parser_rejects_missing_or_empty_detector_regions() {
        let cases = [
            r#"{"regions":[{"id":2,"sourceText":"second","translatedText":"two"}]}"#,
            r#"{"regions":[{"id":1,"sourceText":"first","translatedText":""},{"id":2,"sourceText":"second","translatedText":"two"}]}"#,
        ];
        for value in cases {
            assert!(parse_response(value, &candidates()).is_err());
        }
    }
}
