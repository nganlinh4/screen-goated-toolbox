use std::path::PathBuf;

use image::{ImageBuffer, Rgb};

use super::*;

fn test_root() -> PathBuf {
    std::env::temp_dir().join(format!(
        "sgt-source-snapshot-test-{}-{}",
        std::process::id(),
        crate::overlay::creation_identity::random_id("case-").unwrap()
    ))
}

fn source(path: &Path, color: [u8; 3]) -> InspectedImage {
    ImageBuffer::from_pixel(8, 6, Rgb(color))
        .save(path)
        .unwrap();
    super::super::creation_source::inspect_image(path).unwrap()
}

fn dispatch() -> String {
    crate::overlay::creation_identity::random_id("snapshot-test-").unwrap()
}

#[test]
fn accepted_snapshot_survives_original_deletion_and_releases_exactly() {
    let root = test_root();
    std::fs::create_dir(&root).unwrap();
    let original = root.join("original.png");
    let inspected = source(&original, [12, 34, 56]);
    let original_path = inspected.path.to_string_lossy().to_string();
    let dispatch = dispatch();
    let assignment = prepare("svg", &dispatch, &[inspected]).unwrap();
    let descriptors = assignment.descriptors().to_vec();
    assignment.persist();

    std::fs::remove_file(&original).unwrap();
    validate_sources(&descriptors).unwrap();
    assert_eq!(original_paths(&descriptors).unwrap(), vec![original_path]);
    let preview = presentation_path(&descriptors).unwrap();
    assert_eq!(Path::new(&preview).extension().unwrap(), "png");

    release_intent(&descriptors, &dispatch).unwrap();
    assert!(validate_sources(&descriptors).is_err());
    let _ = std::fs::remove_file(preview);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn presentation_paths_preserve_every_reference_in_frozen_order() {
    let root = test_root();
    std::fs::create_dir(&root).unwrap();
    let first = source(&root.join("first.png"), [240, 10, 20]);
    let second = source(&root.join("second.png"), [10, 20, 240]);
    let dispatch = dispatch();
    let assignment = prepare("image", &dispatch, &[first, second]).unwrap();
    let descriptors = assignment.descriptors().to_vec();
    assignment.persist();

    let previews = presentation_paths(&descriptors).unwrap();
    assert_eq!(previews.len(), 2);
    assert_ne!(previews[0], previews[1]);
    let first_pixel = image::open(&previews[0])
        .unwrap()
        .to_rgb8()
        .get_pixel(0, 0)
        .0;
    let second_pixel = image::open(&previews[1])
        .unwrap()
        .to_rgb8()
        .get_pixel(0, 0)
        .0;
    assert_eq!(first_pixel, [240, 10, 20]);
    assert_eq!(second_pixel, [10, 20, 240]);
    assert_eq!(presentation_path(&descriptors).unwrap(), previews[0]);

    release_intent(&descriptors, &dispatch).unwrap();
    for preview in previews {
        let _ = std::fs::remove_file(preview);
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn continuation_owner_keeps_source_after_base_intent_and_releases_on_expiry_path() {
    let root = test_root();
    std::fs::create_dir(&root).unwrap();
    let inspected = source(&root.join("model.png"), [1, 2, 3]);
    let dispatch = dispatch();
    let continuation = crate::overlay::creation_identity::random_id("job-").unwrap();
    let assignment = prepare("3d", &dispatch, &[inspected]).unwrap();
    let descriptors = assignment.descriptors().to_vec();
    assignment.persist();

    retain_continuation(
        &descriptors,
        &dispatch,
        &continuation,
        now_ms().saturating_add(60_000),
    )
    .unwrap();
    release_intent(&descriptors, &dispatch).unwrap();
    validate_sources(&descriptors).unwrap();
    release_continuation(&descriptors, &continuation).unwrap();
    assert!(validate_sources(&descriptors).is_err());

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn changed_original_is_rejected_before_acceptance_without_leaking_a_snapshot() {
    let root = test_root();
    std::fs::create_dir(&root).unwrap();
    let original = root.join("changing.png");
    let inspected = source(&original, [1, 1, 1]);
    let dispatch = dispatch();
    source(&original, [2, 2, 2]);

    assert!(prepare("svg", &dispatch, &[inspected]).is_err());
    assert!(!snapshot_root().unwrap().join(dispatch).exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn corrupted_manifest_cannot_redirect_cleanup_outside_snapshot_root() {
    let root = test_root();
    std::fs::create_dir(&root).unwrap();
    let original = root.join("source.png");
    let external = root.join("must-survive.png");
    let inspected = source(&original, [9, 8, 7]);
    source(&external, [6, 5, 4]);
    let dispatch = dispatch();
    let assignment = prepare("image", &dispatch, &[inspected]).unwrap();
    assignment.persist();

    let snapshots = snapshot_root().unwrap();
    let directory = validate_snapshot_directory(&snapshots, &dispatch).unwrap();
    let mut manifest = read_manifest(&directory).unwrap();
    manifest.entries[0].descriptor.path = external.to_string_lossy().to_string();
    write_manifest(&directory, &manifest).unwrap();

    cleanup_snapshot(&dispatch).unwrap();
    assert!(external.is_file());
    assert!(!directory.exists());
    std::fs::remove_dir_all(root).unwrap();
}
