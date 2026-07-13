//! Pure, clock-injected Gemini Live lifecycle reducer.
//!
//! Transport adapters execute the returned effects. Feature-owned replay, VAD,
//! hedging, playback, UI, and Computer Control safety policy stay outside it.

mod types;

pub use types::*;

pub struct LiveSessionLifecycle {
    policy: LiveLifecyclePolicy,
    backoff: LiveBackoffPolicy,
    state: LiveLifecycleState,
}

impl LiveSessionLifecycle {
    pub fn new(policy: LiveLifecyclePolicy, backoff: LiveBackoffPolicy) -> Self {
        Self {
            policy: policy.normalized_for_shared_domain(),
            backoff,
            state: LiveLifecycleState::default(),
        }
    }

    pub fn state(&self) -> &LiveLifecycleState {
        &self.state
    }

    pub fn reduce(&mut self, at_ms: u64, event: LiveLifecycleEvent) -> Vec<LiveLifecycleEffect> {
        if self.state.phase.is_terminal() {
            return Vec::new();
        }
        let at_ms = types::cross_platform_clamp(at_ms);
        match event {
            LiveLifecycleEvent::Start => self.start(at_ms),
            LiveLifecycleEvent::SocketOpened { generation } => {
                self.socket_opened(at_ms, generation)
            }
            LiveLifecycleEvent::Frame(frame) => self.frame(at_ms, frame),
            LiveLifecycleEvent::TransportFailure {
                generation,
                retryable,
            } => self.transport_failure(at_ms, generation, retryable),
            LiveLifecycleEvent::InputSent { chunks } => {
                if self.state.phase == LiveSessionPhase::Active {
                    self.state.input_chunks_since_server_activity = types::cross_platform_add(
                        self.state.input_chunks_since_server_activity,
                        chunks,
                    );
                }
                Vec::new()
            }
            LiveLifecycleEvent::InputActivity => {
                if self.state.phase == LiveSessionPhase::Active {
                    self.state.last_input_activity_ms = Some(at_ms);
                }
                Vec::new()
            }
            LiveLifecycleEvent::WorkState {
                pending_work_count,
                buffered_input_count,
                user_speaking,
            } => {
                self.state.pending_work_count = types::cross_platform_clamp(pending_work_count);
                self.state.buffered_input_count = types::cross_platform_clamp(buffered_input_count);
                self.state.user_speaking = user_speaking;
                Vec::new()
            }
            LiveLifecycleEvent::Tick => self.tick(at_ms),
            LiveLifecycleEvent::Cancel => self.cancel(),
        }
    }

    fn start(&mut self, at_ms: u64) -> Vec<LiveLifecycleEffect> {
        match self.state.phase {
            LiveSessionPhase::Idle => {
                self.state.generation = 1;
                self.state.phase = LiveSessionPhase::Connecting;
                self.state.connection_started_at_ms = Some(at_ms);
                vec![LiveLifecycleEffect::OpenSocket {
                    generation: self.state.generation,
                }]
            }
            LiveSessionPhase::BackingOff
                if self
                    .state
                    .reconnect_deadline_ms
                    .is_some_and(|deadline| at_ms >= deadline) =>
            {
                self.state.phase = LiveSessionPhase::Connecting;
                self.state.connection_started_at_ms = Some(at_ms);
                self.state.reconnect_deadline_ms = None;
                vec![LiveLifecycleEffect::OpenSocket {
                    generation: self.state.generation,
                }]
            }
            _ => Vec::new(),
        }
    }

    fn socket_opened(&mut self, at_ms: u64, generation: u64) -> Vec<LiveLifecycleEffect> {
        if !self.accepts(generation) || self.state.phase != LiveSessionPhase::Connecting {
            return Vec::new();
        }
        self.state.phase = LiveSessionPhase::AwaitingSetup;
        self.state.socket_open = true;
        self.state.connected_at_ms = Some(at_ms);
        self.state.last_server_activity_ms = Some(at_ms);
        self.state.last_input_activity_ms = Some(at_ms);
        self.state.setup_deadline_ms = Some(types::cross_platform_add(
            at_ms,
            self.policy.setup_timeout_ms,
        ));
        vec![LiveLifecycleEffect::SendSetup { generation }]
    }

