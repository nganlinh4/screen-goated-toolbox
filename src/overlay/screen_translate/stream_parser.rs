use std::collections::HashSet;

use super::contract::{DetectedTextRegion, TranslationRegion, parse_streamed_region};

pub(crate) struct TranslationStreamParser<'a> {
    candidates: &'a [DetectedTextRegion],
    buffer: String,
    scan: usize,
    array_started: bool,
    object_start: Option<usize>,
    depth: usize,
    in_string: bool,
    escaped: bool,
    emitted: HashSet<u16>,
    rejected: usize,
}

impl<'a> TranslationStreamParser<'a> {
    pub(crate) fn new(candidates: &'a [DetectedTextRegion]) -> Self {
        Self {
            candidates,
            buffer: String::new(),
            scan: 0,
            array_started: false,
            object_start: None,
            depth: 0,
            in_string: false,
            escaped: false,
            emitted: HashSet::new(),
            rejected: 0,
        }
    }

    pub(crate) fn push(&mut self, chunk: &str) -> Vec<(u16, TranslationRegion)> {
        if let Some(replacement) = chunk.strip_prefix(crate::api::WIPE_SIGNAL) {
            self.reset();
            self.buffer.push_str(replacement);
        } else {
            self.buffer.push_str(chunk);
        }
        if !self.array_started {
            if let Some(marker) = self.buffer.find("\"regions\"") {
                let Some(offset) = self.buffer[marker + 9..].find('[') else {
                    return Vec::new();
                };
                self.scan = marker + 9 + offset + 1;
            } else {
                let Some(start) = self
                    .buffer
                    .char_indices()
                    .find_map(|(index, character)| (!character.is_whitespace()).then_some(index))
                else {
                    return Vec::new();
                };
                let remaining = &self.buffer[start..];
                let array = remaining
                    .strip_prefix("```")
                    .map(|fenced| {
                        let fenced = fenced
                            .strip_prefix("json")
                            .or_else(|| fenced.strip_prefix("JSON"))
                            .unwrap_or(fenced);
                        self.buffer.len() - fenced.trim_start().len()
                    })
                    .or_else(|| remaining.starts_with('[').then_some(start));
                let Some(array_start) = array else {
                    return Vec::new();
                };
                let Some(offset) = self.buffer[array_start..].find('[') else {
                    return Vec::new();
                };
                self.scan = array_start + offset + 1;
            }
            self.array_started = true;
        }

        let mut completed = Vec::new();
        let bytes = self.buffer.as_bytes();
        while self.scan < bytes.len() {
            let byte = bytes[self.scan];
            if self.in_string {
                if self.escaped {
                    self.escaped = false;
                } else if byte == b'\\' {
                    self.escaped = true;
                } else if byte == b'"' {
                    self.in_string = false;
                }
            } else {
                match byte {
                    b'"' => self.in_string = true,
                    b'{' => {
                        if self.depth == 0 {
                            self.object_start = Some(self.scan);
                        }
                        self.depth += 1;
                    }
                    b'}' if self.depth > 0 => {
                        self.depth -= 1;
                        if self.depth == 0 {
                            let start = self.object_start.take().expect("object start is tracked");
                            let object = &self.buffer[start..=self.scan];
                            match parse_streamed_region(object, self.candidates) {
                                Ok((id, region)) if self.emitted.insert(id) => {
                                    completed.push((id, region));
                                }
                                Ok(_) => {}
                                Err(_) => self.rejected += 1,
                            }
                        }
                    }
                    _ => {}
                }
            }
            self.scan += 1;
        }
        completed
    }

    pub(crate) fn rejected_count(&self) -> usize {
        self.rejected
    }

    fn reset(&mut self) {
        self.buffer.clear();
        self.scan = 0;
        self.array_started = false;
        self.object_start = None;
        self.depth = 0;
        self.in_string = false;
        self.escaped = false;
        self.emitted.clear();
        self.rejected = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::screen_translate::contract::NormalizedBounds;

    fn candidates() -> Vec<DetectedTextRegion> {
        vec![DetectedTextRegion {
            id: 7,
            bounds: NormalizedBounds {
                left: 1,
                top: 2,
                right: 3,
                bottom: 4,
            },
            source_text: "clear text".to_string(),
            source_alternatives: vec!["clear text".to_string(), "alternate".to_string()],
        }]
    }

    #[test]
    fn emits_a_region_as_soon_as_its_object_closes() {
        let candidates = candidates();
        let mut parser = TranslationStreamParser::new(&candidates);
        assert!(parser.push("{\"reg").is_empty());
        let regions = parser
            .push("ions\":[{\"id\":7,\"sourceCandidateIndex\":1,\"translatedText\":\"done\"},");
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].1.source_text, "alternate");
        assert!(parser.push("]}").is_empty());
    }

    #[test]
    fn malformed_region_is_skipped_without_blocking_later_regions() {
        let candidates = candidates();
        let mut parser = TranslationStreamParser::new(&candidates);
        let regions = parser.push(
            "{\"regions\":[{\"id\":7,\"sourceCandidateIndex\":99,\"translatedText\":\"bad\"},{\"id\":7,\"sourceCandidateIndex\":0,\"translatedText\":\"good\"}]}",
        );

        assert_eq!(parser.rejected_count(), 1);
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].1.translated_text, "good");
    }

    #[test]
    fn emits_regions_from_an_equivalent_top_level_array() {
        let mut candidates = candidates();
        let mut second = candidates[0].clone();
        second.id = 8;
        candidates.push(second);
        let mut parser = TranslationStreamParser::new(&candidates);
        let emitted = parser.push(
            r#"[{"id":7,"sourceCandidateIndex":0,"translatedText":"first"},{"id":8,"sourceCandidateIndex":0,"translatedText":"second"}]"#,
        );
        assert_eq!(
            emitted
                .iter()
                .map(|(_, region)| region.translated_text.as_str())
                .collect::<Vec<_>>(),
            vec!["first", "second"]
        );
    }
}
