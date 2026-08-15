use super::*;

#[test]
fn child_environment_forwards_only_explicit_provider_credentials() {
    let mut command = ProcessCommand::new("unused.exe");
    command.env_clear();
    let mut queried = Vec::new();
    forward_provider_credentials(&mut command, |name| {
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
fn recorder_holds_deferred_capability_for_the_worker_lifetime() {
    let source = include_str!("../host_launcher.rs");
    assert!(source.contains("DeferredFfmpeg::prepare"));
    assert!(source.contains("_deferred_ffmpeg: deferred_ffmpeg"));
    assert!(source.contains("_external_capabilities: external_capabilities"));
}

#[test]
fn recorder_worker_owns_a_distinct_webview_profile() {
    assert_eq!(RECORDER_WEBVIEW_PROFILE, "screen-recorder-worker");
    assert_ne!(RECORDER_WEBVIEW_PROFILE, "common");
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
fn recorder_job_contains_worker_but_allows_webview_processes() {
    let flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK;
    assert!(flags.contains(JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE));
    assert!(flags.contains(JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK));
}

#[test]
fn recorder_removal_terminates_and_reaps_its_active_worker_job() {
    let mut child = ProcessCommand::new("cmd")
        .args(["/C", "ping -n 30 127.0.0.1 >NUL"])
        .spawn()
        .unwrap();
    let job = create_job(&child).unwrap();
    let _active_job = session_lifecycle::ActiveJobRegistration::register(&job).unwrap();

    session_lifecycle::cancel_active_work();
    child.wait().unwrap();

    assert!(child.try_wait().unwrap().is_some());
}
