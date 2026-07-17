//! Mutable reader-side state and per-job cancellation identity.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use super::super::{turn_policy, uia_task};

const MAX_ACTIVE_GOAL_TURNS: usize = 6;
const MAX_ACTIVE_GOAL_CHARS: usize = 12_000;

/// Bounded user-authored request lineage for an unfinished action.
///
/// Code records turn order only. The independent model decides whether a later
/// turn continues, revises, cancels, or replaces an earlier requirement.
#[derive(Default)]
pub(super) struct ActiveGoal {
    turns: VecDeque<String>,
}

impl ActiveGoal {
    pub(super) fn start_turn(&mut self, text: &str, inherit: bool) {
        if !inherit {
            self.turns.clear();
        }
        self.turns.push_back(bounded_goal_text(text));
        self.enforce_bounds();
    }

    pub(super) fn update_current(&mut self, text: &str) {
        if let Some(current) = self.turns.back_mut() {
            *current = bounded_goal_text(text);
        } else {
            self.turns.push_back(bounded_goal_text(text));
        }
        self.enforce_bounds();
    }

    pub(super) fn render(&self) -> String {
        if self.turns.len() == 1 {
            return self.turns.front().cloned().unwrap_or_default();
        }
        let mut rendered = String::from(
            "AUTHORITATIVE USER REQUEST HISTORY (oldest to newest; later turns may revise, cancel, or replace earlier requirements):",
        );
        for (index, turn) in self.turns.iter().enumerate() {
            rendered.push_str(&format!("\nTURN {}: {}", index + 1, turn));
        }
        rendered
    }

    pub(super) fn is_continuation(&self) -> bool {
        self.turns.len() > 1
    }

    fn enforce_bounds(&mut self) {
        while self.turns.len() > MAX_ACTIVE_GOAL_TURNS {
            self.turns.pop_front();
        }
        while self.turns.len() > 1 && goal_chars(&self.turns) > MAX_ACTIVE_GOAL_CHARS {
            self.turns.pop_front();
        }
    }
}

fn bounded_goal_text(text: &str) -> String {
    text.trim().chars().take(MAX_ACTIVE_GOAL_CHARS).collect()
}

fn goal_chars(turns: &VecDeque<String>) -> usize {
    turns.iter().map(|turn| turn.chars().count()).sum()
}

/// One in-flight call, its structural name, and its cancellation token.
#[derive(Default)]
pub(super) struct Pending {
    pub(super) id: Option<String>,
    pub(super) tool: Option<String>,
    pub(super) turn_id: Option<u64>,
    pub(super) cancelled: bool,
    pub(super) cancel: Option<Arc<AtomicBool>>,
}

impl Pending {
    /// Cancel only this job. The token is never reused by a later action.
    pub(super) fn request_cancel(&mut self) -> bool {
        let had_job = self.id.is_some();
        if had_job {
            self.cancelled = true;
            if let Some(cancel) = &self.cancel {
                cancel.store(true, Ordering::SeqCst);
            }
        }
        had_job
    }

    pub(super) fn matches_result(&self, id: &str, cancel: &Arc<AtomicBool>) -> bool {
        self.id.as_deref() == Some(id)
            && self
                .cancel
                .as_ref()
                .is_some_and(|pending_cancel| Arc::ptr_eq(pending_cancel, cancel))
    }
}

