use super::*;
use std::io::{BufRead as _, Write as _};
use std::path::PathBuf;
use std::sync::mpsc;

const FIXTURE: &str = "overlay::computer_control::mcp::client::native_tests::server_fixture";

struct InheritedChild(std::process::Child);
impl Drop for InheritedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub(in crate::overlay::computer_control::mcp) struct Fixture {
    root: PathBuf,
}
impl Fixture {
    pub(in crate::overlay::computer_control::mcp) fn new() -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("sgt-mcp-native-{}-{unique}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        Self { root }
    }
    fn ready(&self) -> PathBuf {
        self.root.join("descendant.ready")
    }
    pub(in crate::overlay::computer_control::mcp) fn launch(
        &self,
        mode: &'static str,
    ) -> LaunchSpec {
        let program = Box::leak(
            std::env::current_exe()
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned()
                .into_boxed_str(),
        ) as &'static str;
        LaunchSpec {
            program,
            args: &["--exact", FIXTURE, "--nocapture"],
            env: Box::leak(
                vec![
                    ("SGT_MCP_NATIVE_MODE", mode),
                    ("SGT_MCP_NATIVE_EXE", program),
                    (
                        "SGT_MCP_NATIVE_READY",
                        Box::leak(self.ready().to_str().unwrap().to_owned().into_boxed_str())
                            as &'static str,
                    ),
                ]
                .into_boxed_slice(),
            ),
        }
    }
    fn descendant(&self) -> u32 {
        std::fs::read_to_string(self.ready())
            .unwrap()
            .parse()
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        for file in [
            self.ready(),
            self.ready().with_extension("writing"),
            self.root.join("server bridge.cmd"),
        ] {
            if file.exists() {
                std::fs::remove_file(file).unwrap();
            }
        }
        std::fs::remove_dir(&self.root).unwrap();
    }
}

#[test]
fn server_fixture() {
    let Some(mode) = std::env::var_os("SGT_MCP_NATIVE_MODE") else {
        return;
    };
    let ready = PathBuf::from(std::env::var_os("SGT_MCP_NATIVE_READY").unwrap());
    if mode == "descendant" {
        std::fs::write(
            ready.with_extension("writing"),
            std::process::id().to_string(),
        )
        .unwrap();
        std::fs::rename(ready.with_extension("writing"), &ready).unwrap();
        loop {
            std::thread::park();
        }
    }
    let mut command = crate::system_command::background_command(std::env::current_exe().unwrap());
    command
        .args(["--exact", FIXTURE, "--nocapture"])
        .env("SGT_MCP_NATIVE_MODE", "descendant")
        .env("SGT_MCP_NATIVE_READY", &ready)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::null());
    let _descendant = InheritedChild(command.spawn().unwrap()); // Inherits the containing job and output pipe.
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ready.exists() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    println!();
    let mut calls = Vec::<Value>::new();
    let mut cancelled = Vec::<Value>::new();
    let mut deferred = Vec::<Value>::new();
    for line in std::io::stdin().lock().lines() {
        let request: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let Some(id) = request.get("id") else {
            if request["method"] == "notifications/cancelled" {
                cancelled.push(request["params"]["requestId"].clone());
            }
            continue;
        };
        if request["method"] == "fixture/exit" {
            std::process::exit(0);
        }
        if request["method"] == "tools/call" {
            calls.push(id.clone());
            if mode == "deferred-call" {
                deferred.push(id.clone());
                continue;
            }
        }
        if request["method"] == "fixture/finish" {
            for id in deferred.drain(..) {
                println!(
                    "{}",
                    json!({"jsonrpc":"2.0","id":id,"result":{"late":true}})
                );
            }
        }
        let result = match request["method"].as_str().unwrap() {
            "initialize" => json!({"capabilities":{}}),
            "tools/list" => json!({"tools":[{"name":"echo","inputSchema":{"type":"object"}}]}),
            "fixture/status" => json!({"calls":calls,"cancelled":cancelled}),
            _ => request["params"].clone(),
        };
        println!("{}", json!({"jsonrpc":"2.0","id":id,"result":result}));
        std::io::stdout().flush().unwrap();
        if request["method"] == "fixture/block_input" {
            loop {
                std::thread::park();
            }
        }
    }
    std::process::exit(0);
}

