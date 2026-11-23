use super::*;

#[test]
fn outdated_worker_cannot_open_before_settings_protocol_negotiation() {
    assert!(!supports_settings_protocol(
        &serde_json::json!({"status": "ready"})
    ));
    assert!(!supports_settings_protocol(
        &serde_json::json!({"settingsProtocol": 0})
    ));
    assert!(supports_settings_protocol(
        &serde_json::json!({"settingsProtocol": sgt_recorder_protocol::SETTINGS_PROTOCOL_VERSION})
    ));
}

#[test]
fn child_environment_forwards_only_explicit_provider_credentials() {
    let mut command = ProcessCommand::new("unused.exe");
    command.env_clear();
    let mut queried = Vec::new();
    forward_provider_credentials_for_runtime(&mut command, false, |name| {
        queried.push(name.to_string());
        Some(OsString::from(format!("secret-for-{name}")))
    });

    assert_eq!(queried, PROVIDER_CREDENTIAL_ENV);
    let forwarded = command
        .get_envs()
        .filter_map(|(name, value)| value.map(|_| name.to_string_lossy().into_owned()))
        .collect::<std::collections::BTreeSet<_>>();
    let expected = PROVIDER_CREDENTIAL_ENV
        .into_iter()
        .map(str::to_string)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(forwarded, expected);
    assert!(!forwarded.iter().any(|name| name == "AWS_SECRET_ACCESS_KEY"));
}

#[test]
fn isolated_worker_never_reads_or_forwards_inherited_provider_credentials() {
    let mut command = ProcessCommand::new("unused.exe");
    command.env_clear();
    forward_provider_credentials_for_runtime(&mut command, true, |_| {
        panic!("isolated workers must not read inherited provider credentials")
    });
    assert!(command.get_envs().next().is_none());
}

#[test]
fn child_environment_forwards_only_valid_webview_runtime_roots() {
    let mut command = ProcessCommand::new("unused.exe");
    command.env_clear();
    let program_files = required_directory_env("ProgramFiles").unwrap();
    forward_webview_runtime_roots(&mut command, |name| match name {
        "ProgramFiles" => Some(program_files.clone().into_os_string()),
        "ProgramFiles(x86)" => Some(OsString::from("relative")),
        _ => None,
    });

    let forwarded = command
        .get_envs()
        .filter_map(|(name, value)| value.map(|_| name.to_string_lossy().into_owned()))
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        forwarded,
        std::collections::BTreeSet::from(["ProgramFiles".to_string()])
    );
}

#[test]
fn worker_inherits_the_isolated_runtime_root_only_when_selected() {
    let root = std::env::temp_dir().join("sgt-isolated-config-test");
    let mut isolated = ProcessCommand::new("unused.exe");
    isolated.env_clear();
    forward_runtime_state_root(&mut isolated, |_| Some(root.clone().into_os_string()));
    assert_eq!(
        isolated
            .get_envs()
            .find(|(name, _)| *name == crate::paths::RUNTIME_STATE_ROOT_ENV)
            .and_then(|(_, value)| value),
        Some(root.as_os_str())
    );
    let mut normal = ProcessCommand::new("unused.exe");
    normal.env_clear();
    forward_runtime_state_root(&mut normal, |_| None);
    assert!(
        normal
            .get_envs()
            .all(|(name, _)| name != crate::paths::RUNTIME_STATE_ROOT_ENV)
    );
}

#[cfg(debug_assertions)]
#[test]
fn cursor_source_forwarding_requires_a_canonical_explicit_repo_root() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .unwrap();
    let mut command = ProcessCommand::new("unused.exe");
    command.env_clear();
    forward_debug_cursor_source(&mut command, Some(repo.clone().into_os_string()));
    assert_eq!(
        command
            .get_envs()
            .find(|(name, _)| *name == "SGT_RECORDER_DEV_ASSET_ROOT")
            .and_then(|(_, value)| value),
        Some(repo.as_os_str())
    );
    for value in [
        None,
        Some(OsString::from("relative")),
        Some(std::env::temp_dir().into_os_string()),
    ] {
        let mut command = ProcessCommand::new("unused.exe");
        command.env_clear();
        forward_debug_cursor_source(&mut command, value);
        assert!(
            command
                .get_envs()
                .all(|(name, _)| name != "SGT_RECORDER_DEV_ASSET_ROOT")
        );
    }
}