/// Mutable reader-side session state threaded through `handle_event`.
#[derive(Default)]
pub(super) struct Reader {
    pub(super) pending: Pending,
    /// The model's spoken output since the last tool call - its "intent" context.
    pub(super) reasoning: String,
    /// The model's SILENT thinking (includeThoughts) since the last tool call - the
    /// preferred intent source: captured even when the model says nothing aloud.
    pub(super) thinking: String,
    /// Committed user goal assembled from bounded unresolved user-turn history.
    /// Model output may explain its rationale, but cannot replace this value.
    pub(super) last_cmd: String,
    pub(super) active_goal: ActiveGoal,
    /// A rejected action turn keeps its request lineage for the next user turn.
    pub(super) carry_unfinished_goal: bool,
    /// The current turn's transcript retained for history and model context.
    pub(super) last_user_text: String,
    pub(super) input_transcript: super::speech_events::InputTranscriptAssembler,
    /// When the current user turn started, for compact turn summaries.
    pub(super) turn_started_at: Option<Instant>,
    pub(super) turn_tools: Vec<String>,
    /// Content-free structural results that were actually delivered this turn.
    pub(super) turn_outcomes: super::ToolOutcomeLedger,
    /// A mutating call was interrupted without a delivered receipt. Further
    /// mutations stay blocked until its worker settles and fresh observational
    /// capability state reaches the replacement turn or transport.
    pub(super) reconciliation_required: bool,
    pub(super) turn_research_count: u32,
    pub(super) model_generation_index: u32,
    pub(super) usage_event_index: u32,
    pub(super) turn_tool_response_bytes: usize,
    pub(super) turn_element_chars: usize,
    pub(super) turn_stall_count: u32,
    pub(super) turn_summary_emitted: bool,
    /// Authorization/lifecycle mode for the current user turn.
    pub(super) turn_mode: turn_policy::TurnMode,
    /// Policy-owned responses waiting for the socket loop to send.
    pub(super) immediate_tool_responses: VecDeque<super::reader_policy::ImmediateToolResponse>,
    /// Exact latest frame successfully sent to the Live model.
    pub(super) source_frame: Option<uia_task::FrameSource>,
    pub(super) assistant_utterance_id: Option<u64>,
    /// Utterance that owns the accumulated caption. It survives barge-in long
    /// enough for transcript telemetry to correlate with the audio it replaced.
    pub(super) reply_utterance_id: Option<u64>,
    /// Monotonic playback ownership epoch. Interruption advances it so the
    /// polling tracker cannot later complete the retired utterance.
    pub(super) playback_epoch: u64,
    /// Model audio held until the current generation is structurally known to
    /// be progress, conversation, or a verified state-changing completion.
    pub(super) generation_audio: super::speech_events::GenerationAudioBuffer,
    /// Substantive assistant output observed in the current model generation.
    /// Reset at each user/tool boundary so a progress utterance cannot stand in
    /// for the final response of a later completion generation.
    pub(super) generation_output_seen: bool,
    /// Substantive output that arrived in the generation which issued the
    /// current tool call. A verified `done` may release that held final response
    /// and close immediately instead of waiting for a second model generation.
    pub(super) pending_tool_output_seen: bool,
    pub(super) connection_generation: u32,
    pub(super) reconnect_total: u32,
    /// Once a generation closes, latch the session against every model-originated
    /// event until a real new user turn. A boundary records quiescence but does
    /// not unlock the model, because a tool response can start a later generation.
    pub(super) terminal_drain: bool,
    /// True only when the closed generation satisfied the user's request.
    pub(super) terminal_accepted: bool,
    pub(super) terminal_boundary_seen: bool,
    /// Boundary status for the in-flight tool generation; result delivery can
    /// race the socket, so final-response ownership must retain it explicitly.
    pub(super) pending_tool_boundary_seen: bool,
    pub(super) terminal_dropped_events: u32,
    /// Effectful model output rejected after the accepted terminal boundary.
    /// Kept separate from harmless protocol chatter so scripted acceptance can
    /// prove that no late speech or tool call escaped the closed generation.
    pub(super) terminal_effectful_dropped_events: u32,
    /// A successful terminal tool owns one final model generation. Its speech
    /// may finish, but no tool from that generation may execute.
    pub(super) terminal_response: super::terminal_drain::FinalResponseState,
    /// True only after the owning final generation produced output and reached
    /// its structural boundary. Acceptance alone is not delivery.
    pub(super) terminal_final_response_delivered: bool,
    /// The wire-level generation boundary is distinct from `turnComplete` for
    /// realtime output; the latter may wait for expected playback duration.
    pub(super) terminal_generation_complete: bool,
    /// A pre-output generation completion belongs to the tool-call generation;
    /// consume its paired turn boundary without closing the post-tool response.
    pub(super) terminal_prior_turn_boundary_pending: bool,
    pub(super) terminal_activity_at: Option<Instant>,
    /// Cumulative output cursor used to prove playback is still advancing while
    /// waiting for the later turn boundary.
    pub(super) terminal_playback_cursor: Option<u64>,
    /// A new user turn can arrive before the completed generation's boundary.
    /// Consume that stale boundary only while the new generation has no output.
    pub(super) ignore_stale_boundary: bool,
    /// True while a spoken request is being worked on. Idle frames are pushed only
    /// while active, so after `done` the agent waits for the user instead of
    /// treating each new frame as a cue to keep acting.
    pub(super) active: bool,
    /// Latest queued turn-resource cleanup not yet acknowledged by the executor.
    /// This delays scripted idleness only; it never blocks live user input.
    pub(super) turn_cleanup_pending: Option<u64>,
    /// Rolling conversation history (alternating "User:"/"Assistant:" lines). The
    /// preview model rejects sessionResumption, so on a dropped connection we
    /// re-seed a fresh session with this recap - the agent keeps its memory.
    pub(super) history: Vec<String>,
    /// The assistant's spoken reply since the last user turn, flushed into
    /// `history` when the user speaks again (or on reconnect).
    pub(super) reply: String,
    /// True while the current model generation still owes a boundary.
    pub(super) awaiting: bool,
    /// True until that generation produces its first substantive output. Silence
    /// recovery is legal only while both this and `awaiting` are true.
    pub(super) recovery_owed: bool,
    /// Set once we've nudged the model during the CURRENT silent spell, so we poke
    /// it only once before escalating to a reconnect. Cleared on any server event.
    pub(super) nudged: bool,
    /// The server sent a `goAway` (session is hitting its duration limit). The run
    /// loop reconnects PROACTIVELY at the next gap so we migrate cleanly with our
    /// recap, instead of being force-closed mid-stream.
    pub(super) go_away: bool,
    /// When the model started OWING us a response (user spoke, or we answered a tool
    /// call). Used to log model THINK-time and to catch turns that end mid-task with
    /// no action ("narrated but didn't act").
    pub(super) think_start: Option<Instant>,
    // Rolling diagnostics, logged as a [PROFILE] line every 12 actions.
    pub(super) tool_calls: u32,
    pub(super) think_total_ms: u128,
    pub(super) spoke_count: u32,
    pub(super) stall_count: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_goal_updates_the_current_transcript_without_duplication() {
        let mut goal = ActiveGoal::default();
        goal.start_turn("first fragment", false);
        goal.update_current("first fragment completed");
        assert_eq!(goal.render(), "first fragment completed");
    }

    #[test]
    fn active_goal_retains_only_bounded_recent_unfinished_turns() {
        let mut goal = ActiveGoal::default();
        for index in 0..(MAX_ACTIVE_GOAL_TURNS + 2) {
            goal.start_turn(&format!("request {index}"), index != 0);
        }
        let rendered = goal.render();
        assert!(!rendered.contains("request 0"));
        assert!(!rendered.contains("request 1"));
        assert!(rendered.contains("request 2"));
        assert!(rendered.contains(&format!("request {}", MAX_ACTIVE_GOAL_TURNS + 1)));
        assert!(goal_chars(&goal.turns) <= MAX_ACTIVE_GOAL_CHARS);
    }

    #[test]
    fn active_goal_exposes_only_structural_continuation_state() {
        let mut goal = ActiveGoal::default();
        goal.start_turn("first", false);
        assert!(!goal.is_continuation());
        goal.start_turn("correction", true);
        assert!(goal.is_continuation());
        goal.start_turn("unrelated", false);
        assert!(!goal.is_continuation());
    }
}
