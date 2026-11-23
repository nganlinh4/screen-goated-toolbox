use super::*;
use serde_json::json;
use sgt_computer_control_protocol::{
    HostPrivilege, MAX_INTEGRATION_DECLARATIONS, MAX_MCP_TOOLS, McpCatalogRequest,
    McpIntegrationSnapshot, McpToolSnapshot, SetupRequest,
};

const FIXTURE: &str = "overlay::computer_control::engine::process::native_tests::server_fixture";
const READY: &[u8] = b"SGT_ENGINE_FIXTURE_READY\n";

fn fixture(mode: &str) -> EngineProcess {
    let token = launch_token().unwrap();
    let mut command = HelperCommand::new(std::env::current_exe().unwrap());
    command
        .arg("--exact")
        .arg(FIXTURE)
        .arg("--nocapture")
        .env("SGT_ENGINE_FIXTURE_MODE", mode)
        .env("SGT_ENGINE_FIXTURE_TOKEN", &token)
        .pipe_stdin()
        .stderr_null();
    let mut child = OwnedProcess::spawn_helper(&command, ()).unwrap();
    let stdin = child.take_stdin().unwrap();
    let mut stdout = child.take_stdout().unwrap();
    child.resume().unwrap();
    // Consume the owned libtest fixture's preface only; production framing stays strict.
    let (tx, rx) = mpsc::channel();
    let ready = std::thread::spawn(move || {
        let result = (|| -> Result<ProcessOutput> {
            let mut preface = Vec::new();
            for _ in 0..4096 {
                let mut byte = [0];
                std::io::Read::read_exact(&mut stdout, &mut byte)?;
                preface.push(byte[0]);
                if preface.ends_with(READY) {
                    return Ok(stdout);
                }
            }
            bail!("native protocol fixture did not become ready")
        })();
        let _ = tx.send(result);
    });
    let stdout = rx.recv_timeout(Duration::from_secs(8)).unwrap().unwrap();
    ready.join().unwrap();
    EngineProcess {
        tool_declarations: Default::default(),
        child,
        stdin,
        responses: start_response_reader(stdout),
        token,
        next_request_id: 1,
    }
}

#[test]
fn server_fixture() {
    let Ok(mode) = std::env::var("SGT_ENGINE_FIXTURE_MODE") else {
        return;
    };
    let token = std::env::var("SGT_ENGINE_FIXTURE_TOKEN").unwrap();
    let mut stdout = std::io::stdout().lock();
    std::io::Write::write_all(&mut stdout, READY).unwrap();
    std::io::Write::flush(&mut stdout).unwrap();
    let mut previous_id = 0;
    for line in std::io::BufRead::lines(std::io::stdin().lock()) {
        let request: Request = serde_json::from_str(&line.unwrap()).unwrap();
        request.validate(&token, previous_id).unwrap();
        previous_id = request.request_id;
        if mode == "closed-stream" {
            std::process::exit(0);
        }
        if mode == "silent" {
            continue;
        }
        if mode == "invalid-framing" {
            let line = format!("{RESPONSE_PREFIX}{{\n");
            std::io::Write::write_all(&mut stdout, line.as_bytes()).unwrap();
            std::io::Write::flush(&mut stdout).unwrap();
            continue;
        }
        let mut response = Response {
            protocol_version: PROTOCOL_VERSION,
            token: token.clone(),
            request_id: request.request_id,
            output: Some(Output::Handshake {
                engine_version: request.request_id.to_string(),
                architecture: std::env::consts::ARCH.into(),
            }),
            error: None,
        };
        let is_shutdown = matches!(request.command, Command::Shutdown);
        if is_shutdown {
            response.output = Some(Output::Shutdown);
        }
        match mode.as_str() {
            "invalid-authentication" => response.token = "00".repeat(TOKEN_BYTES),
            "unauthenticated-rejection" => {
                response.token = "00".repeat(TOKEN_BYTES);
                response.output = None;
                response.error = Some("request could not be processed".into());
            }
            "stale-response" => response.request_id = request.request_id.saturating_sub(1),
            "invalid-version" => response.protocol_version = PROTOCOL_VERSION + 1,
            "invalid-envelope" => response.error = Some("unexpected error".into()),
            "application-error" if request.request_id == 1 => {
                response.output = None;
                response.error = Some("request could not be processed".into());
            }
            _ => {}
        }
        let line = format!(
            "{RESPONSE_PREFIX}{}\n",
            serde_json::to_string(&response).unwrap()
        );
        std::io::Write::write_all(&mut stdout, line.as_bytes()).unwrap();
        std::io::Write::flush(&mut stdout).unwrap();
        if is_shutdown {
            return;
        }
    }
}

fn handshake() -> Command {
    Command::Handshake {
        host_version: "1.0.0".into(),
    }
}

fn assert_rejected_without_stopping(command: Command, expected: &str) {
    let mut process = fixture("valid");
    let error = process.request(command).unwrap_err();
    assert_eq!(
        DataRejection::classify(&error),
        Some(super::super::RejectionKind::Request)
    );
    assert!(error.to_string().contains(expected), "{error:#}");
    assert_eq!(
        process.next_request_id, 1,
        "unwritten request consumed an ID"
    );
    let Output::Handshake { engine_version, .. } = process.request(handshake()).unwrap() else {
        panic!("valid request did not reach the retained engine");
    };
    assert_eq!(engine_version, "1");
    assert!(process.child.try_wait().unwrap().is_none());
    process.child.stop_within(Duration::from_secs(2)).unwrap();
    assert!(process.child.stopped());
}

