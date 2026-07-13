use std::collections::VecDeque;

use super::super::transport::{activate_s2s_socket, connect_s2s_socket};
use super::super::*;

struct PendingConnection {
    generation: u64,
    socket: ConnectedLiveSocket,
}

struct ActiveConnection {
    generation: u64,
    session: ReadyLiveSession,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SetupFailureDisposition {
    TimeoutAt(u64),
    ServerError { retryable: bool },
    RetryableTransport,
}

pub(super) enum AdapterPoll {
    Idle,
    StateChanged,
    Frame {
        frame: Box<LiveServerFrame>,
        effects: Vec<LiveLifecycleEffect>,
    },
}

pub(super) struct LiveTranslateLifecycleAdapter {
    lifecycle: LiveSessionLifecycle,
    epoch: Instant,
    settings: S2sSettings,
    context: S2sContextSnapshot,
    pending: Option<PendingConnection>,
    active: Option<ActiveConnection>,
    failure: Option<String>,
}

impl LiveTranslateLifecycleAdapter {
    pub(super) fn new(settings: S2sSettings) -> Self {
        Self {
            lifecycle: LiveSessionLifecycle::new(continuous_policy(), LiveBackoffPolicy::default()),
            epoch: Instant::now(),
            settings,
            context: S2sContextSnapshot {
                text: String::new(),
            },
            pending: None,
            active: None,
            failure: None,
        }
    }

    pub(super) fn start(&mut self, cancelled: &mut dyn FnMut() -> bool) -> Result<()> {
        if cancelled() {
            return self.cancel();
        }
        let effects = self
            .lifecycle
            .reduce(self.now_ms(), LiveLifecycleEvent::Start);
        self.execute_effects(effects, cancelled).map(|_| ())
    }

