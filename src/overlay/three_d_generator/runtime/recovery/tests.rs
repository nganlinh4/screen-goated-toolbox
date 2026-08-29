use super::super::failure::failure_output;
use super::super::message::{fresh_message, sources_unchanged};
use crate::overlay::three_d_generator::runtime::{
    Continuation, RuntimeOperation, StartJobRequest, generation_mode::GenerationMode,
};
use image::{ImageBuffer, Rgb};
use std::path::PathBuf;

#[test]
fn failed_segmentation_keeps_the_published_base_model_visible() {
    let operation = RuntimeOperation::Segment {
        continuation: Continuation {
            parent_dispatch_id: "parent".to_string(),
            dispatch_id: "parts".to_string(),
            image_path: "source.png".to_string(),
            source_descriptor: crate::overlay::creation_source::SourceDescriptor {
                path: "source.png".to_string(),
                size_bytes: 1,
                sha256: "0".repeat(64),
            },
            output_dir: PathBuf::from("output"),
            staging_dir: PathBuf::from("staging"),
            output_name: "model-parts.glb".to_string(),
            previous_output_path: PathBuf::from("output/base.glb"),
            generation_mode: GenerationMode::Quality,
            polycount: 5_000,
            auto_segment: true,
            instruction: None,
            project_id: "project".to_string(),
            supported_actions: Vec::new(),
            available_actions: Vec::new(),
            is_segmented: false,
            is_textured: false,
            is_pbr: false,
            is_rigged: false,
            rig_type: None,
            refinement: None,
            expires_at_ms: u64::MAX,
        },
    };

    assert_eq!(
        failure_output(&operation),
        (
            Some("output/base.glb".to_string()),
            Some("base.glb".to_string())
        )
    );
}

#[test]
fn fresh_dispatch_uses_immutable_snapshot_when_original_changes() {
    let root = std::env::temp_dir().join(format!(
        "sgt-3d-source-freeze-{}-{}",
        std::process::id(),
        super::now_ms()
    ));
    std::fs::create_dir(&root).unwrap();
    let image_path = root.join("source.png");
    ImageBuffer::from_pixel(1, 1, Rgb([1_u8, 2, 3]))
        .save(&image_path)
        .unwrap();
    let inspected = crate::overlay::creation_source::inspect_image(&image_path).unwrap();
    let snapshot =
        crate::overlay::creation_source_snapshot::prepare("3d", "dispatch-1", &[inspected])
            .unwrap();
    let descriptor = snapshot.descriptors()[0].clone();
    snapshot.persist();
    let operation = RuntimeOperation::Generate {
        request: StartJobRequest {
            image_path: descriptor.path.clone(),
            source_descriptors: vec![descriptor.clone()],
            output_dir: Some(root.to_string_lossy().to_string()),
            final_output_dir: Some(root.to_string_lossy().to_string()),
            polycount: 5_000,
            mode: "topology_mesh".to_string(),
            generation_mode: GenerationMode::Quality,
            output_format: "glb_plain".to_string(),
            auto_segment: false,
            segmentation_mode: "none".to_string(),
            instruction: None,
            output_name: "result-dispatch-1.glb".to_string(),
            dispatch_id: "dispatch-1".to_string(),
        },
        output_dir: root.clone(),
        final_output_dir: root.clone(),
    };
    assert!(sources_unchanged(&operation));
    ImageBuffer::from_pixel(2, 1, Rgb([4_u8, 5, 6]))
        .save(&image_path)
        .unwrap();
    assert!(sources_unchanged(&operation));
    let message = fresh_message("job-1", &operation, "fingerprint-1");
    assert_eq!(
        message["args"]["sourceDescriptors"][0]["sha256"],
        descriptor.sha256
    );
    assert_eq!(message["args"]["requestFingerprint"], "fingerprint-1");
    crate::overlay::creation_source_snapshot::release_for_cleared_intent(
        &crate::overlay::creation_intent_journal::Intent {
            product: "3d".to_string(),
            job_id: "job-1".to_string(),
            dispatch_id: "dispatch-1".to_string(),
            created_at_ms: 1,
            accepted_at_ms: 1,
            deadline_at_ms: 1 + crate::overlay::creation_process_supervisor::MAX_WALL_TIME_MS,
            expires_at_ms: 2,
            arguments: serde_json::json!({"sourceDescriptors": [descriptor]}),
            arguments_fingerprint: "0".repeat(64),
        },
    );
    std::fs::remove_dir_all(root).unwrap();
}
