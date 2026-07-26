use super::*;

fn request(image_paths: Vec<String>) -> StartJobRequest {
    StartJobRequest {
        image_paths,
        image_path: None,
        output_dir: None,
        prompt: "Create a calm landscape".to_string(),
        output_name: None,
    }
}

#[test]
fn image_creation_uses_two_jobs_and_exact_operation() {
    assert_eq!(MAX_PARALLEL_JOBS, 2);
    assert_eq!(OPERATION, "create_image_from_reference");
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
    let second = root.join("second.png");
    std::fs::write(&first, b"first").unwrap();
    std::fs::write(&second, b"second").unwrap();

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
    assert_eq!(multiple.image_paths.len(), 2);
    assert_eq!(
        multiple.image_path.as_deref(),
        multiple.image_paths.first().map(String::as_str)
    );

    std::fs::remove_dir_all(root).unwrap();
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
