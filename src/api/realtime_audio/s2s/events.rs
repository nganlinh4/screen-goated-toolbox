use std::time::Instant;

use super::SegmentOutcome;

#[derive(Clone)]
pub(super) enum S2sEvent {
    Queued {
        id: u64,
        audio_ms: usize,
        queued_at: Instant,
    },
    InputText {
        id: u64,
        text: String,
    },
    OutputText {
        id: u64,
        text: String,
    },
    Audio {
        id: u64,
        bytes: Vec<u8>,
    },
    Done {
        id: u64,
    },
    Error {
        id: u64,
        message: String,
    },
    LiveText {
        source_full: String,
        source_committed_len: usize,
        target_committed: String,
        target_draft: String,
    },
    Interrupt,
}

pub(super) enum S2sRaceEvent {
    Event {
        attempt: usize,
        event: S2sEvent,
    },
    Finished {
        attempt: usize,
        outcome: SegmentOutcome,
    },
    Error {
        attempt: usize,
        message: String,
    },
}

fn s2s_event_counts(events: &[S2sEvent]) -> (usize, usize, usize) {
    let mut input_text = 0usize;
    let mut output_text = 0usize;
    let mut audio = 0usize;
    for event in events {
        match event {
            S2sEvent::InputText { .. } => input_text += 1,
            S2sEvent::OutputText { .. } => output_text += 1,
            S2sEvent::Audio { .. } => audio += 1,
            _ => {}
        }
    }
    (input_text, output_text, audio)
}

pub(super) fn s2s_attempt_counts(events: &[Vec<S2sEvent>]) -> (usize, usize, usize) {
    events
        .iter()
        .map(|buffered| s2s_event_counts(buffered))
        .fold((0usize, 0usize, 0usize), |acc, counts| {
            (acc.0 + counts.0, acc.1 + counts.1, acc.2 + counts.2)
        })
}

pub(super) fn format_s2s_attempt_counts(events: &[Vec<S2sEvent>]) -> String {
    events
        .iter()
        .enumerate()
        .map(|(attempt, buffered)| {
            let (input_text, output_text, audio) = s2s_event_counts(buffered);
            format!("{attempt}:in={input_text},out={output_text},audio={audio}")
        })
        .collect::<Vec<_>>()
        .join(";")
}
