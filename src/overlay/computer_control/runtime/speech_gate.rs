//! Filters malformed spoken output from the Live model. Tool plans and internal
//! reasoning must not reach the orb, history, or speaker.

use super::super::playback::AudioSink;

const MAX_BUFFERED_SAMPLES: usize = 24_000 * 30;

#[derive(Default)]
pub(super) struct SpeechGate {
    checked: bool,
    blocked: bool,
    defer_until_boundary: bool,
    pending_audio: Vec<i16>,
    transcript_prefix: String,
}

pub(super) enum TranscriptDecision<'a> {
    Allow(&'a str),
    Block,
}

impl SpeechGate {
    pub(super) fn push_audio(&mut self, pcm: &[i16], sink: Option<&AudioSink>) {
        if self.blocked || pcm.is_empty() {
            return;
        }
        if self.checked {
            if let Some(sink) = sink {
                sink.push(pcm);
            }
            return;
        }
        let keep = MAX_BUFFERED_SAMPLES.saturating_sub(self.pending_audio.len());
        self.pending_audio
            .extend_from_slice(&pcm[..pcm.len().min(keep)]);
    }

    pub(super) fn transcript<'a>(
        &mut self,
        text: &'a str,
        sink: Option<&AudioSink>,
    ) -> TranscriptDecision<'a> {
        if self.blocked {
            return TranscriptDecision::Block;
        }
        self.transcript_prefix.push_str(text);
        if looks_internal(&self.transcript_prefix) {
            self.blocked = true;
            self.pending_audio.clear();
            if let Some(sink) = sink {
                sink.clear();
            }
            return TranscriptDecision::Block;
        }
        if !self.defer_until_boundary && !self.checked && safe_to_release(&self.transcript_prefix) {
            self.checked = true;
            if let Some(sink) = sink {
                sink.push(&self.pending_audio);
            }
            self.pending_audio.clear();
        }
        TranscriptDecision::Allow(text)
    }

    pub(super) fn reset(&mut self) {
        self.checked = false;
        self.blocked = false;
        self.pending_audio.clear();
        self.transcript_prefix.clear();
        self.defer_until_boundary = false;
    }

    pub(super) fn defer_until_boundary(&mut self, defer: bool) {
        self.defer_until_boundary = defer;
    }

    pub(super) fn is_deferred(&self) -> bool {
        self.defer_until_boundary
    }

    pub(super) fn finish_before_tool(&mut self, sink: Option<&AudioSink>) -> bool {
        if !self.defer_until_boundary {
            self.finish_turn(sink);
            return false;
        }
        self.pending_audio.clear();
        self.transcript_prefix.clear();
        self.checked = false;
        self.blocked = false;
        true
    }

    pub(super) fn finish_turn(&mut self, sink: Option<&AudioSink>) {
        if !self.blocked
            && !self.checked
            && !looks_internal(&self.transcript_prefix)
            && let Some(sink) = sink
        {
            sink.push(&self.pending_audio);
        }
        self.reset();
    }
}

fn safe_to_release(s: &str) -> bool {
    s.chars().filter(|c| !c.is_whitespace()).count() >= 8
}

// Only block UNAMBIGUOUS machine output (JSON / code-fence / tool-plan structure). We must
// NOT block natural-language narration like "I need to open the menu" or "I should check that"
// - those are legitimate things the agent says to the user, and muting them would regress the
// base (non-MCP) voice. Plan-narration prose is a prompt-level concern, not something to gag here.
fn looks_internal(s: &str) -> bool {
    let trimmed = s.trim_start();
    let lower = trimmed.to_ascii_lowercase();
    trimmed.starts_with("```")
        || trimmed.starts_with('{')
        || lower.starts_with("json\n")
        || lower.contains("\"reasoning\"")
        || lower.contains("\"command\"")
        || lower.contains("\"args\"")
        || lower.contains("tool_call")
        || lower.contains("function call")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_json_plans() {
        assert!(looks_internal(
            "```json\n{\"reasoning\":\"x\",\"command\":\"look\"}"
        ));
        assert!(looks_internal(
            "{ \"reasoning\": \"x\", \"command\": \"look\" }"
        ));
    }

    #[test]
    fn allows_normal_speech() {
        assert!(!looks_internal("I found the setting and it is ready."));
        assert!(safe_to_release("Okay, checking."));
    }

    #[test]
    fn deferred_speech_is_discarded_before_a_tool() {
        let mut gate = SpeechGate::default();
        gate.defer_until_boundary(true);
        assert!(matches!(
            gate.transcript("A draft answer before evidence.", None),
            TranscriptDecision::Allow(_)
        ));
        assert!(gate.finish_before_tool(None));
        assert!(gate.defer_until_boundary);
        assert!(gate.pending_audio.is_empty());
    }
}