#[test]
fn worker_workspace_is_outside_the_component_registry() {
    let runtime_root = std::fs::canonicalize(crate::paths::app_runtime_local_data_dir()).unwrap();
    let workspace = recorder_worker_workspace().unwrap();
    assert!(workspace.starts_with(runtime_root.join("worker-workspaces")));
    assert!(!workspace.starts_with(crate::component_registry::components_root()));
}

#[test]
fn response_reader_accepts_exact_frame_limit() {
    let mut exact = vec![b'x'; MAX_RESPONSE_LINE_BYTES - 1];
    exact.push(b'\n');
    assert_eq!(
        read_bounded_line(&mut std::io::Cursor::new(exact), MAX_RESPONSE_LINE_BYTES)
            .unwrap()
            .unwrap()
            .len(),
        MAX_RESPONSE_LINE_BYTES - 1
    );
}

#[test]
fn response_reader_rejects_oversize_and_incomplete_frames() {
    let mut oversized = vec![b'x'; MAX_RESPONSE_LINE_BYTES];
    oversized.push(b'\n');
    assert!(
        read_bounded_line(
            &mut std::io::Cursor::new(oversized),
            MAX_RESPONSE_LINE_BYTES
        )
        .is_err()
    );
    assert!(read_bounded_line(&mut std::io::Cursor::new(b"partial"), 32).is_err());
}

#[test]
fn recorder_does_not_resolve_optional_tools_during_open() {
    assert!(crate::component_registry::capabilities::RECORDER_REQUIRED_EXTERNAL_TOOLS.is_empty());
}

#[test]
fn feature_invocation_installs_before_spawning_and_continues_automatically() {
    let source = include_str!("../host_launcher.rs");
    let launch = source.find("fn launch(").unwrap();
    let install = source[launch..]
        .find("ensure_ready_with_badge(cancelled)?")
        .unwrap();
    let spawn = source[launch..].find("ProcessCommand::new").unwrap();
    assert!(install < spawn);
    assert!(!source[launch..launch + spawn].contains("downloaded tools"));
}

#[test]
fn consolidated_recorder_retires_legacy_packages_after_launch() {
    let source = include_str!("../../../component_registry/recorder.rs");
    assert!(source.contains("retire_legacy_split_installations();"));
    assert!(source.contains("for id in [BUNDLE_ID, WORKER_ID, WEB_ID]"));
}

#[test]
fn deferred_ffmpeg_resolution_is_visible_and_clears_before_capture() {
    let source = include_str!("deferred_capability.rs");
    assert!(source.contains("resolve_external_tool_with_badge_before_capture"));
    assert!(!source.contains("capabilities::resolve_external_tool("));
}

#[test]
fn recorder_worker_owns_a_distinct_webview_profile() {
    let profile = crate::overlay::webview_runtime::Profile::Recorder.directory();
    assert_eq!(profile, "screen-recorder-worker");
    assert_ne!(profile, "common");
}

#[test]
fn recorder_smoke_environment_is_validated_before_forwarding() {
    let isolated = std::env::temp_dir().join("sgt-recorder-smoke-profile");
    assert_eq!(
        recorder_webview_data_dir(Some(isolated.clone().into_os_string())).unwrap(),
        isolated
    );
    assert_eq!(
        recorder_debug_port(Some(OsString::from("49333"))).as_deref(),
        Some("49333")
    );
    for invalid in ["0", "65536", "-1", "not-a-port"] {
        assert!(recorder_debug_port(Some(OsString::from(invalid))).is_none());
    }
}

#[test]
fn an_optional_audio_decode_never_downloads_the_recorder() {
    let decode = Command::DecodeAudio {
        input_path: "C:/input.opus".to_string(),
        output_path: "C:/output.wav".to_string(),
    };
    assert_eq!(
        ComponentDelivery::for_headless(&decode),
        ComponentDelivery::InstalledOnly
    );
    let narration = Command::GtNarrationTest {
        input_wav: "C:/input.wav".to_string(),
        target_language: "en".to_string(),
    };
    assert_eq!(
        ComponentDelivery::for_headless(&narration),
        ComponentDelivery::InstallIfMissing
    );

    // Drive the real launch only where the recorder is absent (the isolated
    // test state root), so the test can never start a real worker.
    if crate::component_registry::recorder::is_installed() {
        eprintln!("recorder is installed here; skipping the launch half");
        return;
    }
    let error = launch(
        &AtomicBool::new(false),
        None,
        session_lifecycle::JobSlot::Headless,
        ComponentDelivery::InstalledOnly,
    )
    .err()
    .expect("an optional launch without the recorder must fail");
    assert!(
        error
            .downcast_ref::<OptionalComponentUnavailable>()
            .is_some(),
        "{error:#}"
    );
}
