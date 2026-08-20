use std::collections::HashSet;

use super::contract::{DetectedTextRegion, TranslationRegion, parse_streamed_translation};

pub(crate) struct TranslationStreamParser<'a> {
    candidates: &'a [DetectedTextRegion],
    buffer: String,
    scan: usize,
    array_started: bool,
    element_start: Option<usize>,
    nested_depth: usize,
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
            element_start: None,
            nested_depth: 0,
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
        if !self.locate_array() {
            return Vec::new();
        }

        let mut completed = Vec::new();
        while self.scan < self.buffer.len() {
            let byte = self.buffer.as_bytes()[self.scan];
            if self.element_start.is_none() {
                match byte {
                    b' ' | b'\t' | b'\r' | b'\n' | b',' => {
                        self.scan += 1;
                        continue;
                    }
                    b']' => break,
                    b'"' => {
                        self.element_start = Some(self.scan);
                        self.in_string = true;
                    }
                    b'{' | b'[' => {
                        self.element_start = Some(self.scan);
                        self.nested_depth = 1;
                    }
                    _ => self.element_start = Some(self.scan),
                }
                self.scan += 1;
                continue;
            }

            if self.in_string {
                if self.escaped {
                    self.escaped = false;
                } else if byte == b'\\' {
                    self.escaped = true;
                } else if byte == b'"' {
                    self.in_string = false;
                    if self.nested_depth == 0 {
                        self.emit_element(self.scan + 1, &mut completed);
                    }
                }
                self.scan += 1;
                continue;
            }

            match byte {
                b'"' => self.in_string = true,
                b'{' | b'[' => self.nested_depth += 1,
                b'}' | b']' if self.nested_depth > 0 => {
                    self.nested_depth -= 1;
                    if self.nested_depth == 0 {
                        self.emit_element(self.scan + 1, &mut completed);
                    }
                }
                b',' | b']' if self.nested_depth == 0 => {
                    self.emit_element(self.scan, &mut completed);
                    if byte == b']' {
                        break;
                    }
                }
                _ => {}
            }
            self.scan += 1;
        }
        completed
    }

    pub(crate) fn rejected_count(&self) -> usize {
        self.rejected
    }

    fn locate_array(&mut self) -> bool {
        if self.array_started {
            return true;
        }
        let array_start = if let Some(marker) = self.buffer.find("\"translations\"") {
            let key_end = marker + "\"translations\"".len();
            self.buffer[key_end..]
                .find('[')
                .map(|offset| key_end + offset)
        } else {
            first_top_level_array(&self.buffer)
        };
        let Some(array_start) = array_start else {
            return false;
        };
        self.scan = array_start + 1;
        self.array_started = true;
        true
    }

    fn emit_element(&mut self, end: usize, completed: &mut Vec<(u16, TranslationRegion)>) {
        let Some(start) = self.element_start.take() else {
            return;
        };
        let value = self.buffer[start..end].trim();
        if value.is_empty() {
            return;
        }
        match parse_streamed_translation(value, self.candidates) {
            Ok((id, region)) if self.emitted.insert(id) => completed.push((id, region)),
            Err(_) => self.rejected += 1,
            Ok(_) => self.rejected += 1,
        }
        self.nested_depth = 0;
        self.in_string = false;
        self.escaped = false;
    }

    fn reset(&mut self) {
        self.buffer.clear();
        self.scan = 0;
        self.array_started = false;
        self.element_start = None;
        self.nested_depth = 0;
        self.in_string = false;
        self.escaped = false;
        self.emitted.clear();
        self.rejected = 0;
    }
}

fn first_top_level_array(buffer: &str) -> Option<usize> {
    let start = buffer
        .char_indices()
        .find_map(|(index, character)| (!character.is_whitespace()).then_some(index))?;
    let remaining = &buffer[start..];
    if remaining.starts_with('[') {
        return Some(start);
    }
    let fenced = remaining.strip_prefix("```")?;
    let fenced = fenced
        .strip_prefix("json")
        .or_else(|| fenced.strip_prefix("JSON"))
        .unwrap_or(fenced);
    let leading = fenced.len() - fenced.trim_start().len();
    let array = fenced.trim_start();
    array
        .starts_with('[')
        .then_some(buffer.len() - fenced.len() + leading)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::screen_translate::contract::NormalizedBounds;

    fn candidates() -> Vec<DetectedTextRegion> {
        (7..=8)
            .map(|id| DetectedTextRegion {
                id,
                bounds: NormalizedBounds {
                    left: 1,
                    top: id * 10,
                    right: 30,
                    bottom: id * 10 + 4,
                },
                source_text: format!("source {id}"),
                source_alternatives: vec![format!("source {id}")],
                recognition: Default::default(),
                appearance: None,
            })
            .collect()
    }

    #[test]
    fn emits_a_translation_as_soon_as_its_string_closes() {
        let candidates = candidates();
        let mut parser = TranslationStreamParser::new(&candidates);
        assert!(parser.push("{\"trans").is_empty());
        let regions = parser.push("lations\":[{\"slot\":0,\"translation\":\"first\"}");
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].0, 7);
        assert_eq!(regions[0].1.translated_segments, ["first"]);
    }

    #[test]
    fn malformed_slot_does_not_shift_a_later_translation() {
        let candidates = candidates();
        let mut parser = TranslationStreamParser::new(&candidates);
        let regions = parser.push(
            r#"{"translations":[{"slot":99,"translation":"bad"},{"slot":1,"translation":"second"}]}"#,
        );
        assert_eq!(parser.rejected_count(), 1);
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].0, 8);
        assert_eq!(regions[0].1.translated_segments, ["second"]);
    }

    #[test]
    fn emits_translations_from_an_equivalent_top_level_array() {
        let candidates = candidates();
        let mut parser = TranslationStreamParser::new(&candidates);
        let emitted =
            parser.push(r#"[{"slot":0,"translation":"first"},{"slot":1,"translation":"second"}]"#);
        assert_eq!(
            emitted
                .iter()
                .map(|(_, region)| region.translated_segments[0].as_str())
                .collect::<Vec<_>>(),
            vec!["first", "second"]
        );
    }
}