#[test]
fn rejected_aggregate_tool_count_leaves_the_engine_usable() {
    let tool = McpToolSnapshot {
        name: "operation".into(),
        description: "Capability".into(),
        input_schema: json!({"type":"object"}),
    };
    assert_rejected_without_stopping(
        Command::NormalizeMcpCatalog(McpCatalogRequest {
            integrations: vec![McpIntegrationSnapshot {
                id: "provider".into(),
                display_name: "Provider".into(),
                tools: vec![tool; MAX_MCP_TOOLS + 1],
            }],
        }),
        "too many Computer Control MCP tools",
    );
}

#[test]
fn rejected_aggregate_declaration_count_leaves_the_engine_usable() {
    assert_rejected_without_stopping(
        Command::BuildSetup(SetupRequest {
            privilege: HostPrivilege::Standard,
            voice_mode: true,
            search_enabled: true,
            integration_declarations: vec![json!({}); MAX_INTEGRATION_DECLARATIONS + 1],
        }),
        "too many Computer Control integration declarations",
    );
}

#[test]
fn rejected_aggregate_declaration_bytes_leave_the_engine_usable() {
    assert_rejected_without_stopping(
        Command::BuildSetup(SetupRequest {
            privilege: HostPrivilege::Standard,
            voice_mode: true,
            search_enabled: true,
            integration_declarations: vec![json!({"value":"x".repeat(96 * 1024)})],
        }),
        "integration declarations are too large",
    );
}

#[test]
fn rejected_encoded_frame_leaves_the_engine_usable_without_consuming_an_id() {
    let command = Command::ParseProviderFrame {
        frame: "\\".repeat(150 * 1024),
    };
    command.validate().unwrap();
    assert_rejected_without_stopping(command, "request exceeds protocol limit");
}

#[test]
fn exact_encoded_limit_accepts_utf8_and_escapes_after_rejecting_one_more_byte() {
    let mut process = fixture("valid");
    let request = |frame: String| Request {
        protocol_version: PROTOCOL_VERSION,
        token: process.token.clone(),
        request_id: 1,
        command: Command::ParseProviderFrame { frame },
    };
    let overhead = serde_json::to_vec(&request(String::new())).unwrap().len();
    let escapes = MAX_JSON_BYTES / 4;
    let remaining = MAX_JSON_BYTES - overhead - 2 * escapes;
    let frame = format!(
        "{}{}{}",
        "\\".repeat(escapes),
        "界".repeat(remaining / 3),
        "a".repeat(remaining % 3)
    );
    let exact = request(frame.clone());
    exact.command.validate().unwrap();
    assert_eq!(serde_json::to_vec(&exact).unwrap().len(), MAX_JSON_BYTES);
    let oversized = request(format!("{frame}a"));
    oversized.command.validate().unwrap();
    assert_eq!(
        serde_json::to_vec(&oversized).unwrap().len(),
        MAX_JSON_BYTES + 1
    );
    assert!(process.request(oversized.command).is_err());
    assert_eq!(process.next_request_id, 1);
    let Output::Handshake { engine_version, .. } = process.request(exact.command).unwrap() else {
        panic!("exactly bounded request did not reach the retained engine");
    };
    assert_eq!(engine_version, "1");
    let Output::Handshake { engine_version, .. } = process.request(handshake()).unwrap() else {
        panic!("following request was not processed");
    };
    assert_eq!(engine_version, "2");
    process.child.stop_within(Duration::from_secs(2)).unwrap();
    assert!(process.child.stopped());
}

#[test]
fn an_invalid_exchange_still_retires_the_owned_engine() {
    for mode in [
        "invalid-authentication",
        "stale-response",
        "unauthenticated-rejection",
        "invalid-version",
        "invalid-envelope",
        "invalid-framing",
        "closed-stream",
    ] {
        let mut process = fixture(mode);
        let error = process.request(handshake()).unwrap_err();
        assert_eq!(DataRejection::classify(&error), None);
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while !process.child.stopped() {
            assert!(
                std::time::Instant::now() < deadline,
                "{mode}: engine still alive"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(process.next_request_id, 2);
    }
}

#[test]
fn authenticated_application_rejection_keeps_the_engine_and_advances_the_id() {
    let mut process = fixture("application-error");
    let error = process.request(handshake()).unwrap_err();
    assert_eq!(
        DataRejection::classify(&error),
        Some(super::super::RejectionKind::Application)
    );
    assert_eq!(error.to_string(), "request could not be processed");
    assert_eq!(process.next_request_id, 2);
    let Output::Handshake { engine_version, .. } = process.request(handshake()).unwrap() else {
        panic!("application rejection stopped the next request");
    };
    assert_eq!(engine_version, "2");
    process.child.stop_within(Duration::from_secs(2)).unwrap();
    assert!(process.child.stopped());
}

#[test]
fn shutdown_asks_the_engine_to_exit_before_retiring_the_job() {
    let mut process = fixture("valid");
    process.request(handshake()).unwrap();
    assert!(
        process.shutdown(),
        "a responsive engine acknowledges Shutdown and exits on its own"
    );
    assert!(process.child.stopped());
}

#[test]
fn an_engine_that_ignores_shutdown_is_still_retired_promptly() {
    let mut process = fixture("silent");
    let started = std::time::Instant::now();
    assert!(!process.shutdown());
    process.child.stop_within(Duration::from_secs(2)).unwrap();
    assert!(process.child.stopped());
    assert!(started.elapsed() < Duration::from_secs(3));
}
