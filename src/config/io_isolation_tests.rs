use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{get_config_path, load_config};
use crate::config::Config;

const CHILD_ENV: &str = "SGT_CONFIG_ISOLATION_TEST_CHILD";
const EXPECTED_KEY_ENV: &str = "SGT_CONFIG_ISOLATION_TEST_EXPECTED_KEY";
const WRITE_KEY_ENV: &str = "SGT_CONFIG_ISOLATION_TEST_WRITE_KEY";

struct TestRoot(PathBuf);

impl TestRoot {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "sgt-config-isolation-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        Self(root)
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        let root = std::fs::canonicalize(&self.0).unwrap();
        let temp = std::fs::canonicalize(std::env::temp_dir()).unwrap();
        assert!(root.starts_with(&temp) && root != temp);
        std::fs::remove_dir_all(root).unwrap();
    }
}

fn spawn_probe(root: &Path, expected_key: Option<&str>, write_key: Option<&str>) -> Child {
    let module = module_path!().split_once("::").unwrap().1;
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .arg("--exact")
        .arg(format!("{module}::isolated_config_child"))
        .arg("--nocapture")
        .env(crate::paths::RUNTIME_STATE_ROOT_ENV, root)
        .env(CHILD_ENV, "1")
        .env("GEMINI_API_KEY", "synthetic-inherited-key")
        .env("GEMINI_API_KEY_7", "synthetic-inherited-key")
        .env("GITHUB_TOKEN", "synthetic-inherited-token")
        .env("AWS_SECRET_ACCESS_KEY", "synthetic-inherited-secret")
        .env("CUSTOM_PROVIDER_KEY", "synthetic-inherited-key")
        .env("SGT_RECORDER_LAUNCH_TOKEN", "synthetic-launch-token")
        .env_remove(EXPECTED_KEY_ENV)
        .env_remove(WRITE_KEY_ENV)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(value) = expected_key {
        command.env(EXPECTED_KEY_ENV, value);
    }
    if let Some(value) = write_key {
        command.env(WRITE_KEY_ENV, value);
    }
    command.spawn().unwrap()
}

fn assert_success(child: Child) {
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "isolated config child failed: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn isolated_config_child() {
    if std::env::var_os(CHILD_ENV).is_none() {
        return;
    }
    let root = PathBuf::from(std::env::var_os(crate::paths::RUNTIME_STATE_ROOT_ENV).unwrap());
    let path = get_config_path();
    let local = root.join("local-app-data").join("screen-goated-toolbox");
    assert_eq!(crate::paths::app_runtime_local_data_dir(), local);
    assert_eq!(
        crate::paths::app_sgt_dir(),
        root.join("local-app-data").join("SGT")
    );
    #[cfg(not(feature = "recorder-worker"))]
    {
        let temporary = root.join("temp").join("screen-goated-toolbox");
        assert_eq!(crate::paths::child_process_temp_dir().unwrap(), temporary);
        let mut command = Command::new("unused.exe");
        crate::paths::configure_isolated_child_environment(&mut command).unwrap();
        for variable in ["TEMP", "TMP"] {
            assert_eq!(
                command
                    .get_envs()
                    .find(|(name, _)| *name == variable)
                    .and_then(|(_, value)| value),
                Some(temporary.as_os_str())
            );
        }
        for variable in [
            "GEMINI_API_KEY",
            "GEMINI_API_KEY_7",
            "GITHUB_TOKEN",
            "AWS_SECRET_ACCESS_KEY",
        ] {
            assert!(
                command
                    .get_envs()
                    .any(|(name, value)| name == variable && value.is_none())
            );
        }
    }
    assert_eq!(
        crate::api::provider_credentials::resolve("GEMINI_API_KEY", "synthetic-saved-key"),
        "synthetic-saved-key"
    );
    assert_eq!(
        crate::api::provider_credentials::resolve("GEMINI_API_KEY", ""),
        ""
    );
    let expected_key = std::env::var(EXPECTED_KEY_ENV).ok();
    let write_key = std::env::var(WRITE_KEY_ENV).ok();
    crate::paths::isolate_inherited_credentials(&["SGT_RECORDER_LAUNCH_TOKEN"]);
    for variable in [
        "GEMINI_API_KEY",
        "GEMINI_API_KEY_7",
        "GITHUB_TOKEN",
        "AWS_SECRET_ACCESS_KEY",
        "CUSTOM_PROVIDER_KEY",
    ] {
        assert!(std::env::var_os(variable).is_none());
    }
    assert_eq!(
        std::env::var("SGT_RECORDER_LAUNCH_TOKEN").unwrap(),
        "synthetic-launch-token"
    );
    assert!(std::env::var_os("PATH").is_some());
    let mut child = Command::new("cmd.exe");
    child.args([
        "/C",
        "if defined GEMINI_API_KEY (exit /b 1) else (exit /b 0)",
    ]);
    assert!(child.status().unwrap().success());
    let mut explicit = Command::new("cmd.exe");
    explicit
        .args([
            "/C",
            "if defined GEMINI_API_KEY (exit /b 0) else (exit /b 1)",
        ])
        .env("GEMINI_API_KEY", "synthetic-explicit-key");
    assert!(explicit.status().unwrap().success());
    assert_eq!(
        path,
        root.join("roaming-state")
            .join("screen-goated-toolbox")
            .join("config_v3.json")
    );
    assert_ne!(path, crate::paths::app_config_dir().join("config_v3.json"));
    let config = load_config();
    if let Some(expected) = expected_key {
        assert_eq!(config.api_key, expected);
    }
    #[cfg(not(feature = "recorder-worker"))]
    if let Some(value) = write_key {
        let mut config = config;
        for _ in 0..10 {
            config.api_key.clone_from(&value);
            super::try_save_config(&config).unwrap();
        }
    }
    #[cfg(feature = "recorder-worker")]
    assert!(write_key.is_none());
}