pub(in crate::overlay::computer_control::mcp) fn wait_for_tree(client: &McpClient) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !client.child.stopped() {
        assert!(Instant::now() < deadline, "owned MCP tree did not retire");
        std::thread::sleep(Duration::from_millis(10));
    }
}

pub(in crate::overlay::computer_control::mcp) fn rpc(
    client: &McpClient,
    method: &str,
    params: Value,
) -> Value {
    client
        .request(method, params, Duration::from_secs(2))
        .unwrap()
}

pub(in crate::overlay::computer_control::mcp) fn pending_count(client: &McpClient) -> usize {
    client.pending.lock().len()
}

pub(in crate::overlay::computer_control::mcp) fn writing(client: &McpClient) -> bool {
    client.writer.is_active()
}

pub(in crate::overlay::computer_control::mcp) fn writer_finished(client: &McpClient) -> bool {
    client.writer.is_finished()
}

pub(in crate::overlay::computer_control::mcp) fn rpc_owned(
    client: &McpClient,
    method: &str,
    params: Value,
    stop: &Arc<AtomicBool>,
) -> Result<Value> {
    client.request_owned(method, params, Duration::from_secs(60), stop)
}

#[test]
fn shutdown_retires_the_server_and_its_inherited_descendant() {
    let fixture = Fixture::new();
    let (client, _) = McpClient::spawn_managed(&fixture.launch("stay"), None).unwrap();
    let job = client.child.control();
    assert!(job.member_ids().unwrap().contains(&fixture.descendant()));
    assert_eq!(client.list_tools().unwrap()[0].name, "echo");
    let params = json!({"text":"native Unicode \u{1f30f}","values":[1,2,3]});
    assert_eq!(
        client
            .request("fixture/echo", params.clone(), Duration::from_secs(2))
            .unwrap(),
        params
    );
    let start = Instant::now();
    client.shutdown();
    assert!(start.elapsed() < Duration::from_secs(2));
    wait_for_tree(&client);
    assert!(job.member_ids().unwrap().is_empty());
    assert!(!client.alive.load(Ordering::Acquire));
}

#[test]
fn parent_exit_disconnects_and_retires_a_descendant_holding_stdout() {
    let fixture = Fixture::new();
    let (client, events) = McpClient::spawn_managed(&fixture.launch("exit"), None).unwrap();
    let job = client.child.control();
    assert!(job.member_ids().unwrap().contains(&fixture.descendant()));
    let token = client.connection_token;
    let (tx, rx) = mpsc::channel();
    let observer =
        std::thread::spawn(move || tx.send((events.connection_token(), events.recv())).unwrap());
    assert!(
        client
            .request("fixture/exit", json!({}), Duration::from_secs(2))
            .is_err()
    );
    let (observed, batch) = rx.recv_timeout(Duration::from_secs(3)).unwrap();
    assert_eq!(observed, token);
    assert!(batch.unwrap().disconnected);
    observer.join().unwrap();
    wait_for_tree(&client);
    assert!(job.member_ids().unwrap().is_empty());
}

#[test]
fn cancelled_launch_never_starts_the_server() {
    let fixture = Fixture::new();
    let stop = AtomicBool::new(true);
    assert!(McpClient::spawn_managed(&fixture.launch("stay"), Some(&stop)).is_err());
    assert!(!fixture.ready().exists());
}

#[test]
fn batch_launch_preserves_standard_stdio_and_tree_containment() {
    let fixture = Fixture::new();
    let bridge = fixture.root.join("server bridge.cmd");
    std::fs::write(
        &bridge,
        format!("@echo off\r\n\"%SGT_MCP_NATIVE_EXE%\" --exact {FIXTURE} --nocapture\r\n"),
    )
    .unwrap();
    let mut launch = fixture.launch("stay");
    launch.program = Box::leak(bridge.to_str().unwrap().to_owned().into_boxed_str());
    let (client, _) = McpClient::spawn_managed(&launch, None).unwrap();
    let job = client.child.control();
    assert!(job.member_ids().unwrap().contains(&fixture.descendant()));
    assert_eq!(client.list_tools().unwrap()[0].name, "echo");
    client.shutdown();
    wait_for_tree(&client);
    assert!(job.member_ids().unwrap().is_empty());
}
