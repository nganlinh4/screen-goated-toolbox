use super::{Divergence, StepPair, StepTranscript, TranscriptResult};

pub(super) fn maybe_normalize(value: &str, ignore_whitespace: bool) -> String {
    if !ignore_whitespace {
        return value.to_string();
    }
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(super) fn first_divergence(
    chunk_id: usize,
    audio_samples: usize,
    dense: &StepTranscript,
    compressed: &StepTranscript,
    ignore_whitespace: bool,
) -> Option<Divergence> {
    for (field, left, right) in [
        (
            "language",
            dense.language.as_str(),
            compressed.language.as_str(),
        ),
        (
            "fixed_text",
            dense.fixed_text.as_str(),
            compressed.fixed_text.as_str(),
        ),
        (
            "draft_text",
            dense.draft_text.as_str(),
            compressed.draft_text.as_str(),
        ),
        ("text", dense.text.as_str(), compressed.text.as_str()),
    ] {
        let left = maybe_normalize(left, ignore_whitespace);
        let right = maybe_normalize(right, ignore_whitespace);
        if left != right {
            return Some(Divergence {
                chunk_id,
                audio_samples,
                field,
                dense: left,
                compressed: right,
            });
        }
    }
    None
}

pub(super) fn worst_streaming_divergence(
    streaming_steps: &[StepPair],
    ignore_whitespace: bool,
) -> Option<(usize, usize, usize)> {
    let mut worst = None;
    for step in streaming_steps {
        let mut field_count = 0usize;
        for (left, right) in [
            (
                step.dense.language.as_str(),
                step.compressed.language.as_str(),
            ),
            (
                step.dense.fixed_text.as_str(),
                step.compressed.fixed_text.as_str(),
            ),
            (
                step.dense.draft_text.as_str(),
                step.compressed.draft_text.as_str(),
            ),
            (step.dense.text.as_str(), step.compressed.text.as_str()),
        ] {
            if maybe_normalize(left, ignore_whitespace) != maybe_normalize(right, ignore_whitespace)
            {
                field_count += 1;
            }
        }
        if field_count == 0 {
            continue;
        }
        let replace = match worst {
            Some((_, _, best)) => field_count > best,
            None => true,
        };
        if replace {
            worst = Some((step.chunk_id, step.audio_samples, field_count));
        }
    }
    worst
}

pub(super) fn offline_divergence(
    dense: &TranscriptResult,
    compressed: &TranscriptResult,
    audio_samples: usize,
    ignore_whitespace: bool,
) -> Option<Divergence> {
    for (field, left, right) in [
        (
            "language",
            dense.language.as_str(),
            compressed.language.as_str(),
        ),
        ("text", dense.text.as_str(), compressed.text.as_str()),
        (
            "raw_output",
            dense.raw_output.as_str(),
            compressed.raw_output.as_str(),
        ),
    ] {
        let left = maybe_normalize(left, ignore_whitespace);
        let right = maybe_normalize(right, ignore_whitespace);
        if left != right {
            return Some(Divergence {
                chunk_id: 0,
                audio_samples,
                field,
                dense: left,
                compressed: right,
            });
        }
    }
    None
}