    fn frame(&mut self, at_ms: u64, frame: LiveLifecycleFrame) -> Vec<LiveLifecycleEffect> {
        if !self.accepts(frame.generation) {
            return Vec::new();
        }
        if let Some(error) = frame.error {
            return if error.retryable {
                self.schedule_reconnect(at_ms, LiveReconnectReason::ServerError)
            } else {
                self.fail(error.kind)
            };
        }

        let mut effects = Vec::new();
        if frame.setup_complete && self.state.phase == LiveSessionPhase::AwaitingSetup {
            self.state.phase = LiveSessionPhase::Active;
            self.state.setup_deadline_ms = None;
            self.state.last_server_activity_ms = Some(at_ms);
            self.state.input_chunks_since_server_activity = 0;
            if self.policy.kind == LiveSessionKind::FiniteRequest {
                self.state.first_response_deadline_ms = self
                    .policy
                    .first_response_timeout_ms
                    .map(|timeout| types::cross_platform_add(at_ms, timeout));
                self.state.hard_response_deadline_ms = self
                    .policy
                    .hard_response_timeout_ms
                    .map(|timeout| types::cross_platform_add(at_ms, timeout));
            }
        }
        if self.state.phase != LiveSessionPhase::Active {
            return effects;
        }

        let meaningful = frame.content_count > 0
            || frame.turn_complete
            || frame.generation_complete
            || frame.interrupted
            || !frame.tool_call_ids.is_empty()
            || !frame.tool_cancellation_ids.is_empty();
        if meaningful {
            self.state.last_server_activity_ms = Some(at_ms);
            self.state.input_chunks_since_server_activity = 0;
            self.state.reconnect_attempt = 0;
        }
        if frame.content_count > 0 {
            self.state.has_output = true;
            self.state.first_response_deadline_ms = None;
            self.state.content_idle_deadline_ms = self
                .policy
                .content_idle_ms
                .map(|idle| types::cross_platform_add(at_ms, idle));
            effects.push(LiveLifecycleEffect::DeliverContent {
                count: frame.content_count,
            });
        }
        if !frame.tool_call_ids.is_empty() {
            let ids = retain_new_ids(&mut self.state.pending_tool_ids, frame.tool_call_ids);
            if !ids.is_empty() {
                effects.push(LiveLifecycleEffect::DispatchTools { ids });
            }
        }
        if frame.interrupted {
            effects.extend([
                LiveLifecycleEffect::StopPlayback,
                LiveLifecycleEffect::DiscardQueuedOutput,
                LiveLifecycleEffect::FinalizeInterruptedGeneration,
            ]);
        }
        if !frame.tool_cancellation_ids.is_empty() {
            let ids = remove_pending_ids(
                &mut self.state.pending_tool_ids,
                &frame.tool_cancellation_ids,
            );
            if !ids.is_empty() {
                effects.push(LiveLifecycleEffect::CancelTools { ids });
            }
        }

        if frame.turn_complete && self.policy.complete_on_turn {
            effects.extend(self.complete(LiveCompletionReason::TurnComplete));
        }
        if !self.state.phase.is_terminal()
            && frame.generation_complete
            && self.policy.complete_on_generation
        {
            effects.extend(self.complete(LiveCompletionReason::GenerationComplete));
        }
        if self.state.phase.is_terminal() {
            return effects;
        }
        if let Some(time_left_ms) = frame.go_away_time_left_ms {
            self.state.go_away_deadline_ms = Some(types::cross_platform_add(at_ms, time_left_ms));
        }
        effects
    }

