use super::super::server_frame::LiveServerFrame;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiveSessionKind {
    FiniteRequest,
    ContinuousStream,
    SegmentedStream,
    AgentSession,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LiveSessionPhase {
    #[default]
    Idle,
    Connecting,
    AwaitingSetup,
    Active,
    BackingOff,
    Completed,
    Cancelled,
    Failed,
}

impl LiveSessionPhase {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled | Self::Failed)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveLifecyclePolicy {
    pub kind: LiveSessionKind,
    pub setup_timeout_ms: u64,
    pub first_response_timeout_ms: Option<u64>,
    pub content_idle_ms: Option<u64>,
    pub hard_response_timeout_ms: Option<u64>,
    pub server_idle_timeout_ms: Option<u64>,
    pub server_idle_min_input_chunks: u64,
    pub rotate_after_ms: Option<u64>,
    pub rotation_quiet_ms: u64,
    pub reconnect_enabled: bool,
    pub max_reconnect_attempts: Option<u32>,
    pub complete_on_turn: bool,
    pub complete_on_generation: bool,
}

impl LiveLifecyclePolicy {
    pub fn finite() -> Self {
        Self {
            kind: LiveSessionKind::FiniteRequest,
            setup_timeout_ms: 15_000,
            first_response_timeout_ms: Some(20_000),
            content_idle_ms: Some(1_200),
            hard_response_timeout_ms: Some(90_000),
            server_idle_timeout_ms: None,
            server_idle_min_input_chunks: 0,
            rotate_after_ms: None,
            rotation_quiet_ms: 0,
            reconnect_enabled: false,
            max_reconnect_attempts: None,
            complete_on_turn: true,
            complete_on_generation: true,
        }
    }

    pub fn continuous() -> Self {
        Self {
            kind: LiveSessionKind::ContinuousStream,
            setup_timeout_ms: 15_000,
            first_response_timeout_ms: None,
            content_idle_ms: None,
            hard_response_timeout_ms: None,
            server_idle_timeout_ms: Some(15_000),
            server_idle_min_input_chunks: 100,
            rotate_after_ms: Some(720_000),
            rotation_quiet_ms: 3_000,
            reconnect_enabled: true,
            max_reconnect_attempts: None,
            complete_on_turn: true,
            complete_on_generation: true,
        }
    }

    pub fn agent() -> Self {
        Self {
            kind: LiveSessionKind::AgentSession,
            max_reconnect_attempts: Some(6),
            rotate_after_ms: None,
            server_idle_timeout_ms: None,
            ..Self::continuous()
        }
    }

    pub(super) fn normalized_for_shared_domain(mut self) -> Self {
        self.setup_timeout_ms = cross_platform_clamp(self.setup_timeout_ms);
        self.first_response_timeout_ms = self.first_response_timeout_ms.map(cross_platform_clamp);
        self.content_idle_ms = self.content_idle_ms.map(cross_platform_clamp);
        self.hard_response_timeout_ms = self.hard_response_timeout_ms.map(cross_platform_clamp);
        self.server_idle_timeout_ms = self.server_idle_timeout_ms.map(cross_platform_clamp);
        self.server_idle_min_input_chunks = cross_platform_clamp(self.server_idle_min_input_chunks);
        self.rotate_after_ms = self.rotate_after_ms.map(cross_platform_clamp);
        self.rotation_quiet_ms = cross_platform_clamp(self.rotation_quiet_ms);
        self.max_reconnect_attempts = self
            .max_reconnect_attempts
            .map(|attempts| attempts.min(CROSS_PLATFORM_ATTEMPT_MAX));
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LiveBackoffPolicy {
    pub base_ms: u64,
    pub exponent_cap: u32,
    pub jitter_min_ms: u64,
    pub jitter_seed: u64,
    pub jitter_step: u64,
    pub jitter_span: u64,
    pub max_ms: u64,
}

impl LiveBackoffPolicy {
    pub fn delay_ms(self, attempt: u32) -> u64 {
        // Keep the public policy total for extreme caller-provided values and
        // in lockstep with Kotlin's positive signed-Long saturation boundary.
        let attempt = attempt.min(CROSS_PLATFORM_ATTEMPT_MAX);
        let exponent = attempt.min(self.exponent_cap).min(62);
        let base = cross_platform_multiply(self.base_ms, 1_u64 << exponent);
        let jitter = if self.jitter_span == 0 {
            self.jitter_min_ms.min(CROSS_PLATFORM_MAX)
        } else {
            let stepped_seed = cross_platform_add(
                self.jitter_seed,
                cross_platform_multiply(u64::from(attempt), self.jitter_step),
            );
            cross_platform_add(
                self.jitter_min_ms,
                stepped_seed % self.jitter_span.min(CROSS_PLATFORM_MAX),
            )
        };
        cross_platform_add(base, jitter).min(self.max_ms.min(CROSS_PLATFORM_MAX))
    }
}

pub(super) const CROSS_PLATFORM_MAX: u64 = i64::MAX as u64;
pub(super) const CROSS_PLATFORM_ATTEMPT_MAX: u32 = i32::MAX as u32;

pub(super) fn cross_platform_clamp(value: u64) -> u64 {
    value.min(CROSS_PLATFORM_MAX)
}

pub(super) fn cross_platform_add(left: u64, right: u64) -> u64 {
    cross_platform_clamp(left)
        .saturating_add(cross_platform_clamp(right))
        .min(CROSS_PLATFORM_MAX)
}

fn cross_platform_multiply(left: u64, right: u64) -> u64 {
    left.min(CROSS_PLATFORM_MAX)
        .saturating_mul(right.min(CROSS_PLATFORM_MAX))
        .min(CROSS_PLATFORM_MAX)
}

impl Default for LiveBackoffPolicy {
    fn default() -> Self {
        Self {
            base_ms: 250,
            exponent_cap: 5,
            jitter_min_ms: 20,
            jitter_seed: 7,
            jitter_step: 53,
            jitter_span: 180,
            max_ms: 6_000,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LiveLifecycleState {
    pub phase: LiveSessionPhase,
    pub generation: u64,
    pub reconnect_attempt: u32,
    pub socket_open: bool,
    pub connection_started_at_ms: Option<u64>,
    pub connected_at_ms: Option<u64>,
    pub setup_deadline_ms: Option<u64>,
    pub first_response_deadline_ms: Option<u64>,
    pub content_idle_deadline_ms: Option<u64>,
    pub hard_response_deadline_ms: Option<u64>,
    pub reconnect_deadline_ms: Option<u64>,
    pub go_away_deadline_ms: Option<u64>,
    pub has_output: bool,
    pub input_chunks_since_server_activity: u64,
    pub last_input_activity_ms: Option<u64>,
    pub last_server_activity_ms: Option<u64>,
    pub pending_work_count: u64,
    pub buffered_input_count: u64,
    pub user_speaking: bool,
    pub pending_tool_ids: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LiveLifecycleEvent {
    Start,
    SocketOpened {
        generation: u64,
    },
    Frame(LiveLifecycleFrame),
    TransportFailure {
        generation: u64,
        retryable: bool,
    },
    InputSent {
        chunks: u64,
    },
    InputActivity,
    WorkState {
        pending_work_count: u64,
        buffered_input_count: u64,
        user_speaking: bool,
    },
    Tick,
    Cancel,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LiveLifecycleFrame {
    pub generation: u64,
    pub content_count: usize,
    pub setup_complete: bool,
    pub turn_complete: bool,
    pub generation_complete: bool,
    pub interrupted: bool,
    pub go_away_time_left_ms: Option<u64>,
    pub tool_call_ids: Vec<String>,
    pub tool_cancellation_ids: Vec<String>,
    pub error: Option<LiveClassifiedError>,
}

impl LiveLifecycleFrame {
    pub fn from_server_frame(generation: u64, frame: &LiveServerFrame) -> Self {
        Self {
            generation,
            content_count: frame.content_count(),
            setup_complete: frame.setup_complete,
            turn_complete: frame.turn_complete,
            generation_complete: frame.generation_complete,
            interrupted: frame.interrupted,
            go_away_time_left_ms: frame
                .go_away
                .as_ref()
                .and_then(|notice| duration_ms(&notice.time_left)),
            tool_call_ids: frame
                .tool_calls
                .iter()
                .map(|call| call.id.clone())
                .collect(),
            tool_cancellation_ids: frame.tool_cancellation_ids.clone().unwrap_or_default(),
            error: frame.error.as_ref().map(|_| LiveClassifiedError {
                kind: "server".to_string(),
                retryable: frame.error_retryable,
            }),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveClassifiedError {
    pub kind: String,
    pub retryable: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiveCompletionReason {
    TurnComplete,
    GenerationComplete,
    ContentIdle,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiveReconnectReason {
    SetupTimeout,
    TransportFailure,
    ServerError,
    ServerIdle,
    ProactiveRotation,
    GoAwaySafeGap,
    GoAwayDeadline,
}

impl LiveReconnectReason {
    pub(super) fn failure_name(self) -> &'static str {
        match self {
            Self::SetupTimeout => "setupTimeout",
            Self::TransportFailure => "transportFailure",
            Self::ServerError => "serverError",
            Self::ServerIdle => "serverIdle",
            Self::ProactiveRotation => "proactiveRotation",
            Self::GoAwaySafeGap => "goAwaySafeGap",
            Self::GoAwayDeadline => "goAwayDeadline",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LiveLifecycleEffect {
    OpenSocket {
        generation: u64,
    },
    SendSetup {
        generation: u64,
    },
    DeliverContent {
        count: usize,
    },
    FinalizeResponse {
        reason: LiveCompletionReason,
    },
    FinalizeGeneration,
    FinalizeTurn,
    StopPlayback,
    DiscardQueuedOutput,
    FinalizeInterruptedGeneration,
    DispatchTools {
        ids: Vec<String>,
    },
    CancelTools {
        ids: Vec<String>,
    },
    CloseSocket {
        generation: u64,
    },
    ScheduleReconnect {
        generation: u64,
        attempt: u32,
        delay_ms: u64,
        reason: LiveReconnectReason,
    },
    ReportFailure {
        reason: String,
    },
    CancelSession,
}

fn duration_ms(value: &str) -> Option<u64> {
    let raw = value.strip_suffix('s')?;
    let (seconds, fraction) = raw
        .split_once('.')
        .map_or((raw, None), |(seconds, fraction)| (seconds, Some(fraction)));
    if seconds.is_empty() || !seconds.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let seconds = seconds.parse::<u64>().ok()?;
    let cross_platform_max = i64::MAX as u64;
    if seconds > cross_platform_max {
        return None;
    }
    let fraction_ms = match fraction {
        None => 0,
        Some(fraction)
            if !fraction.is_empty()
                && fraction.len() <= 9
                && fraction.bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            let mut nanos = fraction.parse::<u64>().ok()?;
            for _ in fraction.len()..9 {
                nanos *= 10;
            }
            (nanos + 500_000) / 1_000_000
        }
        Some(_) => return None,
    };
    Some(
        seconds
            .saturating_mul(1_000)
            .saturating_add(fraction_ms)
            .min(cross_platform_max),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_is_total_for_extreme_public_policy_values() {
        let policy = LiveBackoffPolicy {
            base_ms: u64::MAX,
            exponent_cap: u32::MAX,
            jitter_min_ms: u64::MAX,
            jitter_seed: u64::MAX,
            jitter_step: u64::MAX,
            jitter_span: u64::MAX,
            max_ms: 6_000,
        };

        assert_eq!(policy.delay_ms(u32::MAX), 6_000);
    }

    #[test]
    fn backoff_jitter_saturates_at_the_shared_signed_long_boundary() {
        let policy = LiveBackoffPolicy {
            base_ms: 0,
            exponent_cap: 0,
            jitter_min_ms: 0,
            jitter_seed: CROSS_PLATFORM_MAX,
            jitter_step: 1,
            jitter_span: 180,
            max_ms: CROSS_PLATFORM_MAX,
        };

        assert_eq!(policy.delay_ms(1), CROSS_PLATFORM_MAX % 180);
    }

    #[test]
    fn protobuf_duration_rounding_matches_android() {
        assert_eq!(duration_ms("1.999999999s"), Some(2_000));
        assert_eq!(duration_ms("0.0005s"), Some(1));
        assert_eq!(duration_ms("-1s"), None);
        assert_eq!(duration_ms("1.0000000000s"), None);
    }
}
