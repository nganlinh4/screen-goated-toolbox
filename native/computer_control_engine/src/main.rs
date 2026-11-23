mod mcp;
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
            let events: Vec<ProviderEvent> =
                sgt_computer_control_engine::parse_provider_frame(&frame);
            Output::ProviderEvents { events }
        }
        Command::Shutdown => Output::Shutdown,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // The host owns compatibility by pinning the engine version it receives;
    // `host_version` is informational and must never gate the handshake.
    #[test]
    fn handshake_reports_engine_identity_for_any_host_version() {
        for host_version in ["1.0.0", "0.0.0", "99.1.2-dev", ""] {
            let output = dispatch(Command::Handshake {
                host_version: host_version.to_string(),
            })
            .unwrap();
            let Output::Handshake {
                engine_version,
                architecture,
            } = output
            else {
                panic!("handshake must answer with the engine identity");
            };
            assert_eq!(engine_version, env!("CARGO_PKG_VERSION"));
            assert_eq!(architecture, std::env::consts::ARCH);
        }
    }
}