#[test]
fn isolated_host_and_worker_use_only_their_selected_config() {
    let root = TestRoot::new();
    let first = root.0.join("first");
    let second = root.0.join("second");
    let isolated_config = first
        .join("roaming-state")
        .join("screen-goated-toolbox")
        .join("config_v3.json");
    std::fs::create_dir_all(isolated_config.parent().unwrap()).unwrap();
    let seed = Config {
        api_key: "synthetic-isolated-key".to_string(),
        ..Config::default()
    };
    std::fs::write(&isolated_config, serde_json::to_vec(&seed).unwrap()).unwrap();
    let production = root.0.join("production-config.json");
    let production_bytes = b"synthetic-production-sentinel";
    std::fs::write(&production, production_bytes).unwrap();

    let first_reader = spawn_probe(&first, Some("synthetic-isolated-key"), None);
    let second_reader = spawn_probe(&second, Some(""), None);
    assert_success(first_reader);
    assert_success(second_reader);

    #[cfg(not(feature = "recorder-worker"))]
    {
        let first_a = spawn_probe(&first, None, Some("writer-a"));
        let first_b = spawn_probe(&first, None, Some("writer-b"));
        let second_writer = spawn_probe(&second, None, Some("writer-c"));
        assert_success(first_a);
        assert_success(first_b);
        assert_success(second_writer);
        // Keys are sealed at rest, so decode through the production loader.
        let saved = |path: &std::path::Path| {
            let bytes = std::fs::read(path).unwrap();
            assert!(!String::from_utf8_lossy(&bytes).contains("writer-"));
            super::load_config_with(path, &mut |_| Ok(bytes.clone()), Default::default()).config
        };
        let first_config = saved(&isolated_config);
        assert!(matches!(
            first_config.api_key.as_str(),
            "writer-a" | "writer-b"
        ));
        let second_path = second
            .join("roaming-state")
            .join("screen-goated-toolbox")
            .join("config_v3.json");
        assert_eq!(saved(&second_path).api_key, "writer-c");
    }
    assert_eq!(std::fs::read(production).unwrap(), production_bytes);
}
