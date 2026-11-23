//! Authenticated client for the downloaded, data-only Computer Control engine.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};

use anyhow::{Result, anyhow};
use sgt_computer_control_protocol::{
    Command, McpCatalogPlan, McpCatalogRequest, Output, ProviderEvent, SetupPlan, SetupRequest,
};

mod framing;
mod process;
mod rejection;
pub(super) use rejection::{DataRejection, RejectionKind};

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

/// Retain only declarations that passed the host's complete setup validation.
/// Their lifetime is the authenticated engine session, including reconnects.
pub(super) fn remember_validated_tools(tools: &serde_json::Value) -> Result<()> {
    let declarations = declarations_by_name(tools)?;
    let mut session = SESSION
        .lock()
        .map_err(|_| anyhow!("Computer Control engine lock is poisoned"))?;
    if let Some(session) = session.as_mut() {
        session.tool_declarations = declarations;
    }
    Ok(())
}

pub(super) fn tool_declaration(name: &str) -> Option<serde_json::Value> {
    SESSION
        .lock()
        .ok()?
        .as_ref()?
        .tool_declarations
        .get(name)
        .cloned()
}

fn declarations_by_name(
    tools: &serde_json::Value,
) -> Result<std::collections::HashMap<String, serde_json::Value>> {
    let declarations = tools
        .as_array()
        .and_then(|tools| tools.last())
        .and_then(|tool| tool.get("functionDeclarations"))
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| anyhow!("validated tool declarations unavailable"))?;
    let mut result = std::collections::HashMap::with_capacity(declarations.len());
    for declaration in declarations {
        let name = declaration
            .get("name")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| anyhow!("validated tool name unavailable"))?;
        if result
            .insert(name.to_string(), declaration.clone())
            .is_some()
        {
            return Err(anyhow!("duplicate validated tool name"));
        }
    }
    Ok(result)
}

pub(super) fn parse_provider_frame(frame: &str) -> Result<Vec<ProviderEvent>> {
    provider_frame_events(request_engine(Command::ParseProviderFrame {
        frame: frame.to_string(),
    })?)
}

fn provider_frame_events(output: Output) -> Result<Vec<ProviderEvent>> {
    match output {
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

pub(super) fn stop_for_component_removal() {
    if let Ok(mut session) = SESSION.lock()
        && let Some(mut process) = session.take()
    {
        process.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrong_provider_response_variant_cannot_be_downgraded_to_data_rejection() {
        let error = provider_frame_events(Output::Shutdown)
            .unwrap_err()
            .context("provider decoding");
        assert_eq!(DataRejection::classify(&error), None);
        let events = provider_frame_events(Output::ProviderEvents {
            events: vec![ProviderEvent::SetupComplete],
        })
        .unwrap();
        assert!(matches!(events.as_slice(), [ProviderEvent::SetupComplete]));
    }

    #[test]
    fn checked_catalog_retains_exact_static_and_dynamic_declarations() {
        let first = serde_json::json!({"name":"static_tool","description":"first","parameters":{}});
        let dynamic = serde_json::json!({"name":"mcp__future","description":"dynamic","parameters":{"type":"object"}});
        let declarations = declarations_by_name(&serde_json::json!([
            {"googleSearch":{}},{"functionDeclarations":[first,dynamic]},
        ]))
        .unwrap();
        assert_eq!(declarations.len(), 2);
        assert_eq!(declarations["static_tool"], first);
        assert_eq!(declarations["mcp__future"], dynamic);
        for invalid in [
            serde_json::json!([]),
            serde_json::json!([{"functionDeclarations":[{},first]}]),
            serde_json::json!([{"functionDeclarations":[first,first]}]),
        ] {
            assert!(declarations_by_name(&invalid).is_err());
        }
    }

    #[test]
    fn only_one_job_can_claim_engine_ownership() {
        let first = ActiveJob::claim().unwrap();
        assert!(ActiveJob::claim().is_err());
        drop(first);
        assert!(ActiveJob::claim().is_ok());
    }
}
