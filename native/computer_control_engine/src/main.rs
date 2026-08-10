mod mcp;
mod provider_frame;
mod setup;
mod stdio;

use anyhow::{Context, Result, anyhow};
use sgt_computer_control_protocol::{Command, Output, ProviderEvent};

const TOKEN_ENV: &str = "SGT_CC_ENGINE_LAUNCH_TOKEN";

fn main() {
    if let Err(error) = run() {
        eprintln!("Computer Control engine failed: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let token = std::env::var(TOKEN_ENV).context("launch authentication is unavailable")?;
    // SAFETY: the engine is single-threaded and removes the inherited secret before
    // constructing any worker threads or processing untrusted input.
    unsafe { std::env::remove_var(TOKEN_ENV) };
    if !sgt_computer_control_protocol::valid_token(&token) {
        return Err(anyhow!("launch authentication is malformed"));
    }
    stdio::serve(&token, dispatch)
}

fn dispatch(command: Command) -> Result<Output> {
    Ok(match command {
        Command::Handshake { .. } => Output::Handshake {
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            architecture: std::env::consts::ARCH.to_string(),
        },
        Command::BuildSetup(request) => Output::Setup(setup::build(request)?),
        Command::NormalizeMcpCatalog(request) => Output::McpCatalog(mcp::normalize(request)?),
        Command::ParseProviderFrame { frame } => {
            let events: Vec<ProviderEvent> = provider_frame::parse(&frame);
            Output::ProviderEvents { events }
        }
        Command::Shutdown => Output::Shutdown,
    })
}
