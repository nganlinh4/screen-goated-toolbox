use super::*;

/// How long a window whose control connection failed, or that stopped
/// answering, gets to finalize its recording and exit before it is killed.
pub(super) const UNRESPONSIVE_STOP_GRACE: Duration = Duration::from_secs(10);

/// Whether a launch may download the recorder, or a capability it lacks.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum ComponentDelivery {
    /// A feature the user invoked: install what is missing, with progress.
    InstallIfMissing,
    /// Optional work the host can do without: use only what is installed.
    InstalledOnly,
}

impl ComponentDelivery {
    /// An audio decode is only a fallback after the Windows media stack
    /// failed, so it must never start a multi-hundred-MB download.
    pub(super) fn for_headless(command: &Command) -> Self {
        match command {
            Command::DecodeAudio { .. } => Self::InstalledOnly,
            _ => Self::InstallIfMissing,
        }
    }

    pub(super) fn may_install(self) -> bool {
        self == Self::InstallIfMissing
    }
}

/// Optional work was skipped because the recorder component is not installed.
#[derive(Debug)]
pub(super) struct OptionalComponentUnavailable;

impl std::fmt::Display for OptionalComponentUnavailable {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(
            "the Screen Recorder component is not installed; optional recorder work was skipped",
        )
    }
}

impl std::error::Error for OptionalComponentUnavailable {}

pub(super) fn request_with_capability_retry(
    session: &mut Option<WorkerSession>,
    command: Command,
    timeout: Duration,
    slot: session_lifecycle::JobSlot,
    delivery: ComponentDelivery,
) -> Result<serde_json::Value> {
    let preserve = slot.preserves_session();
    drop_exited_session(session);
    let newly_launched = session.is_none();
    if newly_launched {
        let cancelled = session_lifecycle::LaunchCancellation::begin(slot)?;
        *session = Some(launch(cancelled.token(), None, slot, delivery)?);
    }
    let first = session
        .as_mut()
        .ok_or_else(|| anyhow!("recorder worker did not start"))?
        .request_impl(command.clone(), timeout, !preserve);
    let error = match first {
        Ok(value) => return Ok(value),
        Err(error) => error,
    };
    if preserve && error.downcast_ref::<WorkerTransportError>().is_some() {
        retire_unresponsive(session, UNRESPONSIVE_STOP_GRACE);
        return Err(error);
    }
    if preserve && !newly_launched || !delivery.may_install() {
        return Err(error);
    }
    let Some(capability) = error
        .downcast_ref::<MissingExternalCapability>()
        .map(|missing| missing.tool)
    else {
        return Err(error);
    };
    session.take();
    let cancelled = session_lifecycle::LaunchCancellation::begin(slot)?;
    *session = Some(launch(cancelled.token(), Some(capability), slot, delivery)?);
    session
        .as_mut()
        .ok_or_else(|| anyhow!("recorder worker did not restart"))?
        .request_impl(command, timeout, !preserve)
        .context("recorder command failed after one capability repair retry")
}

/// Retires an interactive window whose control connection failed or that
/// stopped answering, so the next command launches a fresh one. The window is
/// first asked to finalize and exit, so what it recorded so far stays
/// recoverable; whatever still runs when `grace` ends is killed.
pub(super) fn retire_unresponsive(session: &mut Option<WorkerSession>, grace: Duration) {
    let Some(mut live) = session.take() else {
        return;
    };
    let exited = live.stop_gracefully(grace);
    crate::log_info!(
        "[ScreenRecord] retiring an unresponsive recorder window ({}); the next command relaunches it (recording files may be in {})",
        if exited {
            "it finalized and exited"
        } else {
            "killed after the graceful stop deadline"
        },
        recordings_dir()
    );
    // Dropping the session kills whatever is still running.
}

/// A worker that died since the last command (crash, external kill) is
/// replaced by a fresh launch instead of failing the next command.
pub(super) fn drop_exited_session(session: &mut Option<WorkerSession>) {
    let Some(status) = session
        .as_mut()
        .and_then(|live| live.child.try_wait().ok().flatten())
    else {
        return;
    };
    crate::log_info!(
        "[ScreenRecord] worker exited with {status}; relaunching for the next command (retained recording files may be in {})",
        recordings_dir()
    );
    session.take();
}

