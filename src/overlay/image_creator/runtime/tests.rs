use super::*;

const _: () = assert!(MAX_RETAINED_TERMINAL_JOBS >= MAX_QUEUED_JOBS);

fn request(image_paths: Vec<String>) -> StartJobRequest {
    StartJobRequest {
        image_paths,
        image_path: None,
        source_descriptors: Vec::new(),
        output_dir: None,
        final_output_dir: None,
        prompt: "Create a calm landscape".to_string(),
        output_name: None,
        dispatch_id: String::new(),
    }
}

#[test]
fn image_creation_uses_two_jobs_and_exact_operation() {
    assert_eq!(MAX_PARALLEL_JOBS, 2);
    assert_eq!(OPERATION, "create_image");
}

#[test]
fn image_entry_and_reference_retention_contract_remains_fail_closed() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../parity-fixtures/image-creation-editing/state-contract.json"
    ))
    .unwrap();
    assert_eq!(fixture["fixtureVersion"].as_u64(), Some(65));

    let quality = &fixture["qualityControl"];
    assert_eq!(
        quality["semanticEditorReplacementBeforeCommitIsReacquired"].as_bool(),
        Some(true)
    );
    assert_eq!(
        quality["promptAcceptedByEditorStateBeforeSubmit"].as_bool(),
        Some(true)
    );

    let upload = &fixture["referenceUpload"];
    for field in [
        "asynchronousRetentionWaitIsBounded",
        "retentionWaitIsIndependentFromControlDiscovery",
        "visibleRetainedReferenceControlRequired",
        "selectedFileInputAloneIsInsufficient",
        "allRequestedReferencesRetainedBeforeSubmission",
        "duplicateReferenceNamesRequireRetainedMultiplicity",
        "aggregateRetentionWaitFitsWithinJobDeadline",
        "safeInformationalDialogsReconciledDuringRetentionWait",
    ] {
        assert_eq!(upload[field].as_bool(), Some(true), "{field}");
    }
}

#[test]
fn hidden_image_creation_rejects_admission_before_mutating_output() {
    let output =
        std::env::temp_dir().join(format!("sgt-hidden-image-admission-{}", std::process::id()));
    let mut request = request(Vec::new());
    request.output_dir = Some(output.to_string_lossy().to_string());

    let error = start_job(request).unwrap_err();

    assert_eq!(error, "Image creation is temporarily unavailable.");
    assert!(!output.exists());
}

#[test]
fn image_sessions_accept_zero_or_multiple_unique_references() {
    let root = std::env::temp_dir().join(format!(
        "sgt-image-references-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    let first = root.join("first.png");
    let second = root.join("second.jpg");
    image::RgbaImage::new(2, 2).save(&first).unwrap();
    image::RgbImage::new(3, 2).save(&second).unwrap();

    let mut empty = request(Vec::new());
    normalize_reference_paths(&mut empty).unwrap();
    assert!(empty.image_paths.is_empty());
    assert!(empty.image_path.is_none());

    let mut multiple = request(vec![
        first.to_string_lossy().to_string(),
        second.to_string_lossy().to_string(),
        first.to_string_lossy().to_string(),
    ]);
    normalize_reference_paths(&mut multiple).unwrap();
    assert_eq!(multiple.image_paths.len(), 3);
    assert_eq!(
        multiple.image_path.as_deref(),
        multiple.image_paths.first().map(String::as_str)
    );
    assert_eq!(multiple.image_paths[0], multiple.image_paths[2]);

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn explicit_reference_list_does_not_append_the_legacy_fallback() {
    let root = std::env::temp_dir().join(format!(
        "sgt-image-reference-precedence-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let first = root.join("first.png");
    let second = root.join("second.png");
    image::RgbaImage::new(2, 2).save(&first).unwrap();
    image::RgbaImage::new(2, 2).save(&second).unwrap();

    let mut request = request(vec![
        first.to_string_lossy().to_string(),
        second.to_string_lossy().to_string(),
    ]);
    request.image_path = Some(first.to_string_lossy().to_string());
    normalize_reference_paths(&mut request).unwrap();

    assert_eq!(request.image_paths.len(), 2);
    assert_ne!(request.image_paths[0], request.image_paths[1]);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn legacy_reference_remains_compatible_when_the_list_is_empty() {
    let root =
        std::env::temp_dir().join(format!("sgt-image-legacy-reference-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let image = root.join("legacy.png");
    image::RgbaImage::new(2, 2).save(&image).unwrap();

    let mut request = request(Vec::new());
    request.image_path = Some(image.to_string_lossy().to_string());
    normalize_reference_paths(&mut request).unwrap();

    assert_eq!(request.image_paths.len(), 1);
    assert_eq!(request.image_path, request.image_paths.first().cloned());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn image_session_rejects_files_that_are_not_supported_images() {
    let path = std::env::temp_dir().join(format!(
        "sgt-invalid-reference-{}-{}.png",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    std::fs::write(&path, b"not an image").unwrap();
    let mut request = request(vec![path.to_string_lossy().to_string()]);

    assert!(normalize_reference_paths(&mut request).is_err());

    std::fs::remove_file(path).unwrap();
}

#[test]
fn image_session_reference_limit_is_enforced_before_queueing() {
    let mut request = request(
        (0..=MAX_REFERENCE_IMAGES)
            .map(|index| format!("reference-{index}.png"))
            .collect(),
    );
    assert!(normalize_reference_paths(&mut request).is_err());
}
