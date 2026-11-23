//! Audio activity requests observations; response boundaries own transcript epochs.

use super::reader::Reader;
use super::speech_events::UserAudioTracker;
use super::telemetry;

#[derive(Clone, Copy)]
pub(super) enum LocalInputPhase {
    Initial,
    Collecting,
    Complete(u64),
}

pub(super) struct LocalAudioUpdate {
    pub(super) begin_epoch: bool,
    pub(super) request_frame: bool,
}

fn phase(state: &Reader) -> LocalInputPhase {
    if state.input_transcript.is_open()
        && !state.active
        && !state.awaiting
        && state.pending.id.is_none()
        && !state.terminal_response.is_open()
    {
        LocalInputPhase::Complete(state.input_transcript.epoch())
    } else if state.input_transcript.has_fresh_epoch() {
        LocalInputPhase::Collecting
    } else if !state.input_transcript.is_open() {
        LocalInputPhase::Initial
    } else {
        LocalInputPhase::Collecting
    }
}

pub(super) fn observe(
    tracker: &mut UserAudioTracker,
    state: &mut Reader,
    voiced: bool,
    level: f32,
    samples: usize,
    playing: bool,
) {
    let update = tracker.update_for_local_epoch(voiced, level, samples, playing, phase(state));
    if update.begin_epoch {
        state.input_transcript.begin_epoch();
    }
    if update.request_frame {
        begin_frame_input(state);
    }
}

fn begin_frame_input(state: &mut Reader) {
    state.memory.begin_input();
    state.frame_delivery.begin_input();
    state.source_frame = None;
}

pub(super) fn begin_epoch(state: &mut Reader) {
    state.input_transcript.begin_epoch();
    begin_frame_input(state);
}

pub(super) fn confirm_transcript(state: &mut Reader, starts_turn: bool) {
    if starts_turn {
        if state.frame_delivery.confirm_input() {
            state.memory.begin_input();
            state.source_frame = None;
        }
    } else {
        state.frame_delivery.confirm_continuation();
        state.memory.confirm_turn(telemetry::current_turn());
    }
}

#[cfg(test)]
mod tests;
