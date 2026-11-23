use super::*;
use serde_json::json;

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../../../parity-fixtures/image-to-3d/refinement-settings.json"
    ))
    .unwrap()
}

#[test]
fn accepted_fast_request_recovers_without_changing_its_fingerprint() {
    let name = format!(
        "{}::accepted_fast_request_recovers_without_changing_its_fingerprint",
        module_path!().split_once("::").unwrap().1
    );
    crate::overlay::creation_submission_test::run_isolated(&name, |root| {
        let source = root.join("source.png");
        image::RgbImage::new(64, 64).save(&source).unwrap();
        let inspected = crate::overlay::creation_source::inspect_image(&source).unwrap();
        let dispatch = "saved-fast-dispatch";
        let snapshot =
            crate::overlay::creation_source_snapshot::prepare("3d", dispatch, &[inspected])
                .unwrap();
        let output = root.join("output");
        std::fs::create_dir(&output).unwrap();
        let output = std::fs::canonicalize(output).unwrap();
        let output_name = crate::overlay::creation_output::assigned_name(
            source.to_str().unwrap(),
            dispatch,
            None,
            "glb",
        )
        .unwrap();
        let staging =
            crate::overlay::creation_output::prepare_staging(dispatch, &output_name).unwrap();
        let request: StartJobRequest = serde_json::from_value(json!({
            "imagePath": snapshot.paths()[0], "sourceDescriptors":snapshot.descriptors(),
            "outputDir":staging.directory(), "finalOutputDir":output, "outputName":output_name,
            "generationMode":"fast", "mode":"fast", "polycount":100,
            "autoSegment":false, "segmentationLevel":"balanced", "segmentationMode":"none", "outputFormat":"glb"
        })).unwrap();
        let arguments = serde_json::to_value(&request).unwrap();
        let recorded = crate::overlay::creation_intent_journal::record(
            "3d",
            "saved-job",
            dispatch,
            arguments.clone(),
        );
        let error = match recorded {
            Err(error) => error,
            Ok(_) => panic!("uncertain publication expected"),
        };
        assert!(error.is_uncertain());
        snapshot.persist();
        staging.persist();
        let intent = crate::overlay::creation_intent_journal::load("3d")
            .unwrap()
            .remove(0);
        let fingerprint = intent.arguments_fingerprint.clone();
        let (operation, status) = restore_generation(&intent).unwrap();
        assert_eq!(status.generation_mode, Some(GenerationMode::Fast));
        assert_eq!(status.polycount, Some(100));
        let RuntimeOperation::Generate {
            request: restored, ..
        } = operation
        else {
            panic!("generation expected")
        };
        assert_eq!(serde_json::to_value(restored).unwrap(), arguments);
        assert!(
            super::super::submission::start_job(request)
                .unwrap_err()
                .contains("Quality")
        );
        let unchanged = crate::overlay::creation_intent_journal::load("3d")
            .unwrap()
            .remove(0);
        assert_eq!(unchanged.arguments_fingerprint, fingerprint);
        assert_eq!(unchanged.arguments, arguments);
    });
}

#[test]
fn generation_level_is_independent_of_the_refinement_option() {
    for case in fixture()["cases"].as_array().unwrap() {
        let arguments = json!({
            "generationSegmentationLevel": case["generationLevel"],
            "segmentationLevel": case["requestedLevel"],
            "kind": case["refinementKind"],
        });
        assert_eq!(
            restored_segmentation_level(&arguments).unwrap(),
            case["generationLevel"].as_str().unwrap()
        );
    }
}

#[test]
fn legacy_requests_keep_their_recorded_level_or_documented_default() {
    assert_eq!(
        restored_segmentation_level(&json!({})).unwrap(),
        fixture()["legacyDefault"]
    );
    assert_eq!(
        restored_segmentation_level(&json!({"segmentationLevel":null})).unwrap(),
        "detailed"
    );
    for level in ["simple", "balanced", "detailed"] {
        assert_eq!(
            restored_segmentation_level(&json!({"segmentationLevel":level})).unwrap(),
            level
        );
    }
}

#[test]
fn malformed_explicit_generation_levels_are_not_silently_defaulted() {
    for value in fixture()["invalidLevels"].as_array().unwrap() {
        assert!(
            restored_segmentation_level(
                &json!({"generationSegmentationLevel":value,"segmentationLevel":"balanced"})
            )
            .is_err()
        );
    }
}

#[test]
fn pending_refinement_round_trips_its_generation_settings_through_real_storage() {
    let name = format!(
        "{}::pending_refinement_round_trips_its_generation_settings_through_real_storage",
        module_path!().split_once("::").unwrap().1
    );
    crate::overlay::creation_submission_test::run_isolated(&name, |root| {
        super::super::tests::prepare_refinement(root, "balanced");
        let error = super::super::submission::start_refinement(RefineRequest {
            continuation_id: "parent-id".into(),
            kind: super::super::RefinementKind::AddMaterials,
            segmentation_level: None,
            topology: None,
            face_limit: None,
            animation: None,
        })
        .unwrap_err();
        assert!(error.contains("uncertain"), "{error}");
        let intents = crate::overlay::creation_intent_journal::load("3d").unwrap();
        assert_eq!(intents.len(), 1);
        assert_eq!(
            intents[0].arguments["generationSegmentationLevel"],
            "balanced"
        );
        assert!(intents[0].arguments["segmentationLevel"].is_null());
        let (operation, status) = restore_segment(&intents[0]).unwrap();
        assert_eq!(status.segmentation_level.as_deref(), Some("balanced"));
        let RuntimeOperation::Refine { continuation } = &operation else {
            panic!("refinement was not restored")
        };
        assert_eq!(continuation.segmentation_level, "balanced");
        let message = super::super::message::request_value(&operation);
        assert_eq!(message["kind"], "add_materials");
        assert!(message["segmentationLevel"].is_null());
    });
}
