use super::*;
use std::sync::atomic::AtomicBool;

#[test]
fn quality_input_fixture_uses_actual_source_inspection() {
    let name = format!(
        "{}::quality_input_fixture_uses_actual_source_inspection",
        module_path!().split_once("::").unwrap().1
    );
    crate::overlay::creation_submission_test::run_isolated(&name, |root| {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../parity-fixtures/image-to-3d/input-contract.json"
        ))
        .unwrap();
        for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
            let path = root.join(format!("input-{index}.png"));
            image::RgbImage::new(
                case["width"].as_u64().unwrap() as u32,
                case["height"].as_u64().unwrap() as u32,
            )
            .save(&path)
            .unwrap();
            let file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
            let length = case["bytes"].as_u64().unwrap();
            assert!(file.metadata().unwrap().len() <= length);
            file.set_len(length).unwrap();
            drop(file);
            let result = dispatch(
                "validate_image",
                &json!({"path":path,"generationMode":case["mode"]}),
                &AtomicBool::new(true),
            )
            .unwrap();
            assert_eq!(result["error"], case["error"]);
        }
    });
}

#[test]
fn validation_rejects_unreadable_sources_and_unsupported_modes() {
    let args = json!({"path":"", "generationMode":"quality"});
    assert_eq!(
        dispatch("validate_image", &args, &AtomicBool::new(true)).unwrap()["error"],
        "image_invalid"
    );
    assert!(
        dispatch(
            "validate_image",
            &json!({"path":"", "generationMode":"unsupported"}),
            &AtomicBool::new(true)
        )
        .is_err()
    );
}

#[test]
fn retired_commands_cannot_select_files_or_submit_legacy_jobs() {
    for cmd in [
        "pick_image",
        "pick_output_dir",
        "segment_model",
        "open_output",
    ] {
        assert!(dispatch_window(HWND::default(), cmd).is_none());
        assert!(dispatch(cmd, &Value::Null, &AtomicBool::new(true)).is_err());
    }
}

#[test]
fn rejected_requests_produce_diagnostics_without_echoing_the_body() {
    let oversized = format!(
        "{{\"cmd\":\"{}\"}}",
        "x".repeat(super::super::ipc_worker::MAX_REQUEST_BYTES)
    );
    let diagnostic = parse_request(&oversized).err().unwrap();
    assert!(diagnostic.contains("limit"), "{diagnostic}");
    assert!(!diagnostic.contains("xxxx"), "{diagnostic}");
    let diagnostic = parse_request("{\"id\":\"1\"").err().unwrap();
    assert!(diagnostic.starts_with("[3D Generator] Invalid request:"));
    let envelope = parse_request(r#"{"id":"7","cmd":"job_statuses"}"#).unwrap();
    assert_eq!(
        (envelope.id.as_str(), envelope.cmd.as_str()),
        ("7", "job_statuses")
    );
}

#[test]
fn window_command_failures_are_logged_because_the_reply_hides_them() {
    let failed = Err("Selected image is unreadable".into());
    assert!(!reply_script("7", Err("Selected image is unreadable".into())).contains("unreadable"));
    assert_eq!(
        failure_diagnostic("pick_images", &failed).as_deref(),
        Some("[3D Generator] pick_images failed: Selected image is unreadable")
    );
    assert_eq!(failure_diagnostic("pick_images", &Ok(Value::Null)), None);
}
