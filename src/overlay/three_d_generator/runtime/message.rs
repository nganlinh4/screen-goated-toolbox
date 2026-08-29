use serde_json::{Value, json};

use super::RuntimeOperation;

pub(super) fn sources_unchanged(operation: &RuntimeOperation) -> bool {
    match operation {
        RuntimeOperation::Generate { request, .. } => {
            request.source_descriptors.len() == 1
                && request.source_descriptors[0].path == request.image_path
                && crate::overlay::creation_source_snapshot::validate_sources(
                    &request.source_descriptors,
                )
                .is_ok()
        }
        RuntimeOperation::Segment { continuation } | RuntimeOperation::Refine { continuation } => {
            continuation.source_descriptor.path == continuation.image_path
                && crate::overlay::creation_source_snapshot::validate_sources(std::slice::from_ref(
                    &continuation.source_descriptor,
                ))
                .is_ok()
        }
    }
}

pub(super) fn operation_name(operation: &RuntimeOperation) -> &'static str {
    match operation {
        RuntimeOperation::Generate { .. } => "generate",
        RuntimeOperation::Segment { .. } => "segment",
        RuntimeOperation::Refine { .. } => "refine",
    }
}

pub(super) fn request_value(operation: &RuntimeOperation) -> Value {
    match operation {
        RuntimeOperation::Generate { request, .. } => {
            let mut value = serde_json::to_value(request).unwrap_or(Value::Null);
            if let Some(arguments) = value.as_object_mut() {
                arguments.remove("finalOutputDir");
            }
            value
        }
        RuntimeOperation::Segment { continuation } => json!({
            "parentDispatchId": &continuation.parent_dispatch_id,
            "outputPath": &continuation.previous_output_path,
            "outputName": &continuation.output_name,
            "imagePath": &continuation.image_path,
            "sourceDescriptors": [&continuation.source_descriptor],
            "outputDir": &continuation.staging_dir,
            "previousOutputPath": &continuation.previous_output_path,
            "generationMode": continuation.generation_mode,
            "polycount": continuation.polycount,
            "autoSegment": continuation.auto_segment,
            "instruction": &continuation.instruction,
        }),
        RuntimeOperation::Refine { continuation } => {
            let refinement = continuation
                .refinement
                .as_ref()
                .expect("refinement operation has settings");
            json!({
                "parentDispatchId": &continuation.parent_dispatch_id,
                "outputPath": &continuation.previous_output_path,
                "outputName": &continuation.output_name,
                "imagePath": &continuation.image_path,
                "sourceDescriptors": [&continuation.source_descriptor],
                "outputDir": &continuation.staging_dir,
                "previousOutputPath": &continuation.previous_output_path,
                "generationMode": continuation.generation_mode,
                "polycount": continuation.polycount,
                "autoSegment": continuation.auto_segment,
                "instruction": &continuation.instruction,
                "kind": refinement.kind,
                "segmentationLevel": &refinement.segmentation_level,
                "topology": &refinement.topology,
                "faceLimit": refinement.face_limit,
                "animation": &refinement.animation,
            })
        }
    }
}

pub(super) fn fresh_message(
    job_id: &str,
    operation: &RuntimeOperation,
    fingerprint: &str,
) -> Value {
    let mut args = request_value(operation);
    if let Some(args) = args.as_object_mut() {
        args.insert("dispatchId".to_string(), json!(operation.dispatch_id()));
        args.insert("requestFingerprint".to_string(), json!(fingerprint));
    }
    json!({
        "id": job_id,
        "cmd": match operation {
            RuntimeOperation::Generate { .. } => "start_job",
            RuntimeOperation::Segment { .. } => "segment_job",
            RuntimeOperation::Refine { .. } => "refine_model",
        },
        "args": args,
    })
}
