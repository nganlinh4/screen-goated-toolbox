//! Bounded memory of model-delivered frames belonging to confirmed user turns.

use std::time::{Duration, Instant};

use super::super::{memory::MemoryFrame, session_retention, uia_task};

const MAX_FRAMES: usize = 6;
const MAX_ENCODED_FRAME_BYTES: usize = 4 * 1024 * 1024;
const MAX_SNAPSHOT_AGE: Duration = Duration::from_secs(2);
const TRANSCRIPT_FRAME_WINDOW: Duration = Duration::from_secs(10);

#[derive(Default)]
pub(super) struct ConversationMemory {
    frames: Vec<MemoryFrame>,
    provisional: Option<(MemoryFrame, Instant)>,
    turn_id: Option<u64>,
}

impl ConversationMemory {
    pub(super) fn begin_input(&mut self) {
        self.provisional = None;
    }

    pub(super) fn confirm_turn(&mut self, turn_id: u64) {
        self.turn_id = Some(turn_id);
        if !self.retention_allowed() {
            return;
        }
        if let Some((mut frame, captured_at)) = self.provisional.take()
            && captured_at.elapsed() <= TRANSCRIPT_FRAME_WINDOW
        {
            frame.turn_id = turn_id;
            self.remember(frame);
        }
    }

    pub(super) fn snapshot(
        &mut self,
        frame: &uia_task::SnapshotFrame,
        active: bool,
        user_input: bool,
    ) {
        if !self.retention_allowed() || frame.captured_at.elapsed() > MAX_SNAPSHOT_AGE {
            return;
        }
        self.delivered(
            &frame.b64,
            &frame.source,
            active,
            user_input,
            frame.captured_at,
        );
    }

    pub(super) fn action_frame(&mut self, b64: &str, source: &uia_task::FrameSource, turn_id: u64) {
        if self.turn_id == Some(turn_id) {
            self.delivered(b64, source, true, false, Instant::now());
        }
    }

    fn delivered(
        &mut self,
        b64: &str,
        source: &uia_task::FrameSource,
        active: bool,
        user_input: bool,
        captured_at: Instant,
    ) {
        if !self.retention_allowed()
            || b64.len() > MAX_ENCODED_FRAME_BYTES
            || b64.is_empty()
            || (!user_input && (!active || self.turn_id.is_none()))
        {
            return;
        }
        let frame = MemoryFrame {
            b64: b64.to_owned(),
            source: source.clone(),
            turn_id: self.turn_id.unwrap_or_default(),
        };
        if user_input {
            self.provisional = Some((frame, captured_at));
        } else if active && self.turn_id.is_some() {
            self.remember(frame);
        }
    }

    fn retention_allowed(&mut self) -> bool {
        if session_retention::current().allows() {
            return true;
        }
        self.frames.clear();
        self.provisional = None;
        false
    }

    fn remember(&mut self, frame: MemoryFrame) {
        self.frames.retain(|previous| {
            previous.turn_id != frame.turn_id || previous.source.surface != frame.source.surface
        });
        self.frames.push(frame);
        if self.frames.len() > MAX_FRAMES {
            self.frames.remove(0);
        }
    }

    pub(super) fn finish(self, history: Vec<String>) {
        super::super::memory::save(history, self.frames);
    }
}

#[cfg(test)]
mod tests;
