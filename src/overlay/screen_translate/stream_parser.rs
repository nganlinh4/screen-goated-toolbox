use std::collections::HashSet;

use anyhow::Result;

use super::contract::{DetectedTextRegion, TranslationRegion, parse_streamed_region};

pub(super) struct TranslationStreamParser<'a> {
    candidates: &'a [DetectedTextRegion],
    buffer: String,
    scan: usize,
    array_started: bool,
    object_start: Option<usize>,
    depth: usize,
    in_string: bool,
    escaped: bool,
    emitted: HashSet<u16>,
}

impl<'a> TranslationStreamParser<'a> {
    pub(super) fn new(candidates: &'a [DetectedTextRegion]) -> Self {
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
        }
    }

    pub(super) fn push(&mut self, chunk: &str) -> Result<Vec<(u16, TranslationRegion)>> {
        if let Some(replacement) = chunk.strip_prefix(crate::api::WIPE_SIGNAL) {
            self.reset();
            self.buffer.push_str(replacement);
        } else {
            self.buffer.push_str(chunk);
        }
        if !self.array_started {
            let Some(marker) = self.buffer.find("\"regions\"") else {
                return Ok(Vec::new());
            };
            let Some(offset) = self.buffer[marker + 9..].find('[') else {
                return Ok(Vec::new());
            };
            self.scan = marker + 9 + offset + 1;
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
                            let (id, region) = parse_streamed_region(object, self.candidates)?;
                            if self.emitted.insert(id) {
                                completed.push((id, region));
                            }
                        }
                    }
                    _ => {}
                }
            }
            self.scan += 1;
        }
        Ok(completed)
    }

    pub(super) fn emitted(&self, id: u16) -> bool {
        self.emitted.contains(&id)
    }

    pub(super) fn emitted_any(&self) -> bool {
        !self.emitted.is_empty()
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
        assert!(parser.push("{\"reg").unwrap().is_empty());
        let regions = parser
            .push("ions\":[{\"id\":7,\"sourceText\":\"alternate\",\"translatedText\":\"done\"},")
            .unwrap();
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].1.source_text, "alternate");
        assert!(parser.push("]}").unwrap().is_empty());
    }
}