    pub(super) fn tick(
        &mut self,
        pending_work_count: u64,
        buffered_input_count: u64,
        user_speaking: bool,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<()> {
        if cancelled() {
            return self.cancel();
        }
        let now = self.now_ms();
        let work_effects = self.lifecycle.reduce(
            now,
            LiveLifecycleEvent::WorkState {
                pending_work_count,
                buffered_input_count,
                user_speaking,
            },
        );
        self.execute_effects(work_effects, cancelled)?;
        let tick_effects = self.lifecycle.reduce(now, LiveLifecycleEvent::Tick);
        self.execute_effects(tick_effects, cancelled).map(|_| ())
    }

    pub(super) fn send_audio(
        &mut self,
        samples: &[i16],
        input_active: bool,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<bool> {
        if cancelled() {
            self.cancel()?;
            return Ok(false);
        }
        let generation = self.lifecycle.state().generation;
        if self.lifecycle.state().phase != LiveSessionPhase::Active
            || self
                .active
                .as_ref()
                .is_none_or(|connection| connection.generation != generation)
        {
            return Ok(false);
        }

        let now = self.now_ms();
        if input_active {
            let effects = self
                .lifecycle
                .reduce(now, LiveLifecycleEvent::InputActivity);
            self.execute_effects(effects, cancelled)?;
        }
        let result = self
            .active
            .as_mut()
            .expect("active generation checked")
            .session
            .send_audio_pcm(samples, 16_000);
        if let Err(error) = result {
            crate::log_info!(
                "[{}] continuous send failed generation={} error={}",
                self.settings.mode.log_tag(),
                generation,
                error
            );
            let effects = self.lifecycle.reduce(
                self.now_ms(),
                LiveLifecycleEvent::TransportFailure {
                    generation,
                    retryable: true,
                },
            );
            self.execute_effects(effects, cancelled)?;
            return Ok(false);
        }

        let effects = self
            .lifecycle
            .reduce(now, LiveLifecycleEvent::InputSent { chunks: 1 });
        self.execute_effects(effects, cancelled)?;
        Ok(true)
    }

    pub(super) fn poll(&mut self, cancelled: &mut dyn FnMut() -> bool) -> Result<AdapterPoll> {
        if cancelled() {
            self.cancel()?;
            return Ok(AdapterPoll::StateChanged);
        }
        let generation = self.lifecycle.state().generation;
        let Some(connection) = self.active.as_mut() else {
            return Ok(AdapterPoll::StateChanged);
        };
        if connection.generation != generation {
            return Ok(AdapterPoll::StateChanged);
        }

        let poll = connection.session.poll();
        match poll {
            Ok(LivePoll::Frame(frame)) => {
                let lifecycle_frame = LiveLifecycleFrame::from_server_frame(generation, &frame);
                let effects = self
                    .lifecycle
                    .reduce(self.now_ms(), LiveLifecycleEvent::Frame(lifecycle_frame));
                let effects = self.execute_effects(effects, cancelled)?;
                Ok(AdapterPoll::Frame { frame, effects })
            }
            Ok(LivePoll::Idle) => Ok(AdapterPoll::Idle),
            Ok(LivePoll::Unparsed { .. }) => Ok(AdapterPoll::Idle),
            Ok(LivePoll::PeerClosed(frame)) => {
                crate::log_info!(
                    "[{}] continuous socket closed generation={} frame={:?}",
                    self.settings.mode.log_tag(),
                    generation,
                    frame
                );
                self.transport_failure(generation, true, cancelled)?;
                Ok(AdapterPoll::StateChanged)
            }
            Ok(LivePoll::ServerError(error)) => {
                let effects = self.lifecycle.reduce(
                    self.now_ms(),
                    LiveLifecycleEvent::Frame(LiveLifecycleFrame {
                        generation,
                        error: Some(LiveClassifiedError {
                            kind: format!("server: {error}"),
                            retryable: error.retryable,
                        }),
                        ..LiveLifecycleFrame::default()
                    }),
                );
                self.execute_effects(effects, cancelled)?;
                Ok(AdapterPoll::StateChanged)
            }
            Err(error) => {
                let retryable = is_recoverable_anyhow_socket_error(&error);
                crate::log_info!(
                    "[{}] continuous socket read failed generation={} retryable={} error={}",
                    self.settings.mode.log_tag(),
                    generation,
                    retryable,
                    error
                );
                self.transport_failure(generation, retryable, cancelled)?;
                Ok(AdapterPoll::StateChanged)
            }
        }
    }

    pub(super) fn cancel(&mut self) -> Result<()> {
        let effects = self
            .lifecycle
            .reduce(self.now_ms(), LiveLifecycleEvent::Cancel);
        let mut never_cancelled = || false;
        self.execute_effects(effects, &mut never_cancelled)
            .map(|_| ())
    }

    pub(super) fn generation(&self) -> u64 {
        self.lifecycle.state().generation
    }

    pub(super) fn reconnect_attempt(&self) -> u32 {
        self.lifecycle.state().reconnect_attempt
    }

    pub(super) fn is_active(&self) -> bool {
        self.lifecycle.state().phase == LiveSessionPhase::Active && self.active.is_some()
    }

    pub(super) fn socket_age_ms(&self) -> u64 {
        self.elapsed_since(self.lifecycle.state().connected_at_ms)
    }

    pub(super) fn since_server_activity_ms(&self) -> u64 {
        self.elapsed_since(self.lifecycle.state().last_server_activity_ms)
    }

    pub(super) fn since_input_activity_ms(&self) -> u64 {
        self.elapsed_since(self.lifecycle.state().last_input_activity_ms)
    }

    fn transport_failure(
        &mut self,
        generation: u64,
        retryable: bool,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<()> {
        let effects = self.lifecycle.reduce(
            self.now_ms(),
            LiveLifecycleEvent::TransportFailure {
                generation,
                retryable,
            },
        );
        self.execute_effects(effects, cancelled).map(|_| ())
    }

    fn execute_effects(
        &mut self,
        effects: Vec<LiveLifecycleEffect>,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<Vec<LiveLifecycleEffect>> {
        let mut pending_effects = VecDeque::from(effects);
        let mut feature_effects = Vec::new();
        while let Some(effect) = pending_effects.pop_front() {
            match effect {
                LiveLifecycleEffect::OpenSocket { generation } => {
                    self.open_socket(generation, cancelled, &mut pending_effects);
                }
                LiveLifecycleEffect::SendSetup { generation } => {
                    self.send_setup(generation, cancelled, &mut pending_effects);
                }
                LiveLifecycleEffect::CloseSocket { generation } => {
                    self.close_socket(generation);
                }
                LiveLifecycleEffect::ScheduleReconnect {
                    generation,
                    attempt,
                    delay_ms,
                    reason,
                } => {
                    crate::log_info!(
                        "[{}] continuous reconnect scheduled reason={} generation={} attempt={} retry_ms={}",
                        self.settings.mode.log_tag(),
                        reconnect_reason_name(reason),
                        generation,
                        attempt,
                        delay_ms
                    );
                }
                LiveLifecycleEffect::ReportFailure { reason } => {
                    self.failure = Some(reason);
                }
                LiveLifecycleEffect::CancelSession => {}
                feature_effect => feature_effects.push(feature_effect),
            }
        }
        if let Some(failure) = &self.failure {
            anyhow::bail!("Gemini Live lifecycle failed: {failure}");
        }
        Ok(feature_effects)
    }

    fn open_socket(
        &mut self,
        generation: u64,
        cancelled: &mut dyn FnMut() -> bool,
        pending_effects: &mut VecDeque<LiveLifecycleEffect>,
    ) {
        match connect_s2s_socket(&self.settings) {
            Ok(socket) if !cancelled() => {
                self.pending = Some(PendingConnection { generation, socket });
                pending_effects.extend(self.lifecycle.reduce(
                    self.now_ms(),
                    LiveLifecycleEvent::SocketOpened { generation },
                ));
            }
            Ok(_) => {
                pending_effects.extend(
                    self.lifecycle
                        .reduce(self.now_ms(), LiveLifecycleEvent::Cancel),
                );
            }
            Err(error) => {
                crate::log_info!(
                    "[{}] continuous socket connect failed generation={} error={}",
                    self.settings.mode.log_tag(),
                    generation,
                    error
                );
                pending_effects.extend(self.lifecycle.reduce(
                    self.now_ms(),
                    LiveLifecycleEvent::TransportFailure {
                        generation,
                        retryable: true,
                    },
                ));
            }
        }
    }

    fn send_setup(
        &mut self,
        generation: u64,
        cancelled: &mut dyn FnMut() -> bool,
        pending_effects: &mut VecDeque<LiveLifecycleEffect>,
    ) {
        let Some(connection) = self.pending.take() else {
            return;
        };
        if connection.generation != generation {
            self.pending = Some(connection);
            return;
        }

        match activate_s2s_socket(
            connection.socket,
            &self.settings,
            &self.context,
            Duration::from_millis(2),
            &mut *cancelled,
        ) {
            Ok(session) => {
                self.active = Some(ActiveConnection {
                    generation,
                    session,
                });
                crate::log_info!(
                    "[{}] continuous socket connected generation={} reconnect_attempts={}",
                    self.settings.mode.log_tag(),
                    generation,
                    self.lifecycle.state().reconnect_attempt
                );
                pending_effects.extend(self.lifecycle.reduce(
                    self.now_ms(),
                    LiveLifecycleEvent::Frame(LiveLifecycleFrame {
                        generation,
                        setup_complete: true,
                        ..LiveLifecycleFrame::default()
                    }),
                ));
            }
            Err(_) if cancelled() => {
                pending_effects.extend(
                    self.lifecycle
                        .reduce(self.now_ms(), LiveLifecycleEvent::Cancel),
                );
            }
            Err(error) => {
                crate::log_info!(
                    "[{}] continuous setup failed generation={} error={}",
                    self.settings.mode.log_tag(),
                    generation,
                    error
                );
                let now_ms = self.now_ms();
                let disposition = classify_setup_failure(
                    &error,
                    now_ms,
                    self.lifecycle.state().setup_deadline_ms,
                );
                match disposition {
                    SetupFailureDisposition::TimeoutAt(at_ms) => {
                        pending_effects
                            .extend(self.lifecycle.reduce(at_ms, LiveLifecycleEvent::Tick));
                    }
                    SetupFailureDisposition::ServerError { retryable } => {
                        pending_effects.extend(self.lifecycle.reduce(
                            now_ms,
                            LiveLifecycleEvent::Frame(LiveLifecycleFrame {
                                generation,
                                error: Some(LiveClassifiedError {
                                    kind: "setupServerError".to_string(),
                                    retryable,
                                }),
                                ..LiveLifecycleFrame::default()
                            }),
                        ));
                    }
                    SetupFailureDisposition::RetryableTransport => {
                        pending_effects.extend(self.lifecycle.reduce(
                            now_ms,
                            LiveLifecycleEvent::TransportFailure {
                                generation,
                                retryable: true,
                            },
                        ));
                    }
                }
            }
        }
    }

    fn close_socket(&mut self, generation: u64) {
        if self
            .pending
            .as_ref()
            .is_some_and(|connection| connection.generation == generation)
        {
            self.pending = None;
        }
        if self
            .active
            .as_ref()
            .is_some_and(|connection| connection.generation == generation)
            && let Some(mut connection) = self.active.take()
            && let Err(error) = connection.session.close()
        {
            crate::log_info!(
                "[{}] continuous socket close failed generation={} error={}",
                self.settings.mode.log_tag(),
                generation,
                error
            );
        }
    }

    fn now_ms(&self) -> u64 {
        u64::try_from(self.epoch.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    fn elapsed_since(&self, then_ms: Option<u64>) -> u64 {
        then_ms.map_or(0, |then| self.now_ms().saturating_sub(then))
    }
}

fn continuous_policy() -> LiveLifecyclePolicy {
    LiveLifecyclePolicy::continuous()
}

fn classify_setup_failure(
    error: &anyhow::Error,
    now_ms: u64,
    setup_deadline_ms: Option<u64>,
) -> SetupFailureDisposition {
    if error
        .chain()
        .any(|cause| cause.to_string() == "Gemini Live setup timed out")
    {
        // `activate_with` blocks until its own monotonic deadline. Reduce at no
        // earlier than the lifecycle deadline so millisecond rounding cannot
        // accidentally reclassify that timeout as a generic transport error.
        return SetupFailureDisposition::TimeoutAt(
            setup_deadline_ms.map_or(now_ms, |deadline| now_ms.max(deadline)),
        );
    }
    if let Some(server_error) = error.downcast_ref::<LiveSetupServerError>() {
        SetupFailureDisposition::ServerError {
            retryable: server_error.server.retryable,
        }
    } else {
        SetupFailureDisposition::RetryableTransport
    }
}

fn reconnect_reason_name(reason: LiveReconnectReason) -> &'static str {
    match reason {
        LiveReconnectReason::SetupTimeout => "setup-timeout",
        LiveReconnectReason::TransportFailure => "transport-failure",
        LiveReconnectReason::ServerError => "server-error",
        LiveReconnectReason::ServerIdle => "server-silent",
        LiveReconnectReason::ProactiveRotation => "proactive-rotation",
        LiveReconnectReason::GoAwaySafeGap => "go-away-safe-gap",
        LiveReconnectReason::GoAwayDeadline => "go-away-deadline",
    }
}

#[cfg(test)]
#[path = "lifecycle_adapter_tests.rs"]
mod tests;