    fn complete(&mut self, reason: LiveCompletionReason) -> Vec<LiveLifecycleEffect> {
        match self.policy.kind {
            LiveSessionKind::FiniteRequest | LiveSessionKind::SegmentedStream => {
                self.state.phase = LiveSessionPhase::Completed;
                self.clear_deadlines();
                let mut effects = vec![LiveLifecycleEffect::FinalizeResponse { reason }];
                self.close_effect(&mut effects);
                effects
            }
            LiveSessionKind::ContinuousStream | LiveSessionKind::AgentSession => {
                vec![match reason {
                    LiveCompletionReason::TurnComplete => LiveLifecycleEffect::FinalizeTurn,
                    LiveCompletionReason::GenerationComplete => {
                        LiveLifecycleEffect::FinalizeGeneration
                    }
                    LiveCompletionReason::ContentIdle => {
                        unreachable!("content idle is finite-request policy")
                    }
                }]
            }
        }
    }

    fn transport_failure(
        &mut self,
        at_ms: u64,
        generation: u64,
        retryable: bool,
    ) -> Vec<LiveLifecycleEffect> {
        if !self.accepts(generation) {
            return Vec::new();
        }
        if retryable {
            self.schedule_reconnect(at_ms, LiveReconnectReason::TransportFailure)
        } else {
            self.fail("transportFailure".to_string())
        }
    }

    fn tick(&mut self, at_ms: u64) -> Vec<LiveLifecycleEffect> {
        if self.state.phase == LiveSessionPhase::BackingOff {
            return self.start(at_ms);
        }
        if self.state.phase == LiveSessionPhase::Active
            && let Some(deadline) = self.state.go_away_deadline_ms
        {
            if at_ms >= deadline {
                return self.schedule_reconnect(at_ms, LiveReconnectReason::GoAwayDeadline);
            }
            if self.safe_gap() {
                return self.schedule_reconnect(at_ms, LiveReconnectReason::GoAwaySafeGap);
            }
        }
        if self.state.phase == LiveSessionPhase::AwaitingSetup
            && deadline_reached(at_ms, self.state.setup_deadline_ms)
        {
            return if self.policy.reconnect_enabled {
                self.schedule_reconnect(at_ms, LiveReconnectReason::SetupTimeout)
            } else {
                self.fail("setupTimeout".to_string())
            };
        }
        if self.state.phase != LiveSessionPhase::Active {
            return Vec::new();
        }
        if deadline_reached(at_ms, self.state.hard_response_deadline_ms) {
            return self.fail("hardResponseTimeout".to_string());
        }
        if deadline_reached(at_ms, self.state.first_response_deadline_ms) {
            return self.fail("firstResponseTimeout".to_string());
        }
        if deadline_reached(at_ms, self.state.content_idle_deadline_ms) {
            return self.complete(LiveCompletionReason::ContentIdle);
        }
        if self.policy.server_idle_timeout_ms.is_some_and(|timeout| {
            self.state.input_chunks_since_server_activity
                >= self.policy.server_idle_min_input_chunks
                && self
                    .state
                    .last_server_activity_ms
                    .is_some_and(|last| at_ms.saturating_sub(last) >= timeout)
        }) {
            return self.schedule_reconnect(at_ms, LiveReconnectReason::ServerIdle);
        }
        if self.rotation_due(at_ms) {
            return self.schedule_reconnect(at_ms, LiveReconnectReason::ProactiveRotation);
        }
        Vec::new()
    }

    fn schedule_reconnect(
        &mut self,
        at_ms: u64,
        reason: LiveReconnectReason,
    ) -> Vec<LiveLifecycleEffect> {
        if !self.policy.reconnect_enabled
            || self
                .policy
                .max_reconnect_attempts
                .is_some_and(|max| self.state.reconnect_attempt >= max)
        {
            return self.fail(reason.failure_name().to_string());
        }
        let previous_generation = self.state.generation;
        let current_attempt = self.state.reconnect_attempt;
        let delay_ms = self.backoff.delay_ms(current_attempt);
        self.state.reconnect_attempt = current_attempt
            .saturating_add(1)
            .min(types::CROSS_PLATFORM_ATTEMPT_MAX);
        self.state.generation = types::cross_platform_add(self.state.generation, 1);
        self.state.phase = LiveSessionPhase::BackingOff;
        self.state.reconnect_deadline_ms = Some(types::cross_platform_add(at_ms, delay_ms));
        self.state.socket_open = false;
        self.state.connected_at_ms = None;
        self.clear_session_deadlines();
        let mut effects = Vec::new();
        if previous_generation > 0 {
            effects.push(LiveLifecycleEffect::CloseSocket {
                generation: previous_generation,
            });
        }
        effects.push(LiveLifecycleEffect::ScheduleReconnect {
            generation: self.state.generation,
            attempt: self.state.reconnect_attempt,
            delay_ms,
            reason,
        });
        effects
    }

