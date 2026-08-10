//! Authenticated client for the downloaded, data-only Computer Control engine.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};

use anyhow::{Result, anyhow};
use sgt_computer_control_protocol::{
    Command, McpCatalogPlan, McpCatalogRequest, Output, ProviderEvent, SetupPlan, SetupRequest,
};

mod framing;
mod process;

static SESSION: LazyLock<Mutex<Option<process::EngineProcess>>> =
    LazyLock::new(|| Mutex::new(None));
static ACTIVE_JOB: AtomicBool = AtomicBool::new(false);

struct ActiveJob;

impl ActiveJob {
    fn claim() -> Result<Self> {
        ACTIVE_JOB
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| anyhow!("a Computer Control engine job is already active"))?;
        Ok(Self)
    }
}

impl Drop for ActiveJob {
    fn drop(&mut self) {
        ACTIVE_JOB.store(false, Ordering::Release);
    }
}

pub(super) struct SessionGuard {
    _active_job: ActiveJob,
}

impl SessionGuard {
    pub(super) fn start(cancelled: &AtomicBool) -> Result<Self> {
        let active_job = ActiveJob::claim()?;
        let mut session = SESSION
            .lock()
            .map_err(|_| anyhow!("Computer Control engine lock is poisoned"))?;
        if session.is_some() {
            return Err(anyhow!("Computer Control engine ownership is inconsistent"));
        }
        *session = Some(process::EngineProcess::launch(cancelled)?);
        Ok(Self {
            _active_job: active_job,
        })
    }
}

impl Drop for SessionGuard {
    fn drop(&mut self) {
        if let Ok(mut session) = SESSION.lock()
            && let Some(mut process) = session.take()
        {
            process.shutdown();
        }
    }
}

pub(super) fn build_setup(request: SetupRequest) -> Result<SetupPlan> {
    match request_engine(Command::BuildSetup(request))? {
        Output::Setup(plan) => Ok(plan),
        _ => Err(anyhow!(
            "Computer Control engine returned the wrong setup response"
        )),
    }
}

pub(super) fn parse_provider_frame(frame: &str) -> Result<Vec<ProviderEvent>> {
    match request_engine(Command::ParseProviderFrame {
        frame: frame.to_string(),
    })? {
        Output::ProviderEvents { events } => Ok(events),
        _ => Err(anyhow!(
            "Computer Control engine returned the wrong provider-frame response"
        )),
    }
}

pub(super) fn normalize_mcp_catalog(request: McpCatalogRequest) -> Result<McpCatalogPlan> {
    match request_engine(Command::NormalizeMcpCatalog(request))? {
        Output::McpCatalog(plan) => Ok(plan),
        _ => Err(anyhow!(
            "Computer Control engine returned the wrong MCP catalog response"
        )),
    }
}

fn request_engine(command: Command) -> Result<Output> {
    let mut session = SESSION
        .lock()
        .map_err(|_| anyhow!("Computer Control engine lock is poisoned"))?;
    session
        .as_mut()
        .ok_or_else(|| anyhow!("Computer Control engine is not running"))?
        .request(command)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_one_job_can_claim_engine_ownership() {
        let first = ActiveJob::claim().unwrap();
        assert!(ActiveJob::claim().is_err());
        drop(first);
        assert!(ActiveJob::claim().is_ok());
    }
}