impl WorkerSession {
    /// Writes PrepareShutdown then Shutdown without waiting for answers, which
    /// may never arrive over a broken connection, and waits up to `grace` for
    /// the worker to exit. A helper thread writes, so a worker that stopped
    /// reading its input cannot block the dispatcher past the deadline.
    fn stop_gracefully(&mut self, grace: Duration) -> bool {
        let deadline = Instant::now() + grace;
        let writer = self.stdin.clone();
        let _ = std::thread::Builder::new()
            .name("sgt-recorder-retire".to_string())
            .spawn(move || {
                for command in [Command::PrepareShutdown, Command::Shutdown] {
                    if writer.send(command).is_err() {
                        return;
                    }
                }
            });
        loop {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    pub(super) fn shutdown(&mut self) -> Result<()> {
        self.request(Command::Shutdown, NORMAL_TIMEOUT)?;
        self.child.stop_within(EXIT_TIMEOUT)
    }
    pub(super) fn request(
        &mut self,
        command: Command,
        timeout: Duration,
    ) -> Result<serde_json::Value> {
        self.request_impl(command, timeout, true)
    }

    pub(super) fn request_preserving(
        &mut self,
        command: Command,
        timeout: Duration,
    ) -> Result<serde_json::Value> {
        self.request_impl(command, timeout, false)
    }

    fn request_impl(
        &mut self,
        command: Command,
        timeout: Duration,
        terminate_on_error: bool,
    ) -> Result<serde_json::Value> {
        let result = (|| {
            if let Some(status) = self.child.try_wait()? {
                return Err(transport(format!(
                    "recorder worker exited with {status}; retained recording files may be in {}",
                    recordings_dir()
                )));
            }
            // A rejected command never reaches the worker, so it is not a
            // transport failure.
            command.validate().map_err(anyhow::Error::msg)?;
            let request_id = self
                .stdin
                .send(command)
                .map_err(|error| transport(format!("{error:#}")))?;
            let deadline = Instant::now() + timeout;
            loop {
                match self
                    .responses
                    .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                {
                    Ok(ResponseEvent::Response(response)) => {
                        let stale = response.request_id < request_id;
                        let expected = if stale {
                            response.request_id
                        } else {
                            request_id
                        };
                        response
                            .validate(&self.token, expected)
                            .map_err(transport)?;
                        if stale {
                            continue;
                        }
                        if let Some(error) = response.error {
                            if let Some(tool) =
                                crate::component_registry::capabilities::requested_external_tool(
                                    &error,
                                )
                            {
                                return Err(MissingExternalCapability { tool }.into());
                            }
                            return Err(WorkerCommandError(error).into());
                        }
                        break response
                            .result
                            .ok_or_else(|| anyhow!("recorder worker returned an empty response"));
                    }
                    Ok(ResponseEvent::Failed(error)) => {
                        return Err(transport(format!(
                            "recorder control connection failed: {error}; recording files may be in {}",
                            recordings_dir()
                        )));
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        return Err(transport("recorder worker response timed out".to_string()));
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        return Err(transport(format!(
                            "recorder control connection closed; recording files may be in {}",
                            recordings_dir()
                        )));
                    }
                }
            }
        })();
        if let Err(error) = &result
            && should_terminate(error, terminate_on_error)
        {
            let _ = self.child.terminate();
        }
        result
    }
}

/// An error the worker itself answered with: the session and its control
/// connection are still healthy.
#[derive(Debug)]
pub(super) struct WorkerCommandError(pub(super) String);

impl std::fmt::Display for WorkerCommandError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for WorkerCommandError {}

/// The control connection itself failed: the worker exited, its stream broke
/// or was unauthenticated, or it did not answer in time.
#[derive(Debug)]
pub(super) struct WorkerTransportError(String);

impl std::fmt::Display for WorkerTransportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for WorkerTransportError {}

fn transport(message: String) -> anyhow::Error {
    WorkerTransportError(message).into()
}

fn recordings_dir() -> String {
    crate::paths::app_runtime_local_data_dir()
        .join("recordings")
        .display()
        .to_string()
}

/// A disposable session is retired when its transport failed (exit, broken
/// or unauthenticated connection, timeout) or it must relaunch with a missing
/// capability. A command the worker answered with an error leaves it healthy,
/// so it can still shut down gracefully. The interactive window is retired
/// separately, and only for a transport failure (see `retire_unresponsive`).
fn should_terminate(error: &anyhow::Error, terminate_on_error: bool) -> bool {
    terminate_on_error && error.downcast_ref::<WorkerCommandError>().is_none()
}
