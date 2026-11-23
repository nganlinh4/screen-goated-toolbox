use super::*;
use serde_json::json;

#[test]
fn unavailable_destination_retains_delivery_allocations_without_an_intent() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../parity-fixtures/creation/destination-availability.json"
    ))
    .unwrap();
    let root = std::env::temp_dir()
        .join(crate::overlay::creation_identity::random_id("sgt-admission-retained-").unwrap());
    std::fs::create_dir(&root).unwrap();
    let result = (|| {
        for case in fixture["deliveryWithoutIntentCases"].as_array().unwrap() {
            let directory = root.join(if case["managed"].as_bool().unwrap() {
                "images"
            } else {
                "external"
            });
            let pending = crate::overlay::creation_delivery::PendingStorageReservation {
                dispatch_id: "dispatch".into(),
                output_path: directory.join("result.png").to_string_lossy().into_owned(),
                additional_output_bytes: case["expectedOutputReservationBytes"].as_u64().unwrap(),
            };
            let volume = volume_key(&root)?;
            let snapshot = reservation_snapshot_for(&[], &root, &volume, &[pending])?;
            assert_eq!(
                volume_bytes(&snapshot.pending_volume_bytes, &volume)?,
                case["expectedOutputReservationBytes"].as_u64().unwrap()
            );
            assert_eq!(
                snapshot.pending_managed_bytes,
                case["expectedManagedBytes"].as_u64().unwrap()
            );
        }
        Ok::<(), String>(())
    })();
    std::fs::remove_dir_all(&root).unwrap();
    result.unwrap();
}

#[test]
fn unavailable_destination_counts_a_matched_delivery_once() {
    let root = std::env::temp_dir()
        .join(crate::overlay::creation_identity::random_id("sgt-admission-matched-").unwrap());
    std::fs::create_dir(&root).unwrap();
    let result = (|| {
        let destination = root.join("images");
        let intent = crate::overlay::creation_intent_journal::Intent {
            product: "image".into(),
            job_id: "job".into(),
            dispatch_id: "dispatch".into(),
            created_at_ms: 1,
            accepted_at_ms: 1,
            deadline_at_ms: 2,
            expires_at_ms: 3,
            arguments: json!({"outputDir":root.join("staging"),"finalOutputDir":destination,"outputName":"result.png"}),
            arguments_fingerprint: "a".repeat(64),
        };
        let pending = crate::overlay::creation_delivery::PendingStorageReservation {
            dispatch_id: "dispatch".into(),
            output_path: destination
                .join("result.png")
                .to_string_lossy()
                .into_owned(),
            additional_output_bytes: 1024,
        };
        let volume = volume_key(&root)?;
        let snapshot = reservation_snapshot_for(&[intent], &root, &volume, &[pending])?;
        assert_eq!(volume_bytes(&snapshot.pending_volume_bytes, &volume)?, 1024);
        assert_eq!(snapshot.pending_managed_bytes, 1024);
        Ok::<(), String>(())
    })();
    std::fs::remove_dir_all(&root).unwrap();
    result.unwrap();
}

#[test]
fn unavailable_destination_keeps_pending_storage_allocations() {
    let root = std::env::temp_dir()
        .join(crate::overlay::creation_identity::random_id("sgt-admission-unavailable-").unwrap());
    std::fs::create_dir(&root).unwrap();
    let result = (|| {
        let staging = root.join("staging");
        std::fs::create_dir(&staging).unwrap();
        let destination = root.join("missing-destination");
        let intent = crate::overlay::creation_intent_journal::Intent {
            product: "image".to_string(),
            job_id: "job".to_string(),
            dispatch_id: "dispatch".to_string(),
            created_at_ms: 1,
            accepted_at_ms: 1,
            deadline_at_ms: 2,
            expires_at_ms: 3,
            arguments: json!({"outputDir":staging,"finalOutputDir":destination,"outputName":"result.png"}),
            arguments_fingerprint: "a".repeat(64),
        };
        let volume = volume_key(&root)?;
        let requested = product_reservation("image", 0, 0)?;
        let snapshot = reservation_snapshot_for(&[intent], &root, &volume, &[])?;
        assert_eq!(snapshot.pending_managed_bytes, requested.output_bytes);
        assert_eq!(
            snapshot.pending_volume_bytes.get(&volume).copied(),
            Some(requested.output_bytes * 2)
        );
        assert!(
            snapshot
                .protected_paths
                .contains(&saved_path_key(&destination.join("result.png")).unwrap())
        );
        Ok::<(), String>(())
    })();
    std::fs::remove_dir_all(&root).unwrap();
    result.expect("An unavailable destination discarded or blocked saved reservations");
}
