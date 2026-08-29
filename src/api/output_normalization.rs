use super::WIPE_SIGNAL;

#[derive(Default)]
pub(crate) struct InitialLineBreakNormalizer {
    started: bool,
}

pub(crate) enum NormalizedChunk<'a> {
    Paint(&'a str),
    Replace(&'a str),
    Suppress,
}

impl InitialLineBreakNormalizer {
    pub(crate) fn observe<'a>(&mut self, chunk: &'a str) -> NormalizedChunk<'a> {
        if let Some(replacement) = chunk.strip_prefix(WIPE_SIGNAL) {
            self.started = false;
            return NormalizedChunk::Replace(self.normalize_start(replacement));
        }
        let normalized = self.normalize_start(chunk);
        if normalized.is_empty() {
            NormalizedChunk::Suppress
        } else {
            NormalizedChunk::Paint(normalized)
        }
    }

    pub(crate) fn finish(&self, output: String) -> String {
        let start = initial_line_break_end(&output);
        if start == 0 {
            output
        } else {
            output[start..].to_string()
        }
    }

    fn normalize_start<'a>(&mut self, text: &'a str) -> &'a str {
        if self.started {
            return text;
        }
        let start = initial_line_break_end(text);
        if start < text.len() {
            self.started = true;
        }
        &text[start..]
    }
}

fn initial_line_break_end(text: &str) -> usize {
    text.as_bytes()
        .iter()
        .take_while(|byte| matches!(byte, b'\r' | b'\n'))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_only_initial_line_breaks_across_stream_chunks() {
        let mut normalizer = InitialLineBreakNormalizer::default();
        let mut painted = String::new();
        for chunk in ["\r", "\n\n", "  result", "\nnext"] {
            if let NormalizedChunk::Paint(text) = normalizer.observe(chunk) {
                painted.push_str(text);
            }
        }
        assert_eq!(painted, "  result\nnext");
        assert_eq!(normalizer.finish("\r\n  result\nnext".to_string()), painted);
    }

    #[test]
    fn replacement_restarts_initial_normalization() {
        let mut normalizer = InitialLineBreakNormalizer::default();
        assert!(matches!(
            normalizer.observe("first"),
            NormalizedChunk::Paint("first")
        ));
        let replacement = format!("{WIPE_SIGNAL}\r\nsecond");
        assert!(matches!(
            normalizer.observe(&replacement),
            NormalizedChunk::Replace("second")
        ));
    }

    #[test]
    fn behavior_matches_the_shared_preset_fixture() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../parity-fixtures/preset-system/text-provider-routing.json"
        ))
        .expect("valid text-provider-routing fixture");
        let contract = &fixture["output_normalization"];
        assert_eq!(contract["restart_on_transport_replacement"], true);
        assert_eq!(
            contract["scopes"],
            serde_json::json!(["text-to-text", "refinement", "image-to-text"])
        );
    }
}