    fn fail(&mut self, reason: String) -> Vec<LiveLifecycleEffect> {
        let generation = self.state.generation;
        let was_open = self.state.socket_open;
        self.state.phase = LiveSessionPhase::Failed;
        self.state.socket_open = false;
        self.clear_deadlines();
        let mut effects = Vec::new();
        if was_open {
            effects.push(LiveLifecycleEffect::CloseSocket { generation });
        }
        effects.push(LiveLifecycleEffect::ReportFailure { reason });
        effects
    }

    fn cancel(&mut self) -> Vec<LiveLifecycleEffect> {
        let generation = self.state.generation;
        let was_open = self.state.socket_open;
        self.state.phase = LiveSessionPhase::Cancelled;
        self.state.socket_open = false;
        self.clear_deadlines();
        let mut effects = Vec::new();
        if was_open {
            effects.push(LiveLifecycleEffect::CloseSocket { generation });
        }
        effects.push(LiveLifecycleEffect::CancelSession);
        effects
    }

    fn rotation_due(&self, at_ms: u64) -> bool {
        self.policy.rotate_after_ms.is_some_and(|age| {
            self.state
                .connected_at_ms
                .is_some_and(|connected| at_ms.saturating_sub(connected) >= age)
        }) && self
            .state
            .last_input_activity_ms
            .is_some_and(|last| at_ms.saturating_sub(last) >= self.policy.rotation_quiet_ms)
            && self
                .state
                .last_server_activity_ms
                .is_some_and(|last| at_ms.saturating_sub(last) >= self.policy.rotation_quiet_ms)
            && self.safe_gap()
    }

    fn safe_gap(&self) -> bool {
        self.state.pending_work_count == 0
            && self.state.buffered_input_count == 0
            && !self.state.user_speaking
    }

    fn accepts(&self, generation: u64) -> bool {
        generation > 0 && generation == self.state.generation
    }

    fn close_effect(&mut self, effects: &mut Vec<LiveLifecycleEffect>) {
        if self.state.socket_open {
            self.state.socket_open = false;
            effects.push(LiveLifecycleEffect::CloseSocket {
                generation: self.state.generation,
            });
        }
    }

    fn clear_session_deadlines(&mut self) {
        self.state.setup_deadline_ms = None;
        self.state.first_response_deadline_ms = None;
        self.state.content_idle_deadline_ms = None;
        self.state.hard_response_deadline_ms = None;
        self.state.go_away_deadline_ms = None;
    }

    fn clear_deadlines(&mut self) {
        self.clear_session_deadlines();
        self.state.reconnect_deadline_ms = None;
    }
}

fn deadline_reached(at_ms: u64, deadline_ms: Option<u64>) -> bool {
    deadline_ms.is_some_and(|deadline| at_ms >= deadline)
}

fn retain_new_ids(pending: &mut Vec<String>, incoming: Vec<String>) -> Vec<String> {
    let mut added = Vec::new();
    for id in incoming {
        if !pending.contains(&id) {
            pending.push(id.clone());
            added.push(id);
        }
    }
    added
}

fn remove_pending_ids(pending: &mut Vec<String>, incoming: &[String]) -> Vec<String> {
    let mut removed = Vec::new();
    for id in incoming {
        if let Some(index) = pending.iter().position(|pending_id| pending_id == id) {
            removed.push(pending.remove(index));
        }
    }
    removed
}
